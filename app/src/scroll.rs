//! Scrolling capture session: grab the region over and over while it scrolls (by the user, or
//! by myCapture on Windows) and stitch the frames into one tall image.
use crate::capture::Rect;
use crate::stitch::{Step, Stitcher};
use image::RgbaImage;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Whether myCapture can scroll the page itself on this platform.
pub const AUTO_SUPPORTED: bool = cfg!(windows);

#[derive(Clone, Serialize)]
pub struct Progress {
    pub height: u32,
    /// "ok" | "lost" (scrolled too far at once) | "end" (auto-scroll hit the bottom) | "full" (size limit)
    pub state: &'static str,
}

pub struct Session {
    stop: Arc<AtomicBool>,
    auto: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<RgbaImage, String>>>,
}

impl Session {
    /// `crop` is in image pixels of monitor `screen_index`; `center` is the region centre on the desktop.
    pub fn start(app: AppHandle, screen_index: usize, crop: Rect, center: (i32, i32)) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let auto = Arc::new(AtomicBool::new(false));
        let (s, a) = (stop.clone(), auto.clone());
        let thread = std::thread::spawn(move || {
            run(|p| { let _ = app.emit("scroll-progress", p); }, screen_index, crop, center, &s, &a)
        });
        Self { stop, auto, thread: Some(thread) }
    }

    pub fn set_auto(&self, on: bool) {
        self.auto.store(on && AUTO_SUPPORTED, Ordering::Relaxed);
    }

    /// Stop grabbing and return the stitched image.
    pub fn finish(mut self) -> Result<RgbaImage, String> {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().ok_or_else(|| tr!("Nothing was captured."))?.join().map_err(|_| tr!("Nothing was captured."))?
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn grab(screen_index: usize, crop: Rect) -> Result<RgbaImage, String> {
    let mon = xcap::Monitor::all().map_err(|e| e.to_string())?.into_iter().nth(screen_index).ok_or_else(|| tr!("Could not capture the screen."))?;
    let img = mon.capture_image().map_err(|e| e.to_string())?;
    let x = (crop.x.max(0) as u32).min(img.width().saturating_sub(1));
    let y = (crop.y.max(0) as u32).min(img.height().saturating_sub(1));
    let w = (crop.w as u32).min(img.width() - x);
    let h = (crop.h as u32).min(img.height() - y);
    Ok(image::imageops::crop_imm(&img, x, y, w, h).to_image())
}

fn run(report: impl Fn(Progress), screen_index: usize, crop: Rect, center: (i32, i32), stop: &AtomicBool, auto: &AtomicBool) -> Result<RgbaImage, String> {
    let mut st = Stitcher::new(grab(screen_index, crop)?);
    let emit = |height, state| report(Progress { height, state });
    emit(st.height(), "ok");
    let mut idle = 0;
    while !stop.load(Ordering::Relaxed) {
        let auto_on = auto.load(Ordering::Relaxed);
        if auto_on {
            wheel(center, crop.h);
            // Let smooth scrolling settle before grabbing.
            std::thread::sleep(Duration::from_millis(260));
        } else {
            std::thread::sleep(Duration::from_millis(90));
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let Ok(frame) = grab(screen_index, crop) else { continue };
        let state = match st.push(frame) {
            Step::Grew(_) => {
                idle = 0;
                "ok"
            }
            Step::Same => {
                idle += 1;
                "ok"
            }
            Step::Lost => "lost",
        };
        if st.full() {
            emit(st.height(), "full");
            break;
        }
        if auto_on && idle >= 4 {
            // Several wheel turns and nothing new: that's the bottom of the page.
            auto.store(false, Ordering::Relaxed);
            emit(st.height(), "end");
            idle = 0;
            continue;
        }
        emit(st.height(), state);
    }
    Ok(st.finish())
}

/// Scroll down over the region (the cursor is moved there if it's elsewhere): one wheel notch,
/// or half of one for short regions so consecutive frames still overlap.
#[cfg(windows)]
fn wheel(center: (i32, i32), region_h: i32) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_WHEEL, MOUSEINPUT};
    use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos};
    unsafe {
        let mut p = POINT::default();
        if GetCursorPos(&mut p).is_err() || (p.x - center.0).abs() > 40 || (p.y - center.1).abs() > 40 {
            let _ = SetCursorPos(center.0, center.1);
        }
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: (if region_h < 400 { -60i32 } else { -120 }) as u32, dwFlags: MOUSEEVENTF_WHEEL, time: 0, dwExtraInfo: 0 } },
        };
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(not(windows))]
fn wheel(_: (i32, i32), _: i32) {}

