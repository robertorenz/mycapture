//! Dropbox, OneDrive and Google Drive: browser sign-in (OAuth with PKCE and a loopback
//! redirect), token refresh, uploads into a `myCapture` folder and public share links.
use crate::settings::{config_dir, UploadSettings};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Fixed so it can be registered as the redirect URI (Dropbox needs the exact port).
const REDIRECT_PORT: u16 = 53682;
const FOLDER: &str = "myCapture";
const MB: usize = 1024 * 1024;

#[derive(Clone, Copy, PartialEq)]
pub enum Provider {
    Dropbox,
    OneDrive,
    GDrive,
}

pub const ALL: [Provider; 3] = [Provider::Dropbox, Provider::OneDrive, Provider::GDrive];

impl Provider {
    pub fn from_id(id: &str) -> Option<Self> {
        ALL.into_iter().find(|p| p.id() == id)
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Dropbox => "dropbox",
            Self::OneDrive => "onedrive",
            Self::GDrive => "gdrive",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Dropbox => "Dropbox",
            Self::OneDrive => "OneDrive",
            Self::GDrive => "Google Drive",
        }
    }

    fn auth_url(self) -> &'static str {
        match self {
            Self::Dropbox => "https://www.dropbox.com/oauth2/authorize",
            Self::OneDrive => "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            Self::GDrive => "https://accounts.google.com/o/oauth2/v2/auth",
        }
    }

    fn token_url(self) -> &'static str {
        match self {
            Self::Dropbox => "https://api.dropboxapi.com/oauth2/token",
            Self::OneDrive => "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            Self::GDrive => "https://oauth2.googleapis.com/token",
        }
    }

    fn scope(self) -> &'static str {
        match self {
            Self::Dropbox => "account_info.read files.content.write sharing.write",
            Self::OneDrive => "offline_access Files.ReadWrite User.Read",
            Self::GDrive => "https://www.googleapis.com/auth/drive.file",
        }
    }

    /// App credentials compiled into this build (set when building the release).
    fn builtin(self) -> (Option<&'static str>, Option<&'static str>) {
        match self {
            Self::Dropbox => (option_env!("MYCAPTURE_DROPBOX_KEY"), None),
            Self::OneDrive => (option_env!("MYCAPTURE_ONEDRIVE_CLIENT_ID"), None),
            Self::GDrive => (option_env!("MYCAPTURE_GDRIVE_CLIENT_ID"), option_env!("MYCAPTURE_GDRIVE_SECRET")),
        }
    }

    pub fn has_builtin(self) -> bool {
        self.builtin().0.is_some_and(|k| !k.is_empty())
    }
}

/// Client id (and, for Google, the non-confidential desktop secret): the user's own, else the built-in one.
fn app_keys(p: Provider, cfg: &UploadSettings) -> Result<(String, Option<String>), String> {
    let (own_id, own_secret) = match p {
        Provider::Dropbox => (cfg.dropbox_app_key.trim(), ""),
        Provider::OneDrive => (cfg.onedrive_client_id.trim(), ""),
        Provider::GDrive => (cfg.gdrive_client_id.trim(), cfg.gdrive_client_secret.trim()),
    };
    let (bid, bsecret) = p.builtin();
    let (id, secret) = if own_id.is_empty() { (bid.unwrap_or(""), bsecret.unwrap_or("")) } else { (own_id, own_secret) };
    if id.is_empty() {
        return Err(tr!("{} needs an app key first. In Settings → Sharing → {}, open “Use your own {} app” and follow the steps.", p.name(), p.name(), p.name()));
    }
    Ok((id.to_string(), (!secret.is_empty()).then(|| secret.to_string())))
}

// ---------- stored sign-ins (kept out of settings.json so the settings page never sees them) ----------

#[derive(Clone, Serialize, Deserialize)]
struct Account {
    name: String,
    refresh_token: String,
    /// The client that issued the token; refreshing needs the same one.
    client_id: String,
    client_secret: Option<String>,
}

fn store_path() -> std::path::PathBuf {
    config_dir().join("cloud.json")
}

fn load() -> HashMap<String, Account> {
    std::fs::read_to_string(store_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save(map: &HashMap<String, Account>) -> Result<(), String> {
    let s = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    std::fs::write(store_path(), s).map_err(|e| e.to_string())
}

/// Connected account name per provider id.
pub fn accounts() -> HashMap<String, String> {
    load().into_iter().map(|(k, a)| (k, a.name)).collect()
}

pub fn disconnect(p: Provider) -> Result<(), String> {
    let mut map = load();
    map.remove(p.id());
    save(&map)
}

// ---------- sign-in ----------

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn redirect_uri() -> String {
    format!("http://localhost:{REDIRECT_PORT}/")
}

/// Open the provider's sign-in page in the browser, wait for the redirect, and keep the refresh token.
/// Returns the account name.
pub async fn connect(p: Provider, cfg: &UploadSettings, open: impl Fn(&str) -> Result<(), String>) -> Result<String, String> {
    let (client_id, secret) = app_keys(p, cfg)?;
    let verifier = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
    let state = uuid::Uuid::new_v4().simple().to_string();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", REDIRECT_PORT))
        .await
        .map_err(|_| tr!("Port {} is busy, so the sign-in can't come back to myCapture. Close the other sign-in and try again.", REDIRECT_PORT))?;

    let redirect = redirect_uri();
    let mut params = vec![
        ("client_id", client_id.as_str()),
        ("response_type", "code"),
        ("redirect_uri", redirect.as_str()),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("state", state.as_str()),
        ("scope", p.scope()),
    ];
    match p {
        Provider::Dropbox => params.push(("token_access_type", "offline")),
        Provider::GDrive => params.extend([("access_type", "offline"), ("prompt", "consent")]),
        Provider::OneDrive => params.push(("prompt", "select_account")),
    }
    let url = reqwest::Url::parse_with_params(p.auth_url(), &params).map_err(|e| e.to_string())?;
    open(url.as_str())?;

    let code = tokio::time::timeout(Duration::from_secs(300), wait_for_code(&listener, &state, p))
        .await
        .map_err(|_| tr!("Sign-in timed out. Click Connect to try again."))??;

    let mut form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code),
        ("client_id", client_id.clone()),
        ("code_verifier", verifier),
        ("redirect_uri", redirect),
    ];
    if let Some(s) = &secret {
        form.push(("client_secret", s.clone()));
    }
    if p == Provider::OneDrive {
        form.push(("scope", p.scope().to_string()));
    }
    let tok = token_request(p, &form).await?;
    let access = tok["access_token"].as_str().ok_or_else(|| tr!("The sign-in returned no access token."))?.to_string();
    let refresh = tok["refresh_token"].as_str().ok_or_else(|| tr!("The sign-in returned no refresh token."))?.to_string();
    let name = account_name(p, &access).await.unwrap_or_else(|_| p.name().to_string());
    let mut map = load();
    map.insert(p.id().into(), Account { name: name.clone(), refresh_token: refresh, client_id, client_secret: secret });
    save(&map)?;
    Ok(name)
}

/// Serve the loopback redirect until the browser comes back with our `state`.
async fn wait_for_code(listener: &tokio::net::TcpListener, state: &str, p: Provider) -> Result<String, String> {
    loop {
        let (mut sock, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 8192];
        let n = sock.read(&mut buf).await.unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]);
        let target = req.split_whitespace().nth(1).unwrap_or("/");
        let Ok(url) = reqwest::Url::parse(&format!("http://localhost{target}")) else { continue };
        let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
        if q.get("state").map(String::as_str) != Some(state) {
            let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            continue;
        }
        let result = match (q.get("code"), q.get("error")) {
            (Some(code), _) => Ok(code.clone()),
            (_, err) => Err(q.get("error_description").or(err).cloned().unwrap_or_else(|| tr!("Sign-in was cancelled."))),
        };
        let page = done_page(p, result.as_ref().err().map(String::as_str));
        let head = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", page.len());
        let _ = sock.write_all(head.as_bytes()).await;
        let _ = sock.write_all(page.as_bytes()).await;
        return result.map_err(|e| tr!("{} sign-in failed: {}", p.name(), e));
    }
}

fn done_page(p: Provider, error: Option<&str>) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let (title, body, color) = match error {
        None => (tr!("{} is connected", p.name()), tr!("You can close this tab and go back to myCapture."), "#22c55e"),
        Some(e) => (tr!("Sign-in didn't finish"), esc(e), "#f87171"),
    };
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>myCapture</title><style>
body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#0b1220;color:#e6ebf5;font:15px "Segoe UI",-apple-system,sans-serif}}
.card{{background:#121b2e;border:1px solid #23304a;border-radius:16px;padding:36px 44px;text-align:center;max-width:440px;box-shadow:0 20px 60px rgba(0,0,0,.45)}}
.dot{{width:14px;height:14px;border-radius:50%;background:{color};margin:0 auto 18px;box-shadow:0 0 0 6px {color}22}}
h1{{font-size:20px;margin:0 0 10px}}p{{margin:0;color:#94a3b8;line-height:1.5}}b{{color:#f59e0b}}
</style></head><body><div class="card"><div class="dot"></div><h1>{title}</h1><p>{body}</p><p style="margin-top:18px"><b>myCapture</b></p></div></body></html>"#
    )
}

async fn token_request(p: Provider, form: &[(&str, String)]) -> Result<Value, String> {
    let res = reqwest::Client::new().post(p.token_url()).form(form).send().await.map_err(|e| e.to_string())?;
    json_ok(res, &tr!("{} sign-in", p.name())).await
}

async fn json_ok(res: reqwest::Response, what: &str) -> Result<Value, String> {
    let status = res.status();
    let body = res.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        let detail: String = body.chars().take(400).collect();
        return Err(tr!("{} failed ({}): {}", what, status, detail));
    }
    if body.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&body).map_err(|e| tr!("{}: unexpected reply ({})", what, e))
}

/// A fresh access token for the stored sign-in (rotating the refresh token when the provider issues a new one).
async fn access_token(p: Provider) -> Result<String, String> {
    let mut map = load();
    let acct = map.get(p.id()).cloned().ok_or_else(|| tr!("{} isn't connected. Open Settings → Sharing → {} and click Connect.", p.name(), p.name()))?;
    let mut form = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", acct.refresh_token.clone()),
        ("client_id", acct.client_id.clone()),
    ];
    if let Some(s) = &acct.client_secret {
        form.push(("client_secret", s.clone()));
    }
    if p == Provider::OneDrive {
        form.push(("scope", p.scope().to_string()));
    }
    let tok = token_request(p, &form)
        .await
        .map_err(|e| tr!("{}\n\nIf you removed myCapture's access, connect {} again in Settings.", e, p.name()))?;
    if let Some(r) = tok["refresh_token"].as_str().filter(|r| *r != acct.refresh_token) {
        map.insert(p.id().into(), Account { refresh_token: r.to_string(), ..acct });
        save(&map)?;
    }
    tok["access_token"].as_str().map(String::from).ok_or_else(|| tr!("No access token in the reply."))
}

async fn account_name(p: Provider, token: &str) -> Result<String, String> {
    let c = reqwest::Client::new();
    let v = match p {
        Provider::Dropbox => c.post("https://api.dropboxapi.com/2/users/get_current_account").bearer_auth(token).send().await,
        Provider::OneDrive => c.get("https://graph.microsoft.com/v1.0/me").bearer_auth(token).send().await,
        Provider::GDrive => c.get("https://www.googleapis.com/drive/v3/about?fields=user").bearer_auth(token).send().await,
    }
    .map_err(|e| e.to_string())?;
    let v = json_ok(v, &tr!("Account lookup")).await?;
    let pick = |keys: &[&str]| keys.iter().find_map(|k| v.pointer(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from));
    match p {
        Provider::Dropbox => pick(&["/email", "/name/display_name"]),
        Provider::OneDrive => pick(&["/mail", "/userPrincipalName", "/displayName"]),
        Provider::GDrive => pick(&["/user/emailAddress", "/user/displayName"]),
    }
    .ok_or_else(|| "no account name".into())
}

// ---------- uploads ----------

/// Upload into the `myCapture` folder and return a link anyone can open.
pub async fn upload(p: Provider, file: &Path, mime: &str) -> Result<String, String> {
    let token = access_token(p).await?;
    let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "capture".into());
    let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
    match p {
        Provider::Dropbox => dropbox_upload(&token, &name, bytes).await,
        Provider::OneDrive => onedrive_upload(&token, &name, bytes).await,
        Provider::GDrive => gdrive_upload(&token, &name, mime, bytes).await,
    }
}

async fn dropbox_upload(token: &str, name: &str, bytes: Vec<u8>) -> Result<String, String> {
    let c = reqwest::Client::new();
    let commit = json!({ "path": format!("/{FOLDER}/{name}"), "mode": "add", "autorename": true, "mute": true });
    let call = |endpoint: &str, arg: Value, body: Vec<u8>| {
        c.post(format!("https://content.dropboxapi.com/2/files/{endpoint}"))
            .bearer_auth(token)
            .header("Dropbox-API-Arg", arg.to_string())
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .send()
    };
    // Single requests are limited to 150 MB; larger files go through an upload session.
    const CHUNK: usize = 64 * MB;
    let meta = if bytes.len() <= CHUNK {
        json_ok(call("upload", commit, bytes).await.map_err(|e| e.to_string())?, &tr!("Dropbox upload")).await?
    } else {
        let mut chunks = bytes.chunks(CHUNK);
        let first = chunks.next().unwrap_or_default().to_vec();
        let start = json_ok(call("upload_session/start", json!({ "close": false }), first.clone()).await.map_err(|e| e.to_string())?, &tr!("Dropbox upload")).await?;
        let id = start["session_id"].as_str().ok_or_else(|| tr!("Dropbox gave no upload session."))?.to_string();
        let mut offset = first.len();
        let rest: Vec<&[u8]> = chunks.collect();
        let (last, middle) = rest.split_last().ok_or_else(|| tr!("Dropbox upload: empty file"))?;
        for part in middle {
            let arg = json!({ "cursor": { "session_id": id, "offset": offset }, "close": false });
            json_ok(call("upload_session/append_v2", arg, part.to_vec()).await.map_err(|e| e.to_string())?, &tr!("Dropbox upload")).await?;
            offset += part.len();
        }
        let arg = json!({ "cursor": { "session_id": id, "offset": offset }, "commit": commit });
        json_ok(call("upload_session/finish", arg, last.to_vec()).await.map_err(|e| e.to_string())?, &tr!("Dropbox upload")).await?
    };
    let path = meta["path_display"].as_str().ok_or_else(|| tr!("Dropbox did not say where it saved the file."))?;
    let res = c
        .post("https://api.dropboxapi.com/2/sharing/create_shared_link_with_settings")
        .bearer_auth(token)
        .json(&json!({ "path": path }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if res.status().as_u16() == 409 {
        // The file already has a link: reuse it.
        let v: Value = res.json().await.map_err(|e| e.to_string())?;
        if let Some(url) = v.pointer("/error/shared_link_already_exists/metadata/url").and_then(Value::as_str) {
            return Ok(url.to_string());
        }
        return Err(tr!("Dropbox could not make a share link: {}", v));
    }
    let v = json_ok(res, &tr!("Dropbox share link")).await?;
    v["url"].as_str().map(String::from).ok_or_else(|| tr!("Dropbox returned no link."))
}

async fn onedrive_upload(token: &str, name: &str, bytes: Vec<u8>) -> Result<String, String> {
    let c = reqwest::Client::new();
    let graph = "https://graph.microsoft.com/v1.0/me/drive";
    let session = c
        .post(format!("{graph}/root:/{FOLDER}/{name}:/createUploadSession"))
        .bearer_auth(token)
        .json(&json!({ "item": { "@microsoft.graph.conflictBehavior": "rename" } }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let session = json_ok(session, &tr!("OneDrive upload")).await?;
    let upload_url = session["uploadUrl"].as_str().ok_or_else(|| tr!("OneDrive gave no upload address."))?;
    // Chunks must be multiples of 320 KiB.
    const CHUNK: usize = 32 * 320 * 1024;
    let total = bytes.len();
    let mut item = Value::Null;
    for (i, part) in bytes.chunks(CHUNK).enumerate() {
        let start = i * CHUNK;
        let end = start + part.len() - 1;
        let res = c
            .put(upload_url)
            .header("Content-Range", format!("bytes {start}-{end}/{total}"))
            .body(part.to_vec())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        item = json_ok(res, &tr!("OneDrive upload")).await?;
    }
    let id = item["id"].as_str().ok_or_else(|| tr!("OneDrive did not confirm the upload."))?;
    // Work and school accounts may not allow anonymous links; fall back to an organization link.
    let mut last_err = String::new();
    for scope in ["anonymous", "organization"] {
        let res = c
            .post(format!("{graph}/items/{id}/createLink"))
            .bearer_auth(token)
            .json(&json!({ "type": "view", "scope": scope }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        match json_ok(res, &tr!("OneDrive share link")).await {
            Ok(v) => return v.pointer("/link/webUrl").and_then(Value::as_str).map(String::from).ok_or_else(|| tr!("OneDrive returned no link.")),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

async fn gdrive_upload(token: &str, name: &str, mime: &str, bytes: Vec<u8>) -> Result<String, String> {
    let c = reqwest::Client::new();
    let api = "https://www.googleapis.com/drive/v3/files";
    // With the drive.file scope only folders this app made are visible, so this finds our own.
    let q = format!("name = '{FOLDER}' and mimeType = 'application/vnd.google-apps.folder' and trashed = false");
    let found = c.get(api).bearer_auth(token).query(&[("q", q.as_str()), ("fields", "files(id)"), ("spaces", "drive")]).send().await.map_err(|e| e.to_string())?;
    let found = json_ok(found, "Google Drive").await?;
    let folder = match found.pointer("/files/0/id").and_then(Value::as_str) {
        Some(id) => id.to_string(),
        None => {
            let made = c
                .post(api)
                .bearer_auth(token)
                .json(&json!({ "name": FOLDER, "mimeType": "application/vnd.google-apps.folder" }))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            json_ok(made, &tr!("Google Drive folder")).await?["id"].as_str().ok_or_else(|| tr!("Google Drive did not create the folder."))?.to_string()
        }
    };
    let start = c
        .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=resumable&fields=id,webViewLink")
        .bearer_auth(token)
        .header("X-Upload-Content-Type", mime)
        .json(&json!({ "name": name, "parents": [folder] }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !start.status().is_success() {
        return Err(json_ok(start, &tr!("Google Drive upload")).await.err().unwrap_or_default());
    }
    let location = start.headers().get("location").and_then(|v| v.to_str().ok()).ok_or_else(|| tr!("Google Drive gave no upload address."))?.to_string();
    let put = c.put(location).header("Content-Type", mime).body(bytes).send().await.map_err(|e| e.to_string())?;
    let file = json_ok(put, &tr!("Google Drive upload")).await?;
    let id = file["id"].as_str().ok_or_else(|| tr!("Google Drive did not confirm the upload."))?;
    let share = c
        .post(format!("{api}/{id}/permissions"))
        .bearer_auth(token)
        .json(&json!({ "role": "reader", "type": "anyone" }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    json_ok(share, &tr!("Google Drive share link")).await?;
    Ok(file["webViewLink"].as_str().map(String::from).unwrap_or_else(|| format!("https://drive.google.com/file/d/{id}/view")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_matches_rfc7636_example() {
        let challenge = b64url(&Sha256::digest(b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"));
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    /// The loopback redirect ignores strays (favicon, wrong state) and returns the code.
    #[test]
    fn loopback_redirect_returns_code() {
        tauri::async_runtime::block_on(async {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let browser = tokio::spawn(async move {
                let get = |path: &str| {
                    let path = path.to_string();
                    async move {
                        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
                        s.write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes()).await.unwrap();
                        let mut out = String::new();
                        s.read_to_string(&mut out).await.unwrap();
                        out
                    }
                };
                let stray = get("/favicon.ico").await;
                let page = get("/?code=abc123&state=s1").await;
                (stray, page)
            });
            let code = wait_for_code(&listener, "s1", Provider::Dropbox).await.unwrap();
            let (stray, page) = browser.await.unwrap();
            assert_eq!(code, "abc123");
            assert!(stray.starts_with("HTTP/1.1 404"));
            assert!(page.contains("Dropbox is connected"));
        });
    }

    #[test]
    fn missing_key_explains_setup() {
        let err = app_keys(Provider::Dropbox, &UploadSettings::default());
        if !Provider::Dropbox.has_builtin() {
            assert!(err.unwrap_err().contains("Use your own Dropbox app"));
        }
    }
}
