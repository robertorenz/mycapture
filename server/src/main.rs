//! mycapture-server: a tiny self-hosted place to put myCapture captures.
//!
//! Environment:
//!   MYCAPTURE_TOKEN       required; clients send `Authorization: Bearer <token>`
//!   MYCAPTURE_PUBLIC_URL  base URL people will open (default http://localhost:8080)
//!   MYCAPTURE_DATA        storage folder (default ./data)
//!   MYCAPTURE_ADDR        listen address (default 0.0.0.0:8080)
//!   MYCAPTURE_MAX_MB      upload limit in MB (default 1024)
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rand::{distr::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tower_http::services::ServeDir;

struct Config {
    token: String,
    public_url: String,
    data: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Meta {
    id: String,
    file: String,
    thumb: Option<String>,
    kind: String,
    created: u64,
}

fn env(key: &str, default: &str) -> String {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| default.to_string())
}

#[tokio::main]
async fn main() {
    let token = env("MYCAPTURE_TOKEN", "");
    if token.is_empty() {
        eprintln!("MYCAPTURE_TOKEN is not set. Pick a secret and start again, e.g. MYCAPTURE_TOKEN=change-me mycapture-server");
        std::process::exit(1);
    }
    let cfg = Arc::new(Config {
        token,
        public_url: env("MYCAPTURE_PUBLIC_URL", "http://localhost:8080").trim_end_matches('/').to_string(),
        data: PathBuf::from(env("MYCAPTURE_DATA", "data")),
    });
    std::fs::create_dir_all(&cfg.data).expect("cannot create data folder");
    let max_mb: usize = env("MYCAPTURE_MAX_MB", "1024").parse().unwrap_or(1024);
    let addr = env("MYCAPTURE_ADDR", "0.0.0.0:8080");

    let app = Router::new()
        .route("/", get(home))
        .route("/api/upload", post(upload))
        .route("/{id}", get(page))
        .nest_service("/f", ServeDir::new(&cfg.data))
        .layer(DefaultBodyLimit::max(max_mb * 1024 * 1024))
        .with_state(cfg.clone());

    println!("mycapture-server listening on {addr}, public URL {}", cfg.public_url);
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("cannot bind");
    axum::serve(listener, app).await.unwrap();
}

fn err(code: StatusCode, msg: &str) -> Response {
    (code, Json(serde_json::json!({ "error": msg }))).into_response()
}

fn safe_ext(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "png" => "png",
        "jpg" | "jpeg" => "jpg",
        "gif" => "gif",
        "webp" => "webp",
        "mp4" => "mp4",
        "webm" => "webm",
        _ => "bin",
    }
}

async fn upload(State(cfg): State<Arc<Config>>, headers: HeaderMap, mut form: Multipart) -> Response {
    let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).unwrap_or("");
    if auth.strip_prefix("Bearer ").map(str::trim) != Some(cfg.token.as_str()) {
        return err(StatusCode::UNAUTHORIZED, "bad token");
    }
    let id: String = rand::rng().sample_iter(&Alphanumeric).take(8).map(char::from).collect();
    let mut file = None;
    let mut thumb = None;
    while let Ok(Some(field)) = form.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        let fname = field.file_name().unwrap_or("upload.bin").to_string();
        let Ok(bytes) = field.bytes().await else { return err(StatusCode::BAD_REQUEST, "upload interrupted") };
        match name.as_str() {
            "file" => {
                let f = format!("{id}.{}", safe_ext(&fname));
                if tokio::fs::write(cfg.data.join(&f), &bytes).await.is_err() {
                    return err(StatusCode::INTERNAL_SERVER_ERROR, "cannot store file");
                }
                file = Some(f);
            }
            "thumb" => {
                let f = format!("{id}.thumb.{}", safe_ext(&fname));
                if tokio::fs::write(cfg.data.join(&f), &bytes).await.is_ok() {
                    thumb = Some(f);
                }
            }
            _ => {}
        }
    }
    let Some(file) = file else { return err(StatusCode::BAD_REQUEST, "missing `file` field") };
    let kind = if file.ends_with(".mp4") || file.ends_with(".webm") { "video" } else { "image" };
    let meta = Meta {
        id: id.clone(),
        file: file.clone(),
        thumb,
        kind: kind.into(),
        created: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
    };
    let _ = tokio::fs::write(cfg.data.join(format!("{id}.json")), serde_json::to_vec(&meta).unwrap()).await;
    Json(serde_json::json!({
        "url": format!("{}/{id}", cfg.public_url),
        "raw": format!("{}/f/{file}", cfg.public_url),
    }))
    .into_response()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

async fn page(State(cfg): State<Arc<Config>>, Path(id): Path<String>) -> Response {
    if !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return err(StatusCode::NOT_FOUND, "not found");
    }
    let Ok(raw) = tokio::fs::read(cfg.data.join(format!("{id}.json"))).await else {
        return err(StatusCode::NOT_FOUND, "not found");
    };
    let Ok(m) = serde_json::from_slice::<Meta>(&raw) else { return err(StatusCode::NOT_FOUND, "not found") };
    let base = &cfg.public_url;
    let src = format!("{base}/f/{}", m.file);
    let preview = match (&m.kind[..], &m.thumb) {
        ("video", Some(t)) => format!("{base}/f/{t}"),
        ("video", None) => String::new(),
        _ => src.clone(),
    };
    let (og_extra, media) = if m.kind == "video" {
        (
            format!(
                r#"<meta property="og:type" content="video.other"><meta property="og:video" content="{s}"><meta property="og:video:type" content="video/mp4">"#,
                s = esc(&src)
            ),
            format!(r#"<video src="{s}" controls autoplay playsinline poster="{p}"></video>"#, s = esc(&src), p = esc(&preview)),
        )
    } else {
        (r#"<meta property="og:type" content="website">"#.to_string(), format!(r#"<img src="{s}" alt="Capture">"#, s = esc(&src)))
    };
    let og_image = if preview.is_empty() {
        String::new()
    } else {
        format!(r#"<meta property="og:image" content="{p}"><meta name="twitter:image" content="{p}">"#, p = esc(&preview))
    };
    Html(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>myCapture capture</title><meta property="og:title" content="myCapture capture"><meta property="og:site_name" content="myCapture">
<meta name="twitter:card" content="summary_large_image">{og_image}{og_extra}
<style>
:root{{--bg:#0b1220;--panel:#111a2e;--line:#26334f;--text:#e6ebf5;--muted:#8f9bb3;--accent:#f59e0b}}
@media (prefers-color-scheme: light){{:root{{--bg:#f4f6fb;--panel:#fff;--line:#dbe1ec;--text:#0f172a;--muted:#5b6780}}}}
*{{box-sizing:border-box}}body{{margin:0;min-height:100vh;background:var(--bg);color:var(--text);font:15px/1.5 "Segoe UI",-apple-system,Inter,sans-serif;display:flex;flex-direction:column}}
header{{display:flex;align-items:center;gap:10px;padding:14px 18px;border-bottom:1px solid var(--line);background:var(--panel)}}
header b{{font-size:16px}}.dot{{width:22px;height:22px;border-radius:50%;background:radial-gradient(circle at 40% 35%,#ffe27a,#f59e0b 70%);box-shadow:0 0 12px rgba(245,158,11,.6)}}
header a{{margin-left:auto;color:var(--text);text-decoration:none;border:1px solid var(--line);padding:7px 14px;border-radius:8px;font-weight:600}}
header a:hover{{border-color:var(--accent);color:var(--accent)}}
main{{flex:1;display:grid;place-items:center;padding:16px}}
img,video{{max-width:100%;max-height:calc(100vh - 110px);border-radius:8px;box-shadow:0 12px 40px rgba(0,0,0,.35);background:#000}}
</style></head><body><header><span class="dot"></span><b>myCapture</b><a href="{s}" download>Download</a></header><main>{media}</main></body></html>"#,
        s = esc(&src)
    ))
    .into_response()
}

async fn home() -> Html<&'static str> {
    Html("<!doctype html><title>myCapture server</title><body style=\"font-family:sans-serif;background:#0b1220;color:#e6ebf5;display:grid;place-items:center;height:100vh;margin:0\"><p>myCapture server is running.</p></body>")
}
