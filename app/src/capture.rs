//! Screenshot capture: freeze every monitor, let the user pick a region on any of them.
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

/// One xcap window rect in xcap units, before it is mapped onto a monitor.
struct WinRect {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

/// Freeze every monitor. Each one gets its own overlay so selections work on any screen.
pub fn grab_all(app: &AppHandle) -> Result<Vec<Pending>, String> {
    let tmons = app.available_monitors().map_err(|e| e.to_string())?;
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let me = std::process::id();
    let wins: Vec<WinRect> = xcap::Window::all()
        .unwrap_or_default()
        .into_iter()
        .filter(|w| w.pid().ok() != Some(me) && !w.is_minimized().unwrap_or(true))
        .filter_map(|w| Some(WinRect { x: w.x().ok()?, y: w.y().ok()?, w: w.width().ok()?, h: w.height().ok()? }))
        .collect();

    let mut shots = Vec::new();
    for tmon in tmons {
        let tpos = *tmon.position();
        let tsize = *tmon.size();
        let tscale = tmon.scale_factor();
        // Match xcap's monitor to Tauri's (xcap may report points on macOS, pixels elsewhere).
        let Some((screen_index, mon)) = monitors.iter().enumerate().min_by_key(|(_, m)| {
            let (mx, my) = (m.x().unwrap_or(0) as f64, m.y().unwrap_or(0) as f64);
            let d1 = (mx - tpos.x as f64).abs() + (my - tpos.y as f64).abs();
            let d2 = (mx - tpos.x as f64 / tscale).abs() + (my - tpos.y as f64 / tscale).abs();
            d1.min(d2) as i64
        }) else {
            continue;
        };
        let Ok(image) = mon.capture_image() else { continue };
        let mon_x = mon.x().unwrap_or(0);
        let mon_y = mon.y().unwrap_or(0);
        let ratio = image.width() as f64 / mon.width().unwrap_or(image.width()).max(1) as f64;
        let windows = wins
            .iter()
            .filter_map(|w| {
                let r = Rect {
                    x: ((w.x - mon_x) as f64 * ratio) as i32,
                    y: ((w.y - mon_y) as f64 * ratio) as i32,
                    w: (w.w as f64 * ratio) as i32,
                    h: (w.h as f64 * ratio) as i32,
                };
                clip(r, image.width(), image.height())
            })
            .collect();
        shots.push(Pending {
            win_origin: (tpos.x, tpos.y),
            win_k: tsize.width as f64 / image.width().max(1) as f64,
            image,
            frame_png: Mutex::new(None),
            mon_x,
            mon_y,
            ratio,
            screen_index,
            windows,
        });
    }
    if shots.is_empty() {
        return Err("Could not capture the screen.".into());
    }
    Ok(shots)
}

/// Overlay windows are labelled `overlay-<n>`, one per frozen monitor.
pub fn overlay_index(label: &str) -> Option<usize> {
    label.strip_prefix("overlay-")?.parse().ok()
}

pub fn encode_fast_png(img: &image::RgbaImage) -> Vec<u8> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;
    let mut out = Vec::new();
    let enc = PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::NoFilter);
    let _ = enc.write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8);
    out
}

/// Hide the moon, freeze every screen and open a selection overlay on each.
pub fn begin(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.rec.lock().unwrap().is_some() {
        return Err("A recording is in progress.".into());
    }
    if !state.pending.lock().unwrap().is_empty() {
        if let Some(w) = app.webview_windows().into_values().find(|w| overlay_index(w.label()).is_some()) {
            let _ = w.set_focus();
        }
        return Ok(());
    }
    if let Some(moon) = app.get_webview_window("moon") {
        let _ = moon.hide();
    }
    std::thread::sleep(std::time::Duration::from_millis(180));
    let shots = grab_all(app)?;
    let places: Vec<(i32, i32, PhysicalSize<u32>)> = shots
        .iter()
        .map(|p| {
            let size = PhysicalSize::new((p.image.width() as f64 * p.win_k) as u32, (p.image.height() as f64 * p.win_k) as u32);
            (p.win_origin.0, p.win_origin.1, size)
        })
        .collect();
    *state.pending.lock().unwrap() = shots;

    for (i, (ox, oy, size)) in places.into_iter().enumerate() {
        let win = WebviewWindowBuilder::new(app, format!("overlay-{i}"), WebviewUrl::App("overlay.html".into()))
            .title("myCapture capture")
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .visible(false)
            .build()
            .map_err(|e| e.to_string())?;
        // Position first so the window adopts that monitor's DPI, then size it.
        let _ = win.set_position(PhysicalPosition::new(ox, oy));
        let _ = win.set_size(size);
        let _ = win.set_position(PhysicalPosition::new(ox, oy));
    }
    // overlay.html shows itself once its frozen frame has been painted.
    Ok(())
}

/// Whether this overlay is on the monitor under the mouse (that one takes keyboard focus).
pub fn under_cursor(app: &AppHandle, idx: usize) -> bool {
    let Ok(c) = app.cursor_position() else { return idx == 0 };
    let state = app.state::<AppState>();
    let shots = state.pending.lock().unwrap();
    shots.get(idx).is_some_and(|p| {
        let (w, h) = (p.image.width() as f64 * p.win_k, p.image.height() as f64 * p.win_k);
        let (x, y) = (c.x - p.win_origin.0 as f64, c.y - p.win_origin.1 as f64);
        x >= 0.0 && y >= 0.0 && x < w && y < h
    })
}

pub fn info(app: &AppHandle, idx: usize) -> Result<OverlayInfo, String> {
    let state = app.state::<AppState>();
    let shots = state.pending.lock().unwrap();
    let p = shots.get(idx).ok_or("No capture in progress")?;
    Ok(OverlayInfo { width: p.image.width(), height: p.image.height(), windows: p.windows.clone() })
}

pub fn frame_png(app: &AppHandle, idx: usize) -> Option<Vec<u8>> {
    let state = app.state::<AppState>();
    let shots = state.pending.lock().unwrap();
    let p = shots.get(idx)?;
    let mut cache = p.frame_png.lock().unwrap();
    if cache.is_none() {
        *cache = Some(encode_fast_png(&p.image));
    }
    cache.clone()
}

pub fn close_overlay(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        if overlay_index(&label).is_some() {
            let _ = w.destroy();
        }
    }
}

/// Take the frozen shot for overlay `idx`, ending the capture on every monitor.
pub fn take(app: &AppHandle, idx: usize) -> Result<Pending, String> {
    let state = app.state::<AppState>();
    let mut shots = std::mem::take(&mut *state.pending.lock().unwrap());
    close_overlay(app);
    if idx < shots.len() {
        Ok(shots.swap_remove(idx))
    } else {
        Err("No capture in progress".into())
    }
}

/// Crop the frozen frame, store it in history and return the new item.
pub fn finish_image(app: &AppHandle, idx: usize, rect: Rect) -> Result<history::Item, String> {
    let pending = take(app, idx)?;
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
        links: Default::default(),
    };
    crop.save_with_format(captures_dir().join(&item.file), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    history::make_image_thumb(&crop, &item.thumb_path())?;
    history::add(item.clone())?;
    Ok(item)
}
