mod capture;
mod clip;
mod history;
mod record;
mod settings;
mod upload;

use capture::Rect;
use serde::Serialize;
use settings::{captures_dir, Settings};
use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_opener::OpenerExt;

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub pending: Mutex<Option<capture::Pending>>,
    pub rec: Mutex<Option<record::Recording>>,
    /// Whether the sun's menu is fanned out.
    pub sun: Mutex<bool>,
    /// Cursor and window position when a sun drag started.
    pub sun_drag: Mutex<Option<(PhysicalPosition<f64>, PhysicalPosition<i32>)>>,
}

fn settings_of(app: &AppHandle) -> Settings {
    app.state::<AppState>().settings.lock().unwrap().clone()
}

fn show_sun(app: &AppHandle) {
    if settings_of(app).show_sun {
        if let Some(w) = app.get_webview_window("sun") {
            let _ = w.show();
        }
    }
}

/// Show an error in a small styled popup window (no native alert boxes).
fn report_error(app: &AppHandle, msg: &str) {
    let enc: String = msg
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    close_window(app, "notice");
    let _ = WebviewWindowBuilder::new(app, "notice", WebviewUrl::App(format!("notice.html?msg={enc}").into()))
        .title("myCapture")
        .inner_size(460.0, 230.0)
        .resizable(false)
        .always_on_top(true)
        .center()
        .build();
}

fn notify_history(app: &AppHandle) {
    let _ = app.emit("history-changed", ());
}

// ---------- windows ----------

fn open_page(app: &AppHandle, label: &str, url: &str, title: &str, w: f64, h: f64) -> Result<WebviewWindow, String> {
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(win);
    }
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(w, h)
        .min_inner_size(520.0, 400.0)
        .center()
        .build()
        .map_err(|e| e.to_string())
}

fn open_editor_window(app: &AppHandle, id: &str) -> Result<(), String> {
    let label = format!("ed_{}", id.replace(|c: char| !c.is_ascii_alphanumeric(), "_"));
    open_page(app, &label, &format!("editor.html?id={id}"), "myCapture — Preview", 1100.0, 760.0).map(|_| ())
}

fn open_camera(app: &AppHandle, at: Option<Rect>) -> Result<(), String> {
    let s = settings_of(app);
    let size = s.camera_size.clamp(120, 480) as f64;
    let win = match app.get_webview_window("camera") {
        Some(w) => w,
        None => WebviewWindowBuilder::new(app, "camera", WebviewUrl::App("camera.html".into()))
            .title("myCapture camera")
            .inner_size(size, size)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .build()
            .map_err(|e| e.to_string())?,
    };
    let sc = win.scale_factor().unwrap_or(1.0);
    let px = (size * sc) as i32;
    let margin = (16.0 * sc) as i32;
    let (x, y) = match at {
        Some(r) => (r.x + r.w - px - margin, r.y + r.h - px - margin),
        None => {
            let m = app.primary_monitor().ok().flatten().ok_or("No monitor")?;
            (m.position().x + m.size().width as i32 - px - margin * 2, m.position().y + m.size().height as i32 - px - margin * 5)
        }
    };
    let _ = win.set_position(PhysicalPosition::new(x, y));
    let _ = win.show();
    Ok(())
}

fn close_window(app: &AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.destroy();
    }
}

// ---------- capture commands ----------

fn spawn_capture(app: &AppHandle) {
    // While recording, the capture hotkey / tray click finishes the recording instead.
    if app.state::<AppState>().rec.lock().unwrap().is_some() {
        let _ = app.emit_to("recbar", "rec-hotkey", ());
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = capture::begin(&app) {
            show_sun(&app);
            report_error(&app, &e);
        }
    });
}

#[tauri::command]
fn begin_capture(app: AppHandle) {
    spawn_capture(&app);
}

#[tauri::command]
fn overlay_info(app: AppHandle) -> Result<capture::OverlayInfo, String> {
    capture::info(&app)
}

#[tauri::command]
fn cancel_capture(app: AppHandle) {
    app.state::<AppState>().pending.lock().unwrap().take();
    capture::close_overlay(&app);
    show_sun(&app);
}

#[tauri::command(async)]
async fn finish_image(app: AppHandle, rect: Rect) -> Result<(), String> {
    let item = capture::finish_image(&app, rect)?;
    show_sun(&app);
    notify_history(&app);
    let s = settings_of(&app);
    match s.auto_copy.as_str() {
        "image" => {
            let _ = clip::image(&item.path());
        }
        "link" => {
            let app2 = app.clone();
            let id = item.id.clone();
            tauri::async_runtime::spawn(async move {
                match share_item(&app2, &id).await {
                    Ok(url) => {
                        let _ = app2.emit("shared", (id, url));
                    }
                    Err(e) => report_error(&app2, &e),
                }
            });
        }
        _ => {}
    }
    open_editor_window(&app, &item.id)
}

// ---------- recording ----------

#[tauri::command(async)]
fn start_video(app: AppHandle, rect: Rect) -> Result<(), String> {
    let state = app.state::<AppState>();
    let p = state.pending.lock().unwrap().take().ok_or("No capture in progress")?;
    capture::close_overlay(&app);
    let (iw, ih) = (p.image.width() as i32, p.image.height() as i32);
    let mut r = rect;
    r.x = r.x.clamp(0, iw - 2);
    r.y = r.y.clamp(0, ih - 2);
    r.w = (r.w.min(iw - r.x) & !1).max(2);
    r.h = (r.h.min(ih - r.y) & !1).max(2);
    let grab = Rect {
        x: (p.mon_x as f64 + r.x as f64 / p.ratio).round() as i32,
        y: (p.mon_y as f64 + r.y as f64 / p.ratio).round() as i32,
        w: ((r.w as f64 / p.ratio) as i32) & !1,
        h: ((r.h as f64 / p.ratio) as i32) & !1,
    };
    let screen_rect = p.to_physical(r);
    let mon = p.to_physical(Rect { x: 0, y: 0, w: iw, h: ih });

    // Border just outside the region (excluded from capture where the OS allows it).
    let b = 4;
    let frame = WebviewWindowBuilder::new(&app, "frame", WebviewUrl::App("frame.html".into()))
        .title("myCapture region")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = frame.set_position(PhysicalPosition::new(screen_rect.x - b, screen_rect.y - b));
    let _ = frame.set_size(PhysicalSize::new((screen_rect.w + 2 * b) as u32, (screen_rect.h + 2 * b) as u32));
    let _ = frame.set_ignore_cursor_events(true);
    let _ = frame.set_content_protected(true);
    let _ = frame.show();

    // Control bar below the region, else above it, else inside the bottom edge.
    let bar = WebviewWindowBuilder::new(&app, "recbar", WebviewUrl::App("recbar.html".into()))
        .title("myCapture recording")
        .inner_size(400.0, 56.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
    let sc = bar.scale_factor().unwrap_or(1.0);
    let (bw, bh) = ((400.0 * sc) as i32, (56.0 * sc) as i32);
    let gap = (10.0 * sc) as i32;
    let bx = (screen_rect.x + screen_rect.w / 2 - bw / 2).clamp(mon.x, mon.x + mon.w - bw);
    let by = if screen_rect.y + screen_rect.h + gap + bh <= mon.y + mon.h {
        screen_rect.y + screen_rect.h + gap
    } else if screen_rect.y - gap - bh >= mon.y {
        screen_rect.y - gap - bh
    } else {
        screen_rect.y + screen_rect.h - bh - gap
    };
    let _ = bar.set_position(PhysicalPosition::new(bx, by));
    let _ = bar.set_content_protected(true);
    let _ = bar.show();

    let s = settings_of(&app);
    let mut opened_camera = false;
    if s.camera_on_record && app.get_webview_window("camera").is_none() {
        open_camera(&app, Some(screen_rect))?;
        opened_camera = true;
    }
    *state.rec.lock().unwrap() = Some(record::Recording {
        id: history::new_id(),
        grab,
        crop: r,
        screen_index: p.screen_index,
        screen_rect,
        segments: vec![],
        child: None,
        seg_started: None,
        elapsed: Default::default(),
        opened_camera,
    });
    Ok(())
}

#[derive(Serialize)]
struct RecStatus {
    active: bool,
    paused: bool,
    elapsed: f64,
    countdown: u32,
    mic: bool,
    camera: bool,
}

#[tauri::command]
fn record_status(app: AppHandle) -> RecStatus {
    let s = settings_of(&app);
    let state = app.state::<AppState>();
    let rec = state.rec.lock().unwrap();
    RecStatus {
        active: rec.is_some(),
        paused: rec.as_ref().map(|r| r.child.is_none()).unwrap_or(false),
        elapsed: rec.as_ref().map(|r| r.total_elapsed().as_secs_f64()).unwrap_or(0.0),
        countdown: s.countdown.min(10),
        mic: s.record_mic,
        camera: app.get_webview_window("camera").is_some(),
    }
}

fn abort_recording(app: &AppHandle) {
    if let Some(r) = app.state::<AppState>().rec.lock().unwrap().take() {
        if r.opened_camera {
            close_window(app, "camera");
        }
        record::discard(r);
    }
    close_window(app, "frame");
    close_window(app, "recbar");
    show_sun(app);
}

/// Start (or resume) capturing frames.
#[tauri::command(async)]
fn record_resume(app: AppHandle) -> Result<(), String> {
    let s = settings_of(&app);
    let state = app.state::<AppState>();
    let mut guard = state.rec.lock().unwrap();
    let r = guard.as_mut().ok_or("Not recording")?;
    if r.child.is_some() {
        return Ok(());
    }
    let res = record::start_segment(&s, r);
    if let Err(e) = &res {
        if r.segments.is_empty() {
            drop(guard);
            abort_recording(&app);
            report_error(&app, &format!("Recording could not start.

{e}"));
        }
    }
    res
}

#[tauri::command(async)]
fn record_pause(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut guard = state.rec.lock().unwrap();
    let r = guard.as_mut().ok_or("Not recording")?;
    record::stop_segment(r);
    Ok(())
}

#[tauri::command(async)]
fn record_stop(app: AppHandle) -> Result<(), String> {
    let r = app.state::<AppState>().rec.lock().unwrap().take().ok_or("Not recording")?;
    close_window(&app, "frame");
    close_window(&app, "recbar");
    if r.opened_camera {
        close_window(&app, "camera");
    }
    let res = record::finalize(&settings_of(&app), r);
    show_sun(&app);
    let item = res?;
    notify_history(&app);
    open_editor_window(&app, &item.id)
}

#[tauri::command(async)]
fn record_cancel(app: AppHandle) {
    abort_recording(&app);
}

// ---------- camera ----------

#[tauri::command]
fn toggle_camera(app: AppHandle) -> Result<bool, String> {
    if app.get_webview_window("camera").is_some() {
        close_window(&app, "camera");
        return Ok(false);
    }
    let at = app.state::<AppState>().rec.lock().unwrap().as_ref().map(|r| r.screen_rect);
    open_camera(&app, at)?;
    Ok(true)
}

#[tauri::command]
fn camera_set_size(app: AppHandle, size: u32, shape: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut s = state.settings.lock().unwrap();
    s.camera_size = size.clamp(120, 480);
    s.camera_shape = shape;
    s.save()?;
    if let Some(w) = app.get_webview_window("camera") {
        // Keep the bottom-right corner anchored.
        let sc = w.scale_factor().unwrap_or(1.0);
        if let (Ok(pos), Ok(old)) = (w.outer_position(), w.outer_size()) {
            let new = (s.camera_size as f64 * sc) as i32;
            let _ = w.set_size(LogicalSize::new(s.camera_size as f64, s.camera_size as f64));
            let _ = w.set_position(PhysicalPosition::new(pos.x + old.width as i32 - new, pos.y + old.height as i32 - new));
        }
    }
    Ok(())
}

// ---------- sun ----------
// The sun docks to a screen edge with only its inner half showing, like the classic Jing.

const SUN: f64 = 72.0;
const SUN_PEEK: f64 = 40.0;
const SUN_EXP_LONG: f64 = 300.0;
const SUN_EXP_SHORT: f64 = 170.0;

/// Size and position the sun window for its edge, collapsed or expanded.
fn layout_sun(app: &AppHandle, expanded: bool) -> String {
    let Some(w) = app.get_webview_window("sun") else { return "top".into() };
    let s = settings_of(app);
    let mon = s
        .sun_x
        .zip(s.sun_y)
        .and_then(|(x, y)| app.monitor_from_point(x as f64, y as f64).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(mon) = mon else { return s.sun_edge };
    let area = mon.work_area();
    let (ax0, ay0, aw, ah) = (area.position.x, area.position.y, area.size.width as i32, area.size.height as i32);
    let sc = mon.scale_factor();
    let edge = match s.sun_edge.as_str() {
        "bottom" | "left" | "right" => s.sun_edge.clone(),
        _ => "top".to_string(),
    };
    let horizontal = edge == "top" || edge == "bottom";
    let (long, short) = if expanded { (SUN_EXP_LONG, SUN_EXP_SHORT) } else { (SUN, SUN_PEEK) };
    let (lw, lh) = if horizontal { (long, short) } else { (short, long) };
    let (wpx, hpx) = ((lw * sc) as i32, (lh * sc) as i32);
    // Keep the sun far enough from corners that the expanded fan always fits.
    let margin = (SUN_EXP_LONG / 2.0 * sc) as i32;
    let along = |v: Option<i32>, lo: i32, len: i32| v.unwrap_or(lo + len / 2).clamp(lo + margin, (lo + len - margin).max(lo + margin));
    let (x, y) = match edge.as_str() {
        "top" => (along(s.sun_x, ax0, aw) - wpx / 2, ay0),
        "bottom" => (along(s.sun_x, ax0, aw) - wpx / 2, ay0 + ah - hpx),
        "left" => (ax0, along(s.sun_y, ay0, ah) - hpx / 2),
        _ => (ax0 + aw - wpx, along(s.sun_y, ay0, ah) - hpx / 2),
    };
    let _ = w.set_size(LogicalSize::new(lw, lh));
    let _ = w.set_position(PhysicalPosition::new(x, y));
    edge
}

#[tauri::command]
fn sun_expand(app: AppHandle) -> String {
    *app.state::<AppState>().sun.lock().unwrap() = true;
    layout_sun(&app, true)
}

#[tauri::command]
fn sun_collapse(app: AppHandle) -> String {
    *app.state::<AppState>().sun.lock().unwrap() = false;
    layout_sun(&app, false)
}

#[tauri::command]
fn sun_drag_begin(app: AppHandle) {
    let Some(w) = app.get_webview_window("sun") else { return };
    if let (Ok(c), Ok(p)) = (app.cursor_position(), w.outer_position()) {
        *app.state::<AppState>().sun_drag.lock().unwrap() = Some((c, p));
    }
}

#[tauri::command]
fn sun_drag_move(app: AppHandle) {
    let Some((c0, p0)) = *app.state::<AppState>().sun_drag.lock().unwrap() else { return };
    let (Some(w), Ok(c)) = (app.get_webview_window("sun"), app.cursor_position()) else { return };
    let _ = w.set_position(PhysicalPosition::new(p0.x + (c.x - c0.x) as i32, p0.y + (c.y - c0.y) as i32));
}

/// After the user drags the sun, dock it to the nearest edge of the monitor it was dropped on.
#[tauri::command]
fn sun_snap(app: AppHandle) -> String {
    let Some(w) = app.get_webview_window("sun") else { return "top".into() };
    *app.state::<AppState>().sun.lock().unwrap() = false;
    app.state::<AppState>().sun_drag.lock().unwrap().take();
    let (Ok(p), Ok(sz)) = (w.outer_position(), w.outer_size()) else { return "top".into() };
    // The sun's centre sits on the window edge facing the screen edge, expanded or not.
    let (ww, wh) = (sz.width as i32, sz.height as i32);
    let (cx, cy) = match settings_of(&app).sun_edge.as_str() {
        "bottom" => (p.x + ww / 2, p.y + wh),
        "left" => (p.x, p.y + wh / 2),
        "right" => (p.x + ww, p.y + wh / 2),
        _ => (p.x + ww / 2, p.y),
    };
    let mon = app
        .monitor_from_point(cx as f64, cy as f64)
        .ok()
        .flatten()
        .or_else(|| w.current_monitor().ok().flatten());
    if let Some(mon) = mon {
        let a = mon.work_area();
        let (x0, y0, x1, y1) = (a.position.x, a.position.y, a.position.x + a.size.width as i32, a.position.y + a.size.height as i32);
        let (cx, cy) = (cx.clamp(x0, x1 - 1), cy.clamp(y0, y1 - 1));
        let edge = [("top", cy - y0), ("bottom", y1 - cy), ("left", cx - x0), ("right", x1 - cx)]
            .into_iter()
            .min_by_key(|(_, d)| *d)
            .map(|(e, _)| e)
            .unwrap_or("top");
        let state = app.state::<AppState>();
        let mut s = state.settings.lock().unwrap();
        s.sun_edge = edge.into();
        s.sun_x = Some(cx);
        s.sun_y = Some(cy);
        let _ = s.save();
    }
    layout_sun(&app, false)
}

// ---------- history & sharing ----------

#[tauri::command]
fn history_list() -> Vec<history::Item> {
    history::list()
}

#[tauri::command]
fn history_get(id: String) -> Result<history::Item, String> {
    history::get(&id)
}

#[tauri::command]
fn history_delete(app: AppHandle, id: String) -> Result<(), String> {
    history::remove(&id)?;
    notify_history(&app);
    Ok(())
}

#[tauri::command(async)]
fn copy_image(id: String) -> Result<(), String> {
    let it = history::get(&id)?;
    if it.kind == "image" {
        clip::image(&it.path())
    } else {
        clip::file(&it.path())
    }
}

#[tauri::command]
fn copy_file(id: String) -> Result<(), String> {
    clip::file(&history::get(&id)?.path())
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    clip::text(&text)
}

async fn share_item(app: &AppHandle, id: &str) -> Result<String, String> {
    let mut it = history::get(id)?;
    if let Some(url) = it.url.clone() {
        clip::text(&url)?;
        return Ok(url);
    }
    let cfg = settings_of(app).upload;
    let thumb = it.thumb_path();
    let url = upload::upload(&cfg, &it.path(), Some(&thumb)).await?;
    it.url = Some(url.clone());
    history::update(&it)?;
    clip::text(&url)?;
    notify_history(app);
    Ok(url)
}

/// Upload (once) and put the link on the clipboard.
#[tauri::command]
async fn share(app: AppHandle, id: String) -> Result<String, String> {
    share_item(&app, &id).await
}

#[tauri::command(async)]
fn save_as(app: AppHandle, id: String) -> Result<bool, String> {
    let it = history::get(&id)?;
    let ext = if it.kind == "image" { "png" } else { "mp4" };
    let picked = app
        .dialog()
        .file()
        .set_file_name(&it.file)
        .add_filter(if ext == "png" { "PNG image" } else { "MP4 video" }, &[ext])
        .blocking_save_file();
    let Some(dest) = picked else { return Ok(false) };
    let dest = dest.into_path().map_err(|e| e.to_string())?;
    std::fs::copy(it.path(), dest).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Replace an image capture with its annotated version (raw PNG body, id in `x-id` header).
#[tauri::command(async)]
fn save_edited(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let id = request.headers().get("x-id").and_then(|v| v.to_str().ok()).ok_or("Missing id")?.to_string();
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err("Expected PNG bytes".into()) };
    let mut it = history::get(&id)?;
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?.to_rgba8();
    std::fs::write(it.path(), bytes).map_err(|e| e.to_string())?;
    history::make_image_thumb(&img, &it.thumb_path())?;
    it.width = img.width();
    it.height = img.height();
    it.url = None;
    history::update(&it)?;
    notify_history(&app);
    Ok(())
}

#[tauri::command]
fn open_editor(app: AppHandle, id: String) -> Result<(), String> {
    open_editor_window(&app, &id)
}

#[tauri::command]
fn open_history(app: AppHandle) -> Result<(), String> {
    open_page(&app, "history", "history.html", "myCapture — History", 1040.0, 720.0).map(|_| ())
}

#[tauri::command]
fn open_settings(app: AppHandle) -> Result<(), String> {
    open_page(&app, "settings", "settings.html", "myCapture — Settings", 780.0, 720.0).map(|_| ())
}

#[tauri::command]
fn open_captures_folder(app: AppHandle) -> Result<(), String> {
    app.opener().open_path(captures_dir().to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

// ---------- settings ----------

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    settings_of(&app)
}

#[tauri::command]
fn set_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let old = settings_of(&app);
    let mut new = settings;
    new.sun_x = old.sun_x;
    new.sun_y = old.sun_y;
    new.sun_edge = old.sun_edge;
    new.save()?;
    *app.state::<AppState>().settings.lock().unwrap() = new.clone();
    if new.hotkey != old.hotkey {
        register_hotkey(&app)?;
    }
    if let Some(sun) = app.get_webview_window("sun") {
        let _ = if new.show_sun { sun.show() } else { sun.hide() };
    }
    Ok(())
}

#[tauri::command(async)]
fn check_ffmpeg(app: AppHandle) -> Result<String, String> {
    record::ffmpeg_version(&settings_of(&app))
}

#[tauri::command(async)]
fn list_mics(app: AppHandle) -> Vec<String> {
    record::list_mics(&settings_of(&app))
}

#[tauri::command]
async fn test_upload(app: AppHandle) -> Result<String, String> {
    let path = std::env::temp_dir().join("mycapture-test.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([245, 158, 11, 255]))
        .save_with_format(&path, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    upload::upload(&settings_of(&app).upload, &path, None).await
}

#[tauri::command]
fn quit(app: AppHandle) {
    abort_recording(&app);
    app.exit(0);
}

fn register_hotkey(app: &AppHandle) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let key = settings_of(app).hotkey;
    if key.trim().is_empty() {
        return Ok(());
    }
    gs.register(key.as_str()).map_err(|e| format!("Could not register hotkey {key}: {e}"))
}

// ---------- custom protocol: mycap://localhost/frame and /file/<name> ----------

fn serve(app: &AppHandle, req: &tauri::http::Request<Vec<u8>>) -> tauri::http::Response<Vec<u8>> {
    let mut res = serve_inner(app, req);
    // The editor draws these into a canvas it later exports, so allow CORS.
    res.headers_mut().insert(tauri::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, tauri::http::HeaderValue::from_static("*"));
    res
}

fn serve_inner(app: &AppHandle, req: &tauri::http::Request<Vec<u8>>) -> tauri::http::Response<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    use tauri::http::{header, Response, StatusCode};
    let not_found = || Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()).unwrap();
    let path = req.uri().path();
    if path == "/frame" {
        return match capture::frame_png(app) {
            Some(png) => Response::builder().header(header::CONTENT_TYPE, "image/png").body(png).unwrap(),
            None => not_found(),
        };
    }
    let Some(name) = path.strip_prefix("/file/") else { return not_found() };
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return not_found();
    }
    let file_path = captures_dir().join(name);
    let Ok(mut f) = std::fs::File::open(&file_path) else { return not_found() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mime = match file_path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "png" => "image/png",
        "jpg" => "image/jpeg",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    };
    // Range requests so <video> can seek.
    let range = req
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .and_then(|v| v.split_once('-'))
        .and_then(|(a, b)| {
            let start: u64 = a.parse().ok()?;
            let end: u64 = if b.is_empty() { (start + 4 * 1024 * 1024).min(len) - 1 } else { b.parse().ok()? };
            (start <= end && end < len).then_some((start, end))
        });
    match range {
        Some((start, end)) => {
            let mut buf = vec![0u8; (end - start + 1) as usize];
            if f.seek(SeekFrom::Start(start)).is_err() || f.read_exact(&mut buf).is_err() {
                return not_found();
            }
            Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
                .body(buf)
                .unwrap()
        }
        None => {
            let mut buf = Vec::with_capacity(len as usize);
            let _ = f.read_to_end(&mut buf);
            Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CACHE_CONTROL, "no-cache")
                .body(buf)
                .unwrap()
        }
    }
}

// ---------- tray ----------

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let capture = MenuItem::with_id(app, "capture", "Capture…", true, None::<&str>)?;
    let camera = MenuItem::with_id(app, "camera", "Toggle camera", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "History", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let sun = MenuItem::with_id(app, "sun", "Show / hide the sun", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit myCapture", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&capture, &camera, &sep, &history, &settings, &sun, &sep2, &quit_item])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("icon"))
        .tooltip("myCapture — click to capture")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "capture" => spawn_capture(app),
            "camera" => {
                let _ = toggle_camera(app.clone());
            }
            "history" => {
                let _ = open_history(app.clone());
            }
            "settings" => {
                let _ = open_settings(app.clone());
            }
            "sun" => {
                let state = app.state::<AppState>();
                let mut s = state.settings.lock().unwrap();
                s.show_sun = !s.show_sun;
                let _ = s.save();
                if let Some(w) = app.get_webview_window("sun") {
                    let _ = if s.show_sun { w.show() } else { w.hide() };
                }
            }
            "quit" => quit(app.clone()),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                spawn_capture(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, ev| {
                    if ev.state == ShortcutState::Pressed {
                        spawn_capture(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            settings: Mutex::new(Settings::load()),
            pending: Mutex::new(None),
            rec: Mutex::new(None),
            sun: Mutex::new(false),
            sun_drag: Mutex::new(None),
        })
        .register_asynchronous_uri_scheme_protocol("mycap", |ctx, req, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || responder.respond(serve(&app, &req)));
        })
        .invoke_handler(tauri::generate_handler![
            begin_capture,
            overlay_info,
            cancel_capture,
            finish_image,
            start_video,
            record_status,
            record_resume,
            record_pause,
            record_stop,
            record_cancel,
            toggle_camera,
            camera_set_size,
            sun_expand,
            sun_collapse,
            sun_snap,
            sun_drag_begin,
            sun_drag_move,
            history_list,
            history_get,
            history_delete,
            copy_image,
            copy_file,
            copy_text,
            share,
            save_as,
            save_edited,
            open_editor,
            open_history,
            open_settings,
            open_captures_folder,
            open_url,
            get_settings,
            set_settings,
            check_ffmpeg,
            list_mics,
            test_upload,
            quit
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            build_tray(&handle)?;
            if let Err(e) = register_hotkey(&handle) {
                eprintln!("{e}");
            }
            layout_sun(&handle, false);
            show_sun(&handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building myCapture")
        .run(|_app, ev| {
            if let tauri::RunEvent::ExitRequested { api, code: None, .. } = ev {
                api.prevent_exit();
            }
        });
}
