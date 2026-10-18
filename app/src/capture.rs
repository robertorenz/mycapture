//! Screenshot capture: freeze the monitor under the cursor, let the user pick a region.
use crate::{history, settings::captures_dir, AppState};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

pub struct Pending {
    pub image: image::RgbaImage,
    /// Fast-encoded PNG of `image`, built lazily for the overlay background.
    pub frame_png: Mutex<Option<Vec<u8>>>,
    /// Monitor origin in xcap units (physical on Windows/Linux, points on macOS).
    pub mon_x: i32,
    pub mon_y: i32,
    /// Image pixels per xcap unit.
    pub ratio: f64,
    /// Index of the monitor in `xcap::Monitor::all()` (used for avfoundation screen numbers).
    pub screen_index: usize,
    /// Tauri monitor origin (physical) and image-pixel → physical-pixel factor, for placing windows.
    pub win_origin: (i32, i32),
    pub win_k: f64,
    pub windows: Vec<Rect>,
}

impl Pending {
    /// Convert an image-space rect to a physical window rect.
    pub fn to_physical(&self, r: Rect) -> Rect {
        Rect {
            x: self.win_origin.0 + (r.x as f64 * self.win_k) as i32,
            y: self.win_origin.1 + (r.y as f64 * self.win_k) as i32,
            w: (r.w as f64 * self.win_k) as i32,
            h: (r.h as f64 * self.win_k) as i32,
        }
    }
}

#[derive(Serialize)]
pub struct OverlayInfo {
    pub width: u32,
    pub height: u32,
    pub windows: Vec<Rect>,
}

fn clip(r: Rect, w: u32, h: u32) -> Option<Rect> {
    let x0 = r.x.max(0);
    let y0 = r.y.max(0);
    let x1 = (r.x + r.w).min(w as i32);
    let y1 = (r.y + r.h).min(h as i32);
    (x1 - x0 >= 8 && y1 - y0 >= 8).then_some(Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
}

pub fn grab(app: &AppHandle) -> Result<Pending, String> {
    let cursor = app.cursor_position().unwrap_or(PhysicalPosition::new(0.0, 0.0));
    let tmon = app
        .monitor_from_point(cursor.x, cursor.y)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or("No monitor found")?;
    let tpos = *tmon.position();
    let tsize = *tmon.size();
    let tscale = tmon.scale_factor();

    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    // Match xcap's monitor to Tauri's (xcap may report points on macOS, pixels elsewhere).
    let (screen_index, mon) = monitors
        .iter()
        .enumerate()
        .min_by_key(|(_, m)| {
            let (mx, my) = (m.x().unwrap_or(0) as f64, m.y().unwrap_or(0) as f64);
            let d1 = (mx - tpos.x as f64).abs() + (my - tpos.y as f64).abs();
            let d2 = (mx - tpos.x as f64 / tscale).abs() + (my - tpos.y as f64 / tscale).abs();
            d1.min(d2) as i64
        })
        .ok_or("No monitor found")?;
    let image = mon.capture_image().map_err(|e| e.to_string())?;
    let mon_x = mon.x().unwrap_or(0);
    let mon_y = mon.y().unwrap_or(0);
    let ratio = image.width() as f64 / mon.width().unwrap_or(image.width()).max(1) as f64;

    let me = std::process::id();
    let mut windows = Vec::new();
    if let Ok(all) = xcap::Window::all() {
        for w in all {
            if w.pid().ok() == Some(me) || w.is_minimized().unwrap_or(true) {
                continue;
            }
            let (Ok(x), Ok(y), Ok(ww), Ok(wh)) = (w.x(), w.y(), w.width(), w.height()) else { continue };
            let r = Rect {
                x: ((x - mon_x) as f64 * ratio) as i32,
                y: ((y - mon_y) as f64 * ratio) as i32,
                w: (ww as f64 * ratio) as i32,
                h: (wh as f64 * ratio) as i32,
            };
            if let Some(r) = clip(r, image.width(), image.height()) {
                windows.push(r);
            }
        }
    }

    Ok(Pending {
        win_origin: (tpos.x, tpos.y),
        win_k: tsize.width as f64 / image.width().max(1) as f64,
        image,
        frame_png: Mutex::new(None),
        mon_x,
        mon_y,
        ratio,
        screen_index,
        windows,
    })
}

pub fn encode_fast_png(img: &image::RgbaImage) -> Vec<u8> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;
    let mut out = Vec::new();
    let enc = PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::NoFilter);
    let _ = enc.write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8);
    out
}

/// Hide the sun, freeze the screen and open the selection overlay.
pub fn begin(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.rec.lock().unwrap().is_some() {
        return Err("A recording is in progress.".into());
    }
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_focus();
        return Ok(());
    }
    if let Some(sun) = app.get_webview_window("sun") {
        let _ = sun.hide();
    }
    std::thread::sleep(std::time::Duration::from_millis(180));
    let pending = grab(app)?;
    let (ox, oy) = pending.win_origin;
    let size = PhysicalSize::new(
        (pending.image.width() as f64 * pending.win_k) as u32,
        (pending.image.height() as f64 * pending.win_k) as u32,
    );
    *state.pending.lock().unwrap() = Some(pending);

    let win = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
        .title("myCapture capture")
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.set_position(PhysicalPosition::new(ox, oy));
    let _ = win.set_size(size);
    // overlay.html shows itself once the frozen frame has been painted.
    Ok(())
}

pub fn info(app: &AppHandle) -> Result<OverlayInfo, String> {
    let state = app.state::<AppState>();
    let guard = state.pending.lock().unwrap();
    let p = guard.as_ref().ok_or("No capture in progress")?;
    Ok(OverlayInfo { width: p.image.width(), height: p.image.height(), windows: p.windows.clone() })
}

pub fn frame_png(app: &AppHandle) -> Option<Vec<u8>> {
    let state = app.state::<AppState>();
    let guard = state.pending.lock().unwrap();
    let p = guard.as_ref()?;
    let mut cache = p.frame_png.lock().unwrap();
    if cache.is_none() {
        *cache = Some(encode_fast_png(&p.image));
    }
    cache.clone()
}

pub fn close_overlay(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.destroy();
    }
}

/// Crop the frozen frame, store it in history and return the new item.
pub fn finish_image(app: &AppHandle, rect: Rect) -> Result<history::Item, String> {
    let state = app.state::<AppState>();
    let pending = state.pending.lock().unwrap().take().ok_or("No capture in progress")?;
    close_overlay(app);
    let r = clip(rect, pending.image.width(), pending.image.height()).ok_or("Selection is too small")?;
    let crop = image::imageops::crop_imm(&pending.image, r.x as u32, r.y as u32, r.w as u32, r.h as u32).to_image();
    let id = history::new_id();
    let item = history::Item {
        file: format!("{id}.png"),
        thumb: format!("{id}.thumb.jpg"),
        id,
        kind: "image".into(),
        created: chrono::Local::now(),
        width: crop.width(),
        height: crop.height(),
        duration: 0.0,
        url: None,
    };
    crop.save_with_format(captures_dir().join(&item.file), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    history::make_image_thumb(&crop, &item.thumb_path())?;
    history::add(item.clone())?;
    Ok(item)
}
