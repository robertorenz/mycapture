use crate::settings::captures_dir;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    /// "image" | "video"
    pub kind: String,
    pub file: String,
    pub thumb: String,
    pub created: DateTime<Local>,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub duration: f64,
    /// Most recent share link (shown on the card and in the preview).
    #[serde(default)]
    pub url: Option<String>,
    /// Share link per destination (`UploadSettings::provider` ids), so each upload happens once.
    #[serde(default)]
    pub links: std::collections::BTreeMap<String, String>,
}

impl Item {
    pub fn path(&self) -> PathBuf {
        captures_dir().join(&self.file)
    }
    pub fn thumb_path(&self) -> PathBuf {
        captures_dir().join(&self.thumb)
    }
}

fn index_path() -> PathBuf {
    captures_dir().join("index.json")
}

pub fn list() -> Vec<Item> {
    std::fs::read_to_string(index_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(items: &[Item]) -> Result<(), String> {
    let s = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
    std::fs::write(index_path(), s).map_err(|e| e.to_string())
}

pub fn get(id: &str) -> Result<Item, String> {
    list().into_iter().find(|i| i.id == id).ok_or_else(|| "Capture not found".to_string())
}

pub fn add(item: Item) -> Result<(), String> {
    let mut items = list();
    items.insert(0, item);
    save(&items)
}

pub fn update(item: &Item) -> Result<(), String> {
    let mut items = list();
    if let Some(i) = items.iter_mut().find(|i| i.id == item.id) {
        *i = item.clone();
    }
    save(&items)
}

pub fn remove(id: &str) -> Result<(), String> {
    let mut items = list();
    if let Some(pos) = items.iter().position(|i| i.id == id) {
        let it = items.remove(pos);
        let _ = std::fs::remove_file(it.path());
        let _ = std::fs::remove_file(it.thumb_path());
    }
    save(&items)
}

/// New capture id / base filename, e.g. `2026-10-17_22-53-04_ab12`.
pub fn new_id() -> String {
    let short = uuid::Uuid::new_v4().simple().to_string();
    format!("{}_{}", Local::now().format("%Y-%m-%d_%H-%M-%S"), &short[..4])
}

pub fn make_image_thumb(img: &image::RgbaImage, path: &std::path::Path) -> Result<(), String> {
    let w = 360u32.min(img.width().max(1));
    let h = ((img.height() as f64) * (w as f64) / (img.width().max(1) as f64)).max(1.0) as u32;
    let t = image::imageops::thumbnail(img, w, h);
    image::DynamicImage::ImageRgba8(t)
        .to_rgb8()
        .save_with_format(path, image::ImageFormat::Jpeg)
        .map_err(|e| e.to_string())
}
