use crate::settings::UploadSettings;
use std::path::Path;

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

fn file_part(path: &Path) -> Result<reqwest::multipart::Part, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    reqwest::multipart::Part::bytes(bytes).file_name(name).mime_str(mime_for(path)).map_err(|e| e.to_string())
}

fn json_path(v: &serde_json::Value, path: &str) -> Option<String> {
    let mut cur = v;
    for key in path.split('.').filter(|k| !k.is_empty()) {
        cur = match key.parse::<usize>() {
            Ok(i) => cur.get(i)?,
            Err(_) => cur.get(key)?,
        };
    }
    cur.as_str().map(|s| s.to_string())
}

/// Every destination, in menu order.
pub const DESTINATIONS: [(&str, &str); 6] = [
    ("dropbox", "Dropbox"),
    ("onedrive", "OneDrive"),
    ("gdrive", "Google Drive"),
    ("server", "myCapture server"),
    ("s3", "S3 storage"),
    ("http", "Custom upload"),
];

/// Whether a destination has what it needs to upload; `account` is its cloud sign-in, if any.
pub fn configured(cfg: &UploadSettings, id: &str, account: Option<&String>) -> bool {
    let set = |v: &str| !v.trim().is_empty();
    match id {
        "server" => set(&cfg.server_url),
        "s3" => set(&cfg.s3_endpoint) && set(&cfg.s3_bucket) && set(&cfg.s3_access_key),
        "http" => set(&cfg.http_url),
        _ => account.is_some(),
    }
}

/// Upload a capture to `provider` and return the shareable URL.
pub async fn upload(cfg: &UploadSettings, provider: &str, file: &Path, thumb: Option<&Path>) -> Result<String, String> {
    match provider {
        "server" => upload_server(cfg, file, thumb).await,
        "http" => upload_http(cfg, file).await,
        "s3" => upload_s3(cfg, file).await,
        id => match crate::cloud::Provider::from_id(id) {
            Some(p) => crate::cloud::upload(p, file, mime_for(file)).await,
            None => Err("No upload destination is configured yet. Open Settings → Sharing to pick one.".into()),
        },
    }
}

async fn upload_server(cfg: &UploadSettings, file: &Path, thumb: Option<&Path>) -> Result<String, String> {
    let base = cfg.server_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("Set your myCapture server URL in Settings → Sharing.".into());
    }
    let mut form = reqwest::multipart::Form::new().part("file", file_part(file)?);
    if let Some(t) = thumb.filter(|t| t.exists()) {
        form = form.part("thumb", file_part(t)?);
    }
    let res = reqwest::Client::new()
        .post(format!("{base}/api/upload"))
        .bearer_auth(cfg.server_token.trim())
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Upload failed: {e}"))?;
    let status = res.status();
    let body = res.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("Server said {status}: {body}"));
    }
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    json_path(&v, "url").ok_or_else(|| "Server response had no url".into())
}

async fn upload_http(cfg: &UploadSettings, file: &Path) -> Result<String, String> {
    if cfg.http_url.trim().is_empty() {
        return Err("Set the upload URL in Settings → Sharing.".into());
    }
    let field = if cfg.http_field.trim().is_empty() { "file".to_string() } else { cfg.http_field.trim().to_string() };
    let form = reqwest::multipart::Form::new().part(field, file_part(file)?);
    let mut req = reqwest::Client::new().post(cfg.http_url.trim()).multipart(form);
    for line in cfg.http_headers.lines() {
        if let Some((k, v)) = line.split_once(':') {
            req = req.header(k.trim(), v.trim());
        }
    }
    let res = req.send().await.map_err(|e| format!("Upload failed: {e}"))?;
    let status = res.status();
    let body = res.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("Server said {status}: {body}"));
    }
    if cfg.http_url_path.trim().is_empty() {
        return Ok(body.trim().to_string());
    }
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    json_path(&v, cfg.http_url_path.trim()).ok_or_else(|| format!("No `{}` in response: {body}", cfg.http_url_path))
}

async fn upload_s3(cfg: &UploadSettings, file: &Path) -> Result<String, String> {
    use s3::{creds::Credentials, Bucket, Region};
    let region = Region::Custom { region: cfg.s3_region.trim().to_string(), endpoint: cfg.s3_endpoint.trim().to_string() };
    let creds = Credentials::new(Some(cfg.s3_access_key.trim()), Some(cfg.s3_secret_key.trim()), None, None, None)
        .map_err(|e| e.to_string())?;
    let mut bucket = Bucket::new(cfg.s3_bucket.trim(), region, creds).map_err(|e| e.to_string())?;
    if cfg.s3_path_style {
        bucket = bucket.with_path_style();
    }
    let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let key = format!("{}{}", cfg.s3_prefix.trim(), name);
    let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
    bucket
        .put_object_with_content_type(&key, &bytes, mime_for(file))
        .await
        .map_err(|e| format!("Upload failed: {e}"))?;
    if cfg.s3_public_url.trim().is_empty() {
        bucket.presign_get(&key, 7 * 24 * 3600, None).await.map_err(|e| e.to_string())
    } else {
        Ok(format!("{}/{}", cfg.s3_public_url.trim().trim_end_matches('/'), key))
    }
}
