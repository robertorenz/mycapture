//! Clipboard helpers. One long-lived context so content survives on X11.
use clipboard_rs::{common::RustImage, Clipboard, ClipboardContext, RustImageData};
use std::sync::{Mutex, OnceLock};

static CTX: OnceLock<Mutex<ClipboardContext>> = OnceLock::new();

fn with<T>(f: impl FnOnce(&ClipboardContext) -> Result<T, String>) -> Result<T, String> {
    if CTX.get().is_none() {
        let ctx = ClipboardContext::new().map_err(|e| e.to_string())?;
        let _ = CTX.set(Mutex::new(ctx));
    }
    let guard = CTX.get().unwrap().lock().map_err(|e| e.to_string())?;
    f(&guard)
}

pub fn image(path: &std::path::Path) -> Result<(), String> {
    let img = RustImageData::from_path(&path.to_string_lossy()).map_err(|e| e.to_string())?;
    with(|c| c.set_image(img).map_err(|e| e.to_string()))
}

pub fn file(path: &std::path::Path) -> Result<(), String> {
    let p = path.to_string_lossy().to_string();
    with(|c| c.set_files(vec![p]).map_err(|e| e.to_string()))
}

pub fn text(s: &str) -> Result<(), String> {
    with(|c| c.set_text(s.to_string()).map_err(|e| e.to_string()))
}
