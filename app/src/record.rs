//! Screen video recording through ffmpeg, with pause/resume implemented as segments.
use crate::{capture::Rect, history, settings::captures_dir, settings::Settings};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct Recording {
    pub id: String,
    /// Region in the coordinates ffmpeg's grabber expects.
    pub grab: Rect,
    /// Region in image pixels relative to the monitor (used for macOS crop).
    pub crop: Rect,
    pub screen_index: usize,
    /// Physical window rect of the region, for placing the border / camera / bar.
    pub screen_rect: Rect,
    pub segments: Vec<PathBuf>,
    pub child: Option<(Child, Arc<Mutex<String>>)>,
    pub seg_started: Option<Instant>,
    pub elapsed: Duration,
    pub opened_camera: bool,
}

impl Recording {
    pub fn total_elapsed(&self) -> Duration {
        self.elapsed + self.seg_started.map(|s| s.elapsed()).unwrap_or_default()
    }
}

pub fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

fn work_dir(id: &str) -> PathBuf {
    let d = std::env::temp_dir().join("mycapture-rec").join(id);
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Probe ffmpeg and return its version line.
pub fn ffmpeg_version(s: &Settings) -> Result<String, String> {
    let out = command(&s.ffmpeg())
        .arg("-version")
        .output()
        .map_err(|_| format!("ffmpeg was not found ({}). Install it or set its path in Settings.", s.ffmpeg()))?;
    Ok(String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("ffmpeg").to_string())
}

/// List microphones ffmpeg can open on this platform.
pub fn list_mics(s: &Settings) -> Vec<String> {
    let args: Vec<&str> = if cfg!(windows) {
        vec!["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"]
    } else if cfg!(target_os = "macos") {
        vec!["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""]
    } else {
        return vec!["default".into()];
    };
    let Ok(out) = command(&s.ffmpeg()).args(args).output() else { return vec![] };
    let text = String::from_utf8_lossy(&out.stderr).to_string();
    let mut mics = Vec::new();
    if cfg!(windows) {
        for line in text.lines().filter(|l| l.contains("(audio)")) {
            if let (Some(a), Some(b)) = (line.find('"'), line.rfind('"')) {
                if b > a {
                    mics.push(line[a + 1..b].to_string());
                }
            }
        }
    } else {
        let mut in_audio = false;
        for line in text.lines() {
            if line.contains("audio devices") {
                in_audio = true;
                continue;
            }
            if in_audio {
                if let Some(pos) = line.rfind("] ") {
                    mics.push(line[pos + 2..].trim().to_string());
                }
            }
        }
    }
    mics
}

/// avfoundation numbers screens after cameras: find "Capture screen N".
fn mac_screen_device(s: &Settings, screen_index: usize) -> String {
    let out = command(&s.ffmpeg())
        .args(["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""])
        .output();
    let needle = format!("Capture screen {screen_index}");
    if let Ok(out) = out {
        for line in String::from_utf8_lossy(&out.stderr).lines() {
            if line.contains(&needle) {
                if let (Some(a), Some(b)) = (line.rfind('['), line.rfind(']')) {
                    if b > a {
                        return line[a + 1..b].to_string();
                    }
                }
            }
        }
    }
    format!("Capture screen {screen_index}")
}

fn build_args(s: &Settings, r: &Recording, out: &Path) -> Vec<String> {
    let fps = s.fps.clamp(5, 60).to_string();
    let mic = s.record_mic;
    let g = r.grab;
    let mut a: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-nostats".into(), "-y".into()];
    let mut vf: Option<String> = None;
    if cfg!(windows) {
        a.extend(
            ["-f", "gdigrab", "-framerate", &fps, "-draw_mouse", "1", "-offset_x", &g.x.to_string(), "-offset_y", &g.y.to_string()]
                .map(String::from),
        );
        a.extend(["-video_size".into(), format!("{}x{}", g.w, g.h), "-i".into(), "desktop".into()]);
        let dev = if s.mic_device.is_empty() && mic { list_mics(s).into_iter().next().unwrap_or_default() } else { s.mic_device.clone() };
        if mic && !dev.is_empty() {
            a.extend(["-f".into(), "dshow".into(), "-i".into(), format!("audio={dev}")]);
        }
    } else if cfg!(target_os = "macos") {
        let screen = mac_screen_device(s, r.screen_index);
        let audio = if mic { if s.mic_device.is_empty() { "0".to_string() } else { s.mic_device.clone() } } else { "none".into() };
        a.extend(["-f", "avfoundation", "-capture_cursor", "1", "-framerate", &fps].map(String::from));
        a.extend(["-i".into(), format!("{screen}:{audio}")]);
        vf = Some(format!("crop={}:{}:{}:{}", r.crop.w, r.crop.h, r.crop.x, r.crop.y));
    } else {
        let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
        a.extend(["-f", "x11grab", "-framerate", &fps, "-draw_mouse", "1"].map(String::from));
        a.extend(["-video_size".into(), format!("{}x{}", g.w, g.h), "-i".into(), format!("{display}+{},{}", g.x, g.y)]);
        if mic {
            let dev = if s.mic_device.is_empty() { "default".to_string() } else { s.mic_device.clone() };
            a.extend(["-f".into(), "pulse".into(), "-i".into(), dev]);
        }
    }
    if let Some(vf) = vf {
        a.extend(["-vf".into(), vf]);
    }
    a.extend(["-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-pix_fmt", "yuv420p"].map(String::from));
    a.extend(["-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart"].map(String::from));
    a.push(out.to_string_lossy().to_string());
    a
}

/// Start a new ffmpeg segment. Fails fast if ffmpeg exits right away.
pub fn start_segment(s: &Settings, r: &mut Recording) -> Result<(), String> {
    let out = work_dir(&r.id).join(format!("seg{:03}.mp4", r.segments.len()));
    let mut child = command(&s.ffmpeg())
        .args(build_args(s, r, &out))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| format!("Could not start ffmpeg ({}). Install it or set its path in Settings.", s.ffmpeg()))?;
    // Drain stderr so ffmpeg never blocks on a full pipe; keep the tail for error messages.
    let log = Arc::new(Mutex::new(String::new()));
    if let Some(mut err) = child.stderr.take() {
        let log = log.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            while let Ok(n) = err.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let mut l = log.lock().unwrap();
                l.push_str(&String::from_utf8_lossy(&buf[..n]));
                if l.len() > 8000 {
                    let cut = l.len() - 4000;
                    l.drain(..cut);
                }
            }
        });
    }
    std::thread::sleep(Duration::from_millis(600));
    if let Ok(Some(_)) = child.try_wait() {
        std::thread::sleep(Duration::from_millis(100));
        let msg = log.lock().unwrap().trim().to_string();
        return Err(if msg.is_empty() { "ffmpeg stopped unexpectedly.".into() } else { format!("ffmpeg: {msg}") });
    }
    r.segments.push(out);
    r.child = Some((child, log));
    r.seg_started = Some(Instant::now());
    Ok(())
}

/// Ask ffmpeg to finish the current segment cleanly.
pub fn stop_segment(r: &mut Recording) {
    if let Some((mut child, _)) = r.child.take() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(b"q\n");
            let _ = stdin.flush();
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                _ if Instant::now() > deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                _ => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }
    if let Some(s) = r.seg_started.take() {
        r.elapsed += s.elapsed();
    }
}

/// Join segments into the final video, make a thumbnail and add it to history.
pub fn finalize(s: &Settings, mut r: Recording) -> Result<history::Item, String> {
    stop_segment(&mut r);
    let segs: Vec<PathBuf> = r.segments.iter().filter(|p| p.exists() && p.metadata().map(|m| m.len() > 0).unwrap_or(false)).cloned().collect();
    if segs.is_empty() {
        return Err("Nothing was recorded.".into());
    }
    let id = r.id.clone();
    let file = format!("{id}.mp4");
    let dest = captures_dir().join(&file);
    if segs.len() == 1 {
        std::fs::copy(&segs[0], &dest).map_err(|e| e.to_string())?;
    } else {
        let list = work_dir(&id).join("list.txt");
        let body: String = segs.iter().map(|p| format!("file '{}'\n", p.to_string_lossy().replace('\\', "/").replace('\'', "'\\''"))).collect();
        std::fs::write(&list, body).map_err(|e| e.to_string())?;
        let st = command(&s.ffmpeg())
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i"])
            .arg(&list)
            .args(["-c", "copy", "-movflags", "+faststart"])
            .arg(&dest)
            .status()
            .map_err(|e| e.to_string())?;
        if !st.success() {
            return Err("Could not join the recording segments.".into());
        }
    }
    let thumb = format!("{id}.thumb.jpg");
    let _ = command(&s.ffmpeg())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-ss", "0.3", "-i"])
        .arg(&dest)
        .args(["-frames:v", "1", "-vf", "scale=360:-2"])
        .arg(captures_dir().join(&thumb))
        .status();
    let _ = std::fs::remove_dir_all(work_dir(&id));
    let item = history::Item {
        id,
        kind: "video".into(),
        file,
        thumb,
        created: chrono::Local::now(),
        width: r.crop.w as u32,
        height: r.crop.h as u32,
        duration: r.elapsed.as_secs_f64(),
        url: None,
    };
    history::add(item.clone())?;
    Ok(item)
}

pub fn discard(mut r: Recording) {
    stop_segment(&mut r);
    let _ = std::fs::remove_dir_all(work_dir(&r.id));
}
