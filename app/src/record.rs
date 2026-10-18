//! Screen video recording through ffmpeg, with pause/resume implemented as segments.
use crate::{capture::Rect, codec, history, settings::captures_dir, settings::Settings};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// PCM format of the desktop-sound stream (see `loopback`).
pub const DESKTOP_RATE: usize = 48_000;
pub const DESKTOP_CHANNELS: usize = 2;

pub struct Recording {
    pub id: String,
    /// Region in the coordinates ffmpeg's grabber expects.
    pub grab: Rect,
    /// Region in image pixels relative to the monitor (used for macOS crop).
    pub crop: Rect,
    pub screen_index: usize,
    /// Monitor origin in `grab` coordinates (ddagrab offsets are relative to the monitor).
    pub mon_origin: (i32, i32),
    /// Windows: ddagrab failed once, so stay on the slower gdigrab.
    pub gdi_fallback: bool,
    /// Physical window rect of the region, for placing the border / camera / bar.
    pub screen_rect: Rect,
    pub segments: Vec<PathBuf>,
    pub child: Option<(Child, Arc<Mutex<String>>)>,
    #[cfg(windows)]
    pub loopback: Option<crate::loopback::Loopback>,
    pub seg_started: Option<Instant>,
    pub elapsed: Duration,
    pub opened_camera: bool,
    /// Output format id (see `codec::CODECS`), fixed when the recording starts.
    pub codec: String,
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
    let version = String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("ffmpeg").to_string();
    let bundled = s.ffmpeg_path.trim().is_empty() && crate::settings::bundled_ffmpeg().is_some();
    Ok(if bundled { format!("{version} (included with myCapture)") } else { version })
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

/// Windows: index of the DXGI output (on the default adapter, which ddagrab uses) at this desktop origin.
#[cfg(windows)]
fn dxgi_output(origin: (i32, i32)) -> Option<u32> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let adapter = factory.EnumAdapters1(0).ok()?;
        (0..16).map_while(|i| adapter.EnumOutputs(i).ok().map(|o| (i, o))).find_map(|(i, o)| {
            let r = o.GetDesc().ok()?.DesktopCoordinates;
            (r.left == origin.0 && r.top == origin.1).then_some(i)
        })
    }
}

#[cfg(not(windows))]
fn dxgi_output(_: (i32, i32)) -> Option<u32> {
    None
}

/// Audio sources for this segment, as ffmpeg input arguments.
fn audio_inputs(s: &Settings, desktop: Option<&str>) -> Vec<Vec<String>> {
    let q = |mut v: Vec<String>| {
        v.splice(0..0, ["-thread_queue_size".to_string(), "1024".to_string()]);
        v
    };
    let mut inputs = Vec::new();
    if cfg!(windows) {
        if s.record_mic {
            let dev = if s.mic_device.is_empty() { list_mics(s).into_iter().next().unwrap_or_default() } else { s.mic_device.clone() };
            if !dev.is_empty() {
                // A small device buffer and a large real-time buffer keep dshow from stalling the screen grab.
                inputs.push(q(["-f", "dshow", "-rtbufsize", "64M", "-audio_buffer_size", "50", "-i"].map(String::from).into_iter().chain([format!("audio={dev}")]).collect()));
            }
        }
        if let Some(url) = desktop {
            let (rate, ch) = (DESKTOP_RATE.to_string(), DESKTOP_CHANNELS.to_string());
            // Raw PCM arrives in real time: probing it would hold up every other input for seconds.
            inputs.push(q(vec!["-analyzeduration".into(), "0".into(), "-probesize".into(), "32".into(), "-f".into(), "f32le".into(), "-ar".into(), rate, "-ac".into(), ch, "-i".into(), url.into()]));
        }
    } else if cfg!(target_os = "linux") {
        if s.record_mic {
            let dev = if s.mic_device.is_empty() { "default".to_string() } else { s.mic_device.clone() };
            inputs.push(q(vec!["-f".into(), "pulse".into(), "-i".into(), dev]));
        }
        if s.record_system_audio {
            inputs.push(q(vec!["-f".into(), "pulse".into(), "-i".into(), "@DEFAULT_MONITOR@".into()]));
        }
    }
    inputs
}

fn build_args(s: &Settings, r: &Recording, out: &Path, desktop: Option<&str>) -> Vec<String> {
    let fps = s.fps.clamp(5, 60).to_string();
    let g = r.grab;
    let mut a: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-nostats".into(), "-y".into()];
    a.extend(["-thread_queue_size".into(), "1024".into()]);
    let mut vf: Option<String> = None;
    let mut audio = audio_inputs(s, desktop);
    if cfg!(windows) {
        let output = if r.gdi_fallback { None } else { dxgi_output(r.mon_origin) };
        if let Some(idx) = output {
            // Desktop Duplication: GPU capture that keeps up at full frame rate.
            let (ox, oy) = (g.x - r.mon_origin.0, g.y - r.mon_origin.1);
            a.extend(["-f".into(), "lavfi".into(), "-i".into()]);
            a.push(format!("ddagrab=output_idx={idx}:framerate={fps}:draw_mouse=1:offset_x={ox}:offset_y={oy}:video_size={}x{}", g.w, g.h));
            vf = Some("hwdownload,format=bgra".into());
        } else {
            a.extend(
                ["-f", "gdigrab", "-framerate", &fps, "-draw_mouse", "1", "-offset_x", &g.x.to_string(), "-offset_y", &g.y.to_string()]
                    .map(String::from),
            );
            a.extend(["-video_size".into(), format!("{}x{}", g.w, g.h), "-i".into(), "desktop".into()]);
        }
    } else if cfg!(target_os = "macos") {
        let screen = mac_screen_device(s, r.screen_index);
        let mic = if s.record_mic { if s.mic_device.is_empty() { "0".to_string() } else { s.mic_device.clone() } } else { "none".into() };
        a.extend(["-f", "avfoundation", "-capture_cursor", "1", "-framerate", &fps].map(String::from));
        a.extend(["-i".into(), format!("{screen}:{mic}")]);
        vf = Some(format!("crop={}:{}:{}:{}", r.crop.w, r.crop.h, r.crop.x, r.crop.y));
        audio.clear();
    } else {
        let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
        a.extend(["-f", "x11grab", "-framerate", &fps, "-draw_mouse", "1"].map(String::from));
        a.extend(["-video_size".into(), format!("{}x{}", g.w, g.h), "-i".into(), format!("{display}+{},{}", g.x, g.y)]);
    }
    let n_audio = audio.len();
    a.extend(audio.into_iter().flatten());
    let filters: Vec<String> = vf.into_iter().chain(codec::scale_filter(s)).collect();
    if !filters.is_empty() {
        a.extend(["-vf".into(), filters.join(",")]);
    }
    if n_audio > 1 {
        // Microphone and desktop sound mixed into one track, each at its own level.
        let ins: String = (1..=n_audio).map(|i| format!("[{i}:a]")).collect();
        a.extend(["-filter_complex".into(), format!("{ins}amix=inputs={n_audio}:duration=longest:dropout_transition=0:normalize=0[aout]")]);
        a.extend(["-map".into(), "0:v".into(), "-map".into(), "[aout]".into()]);
    }
    a.extend(codec::capture_args(codec::get(&r.codec), s));
    a.push(out.to_string_lossy().to_string());
    a
}

/// Start a new ffmpeg segment. Fails fast if ffmpeg exits right away.
pub fn start_segment(s: &Settings, r: &mut Recording) -> Result<(), String> {
    match spawn_segment(s, r) {
        Err(e) if cfg!(windows) && !r.gdi_fallback && dxgi_output(r.mon_origin).is_some() => {
            // Desktop Duplication can be unavailable (remote sessions, some drivers): use GDI.
            r.gdi_fallback = true;
            spawn_segment(s, r).map_err(|_| e)
        }
        res => res,
    }
}

fn spawn_segment(s: &Settings, r: &mut Recording) -> Result<(), String> {
    // Matroska segments stay playable even if the app or ffmpeg dies mid-recording.
    let out = work_dir(&r.id).join(format!("seg{:03}.mkv", r.segments.len()));
    #[cfg(windows)]
    let loopback = if s.record_system_audio { Some(crate::loopback::Loopback::start()?) } else { None };
    #[cfg(windows)]
    let desktop = loopback.as_ref().map(|l| l.url.clone());
    #[cfg(not(windows))]
    let desktop: Option<String> = None;
    let mut child = command(&s.ffmpeg())
        .args(build_args(s, r, &out, desktop.as_deref()))
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
        #[cfg(windows)]
        if let Some(l) = loopback {
            l.stop();
        }
        std::thread::sleep(Duration::from_millis(100));
        let msg = log.lock().unwrap().trim().to_string();
        return Err(if msg.is_empty() { "ffmpeg stopped unexpectedly.".into() } else { format!("ffmpeg: {msg}") });
    }
    r.segments.push(out);
    r.child = Some((child, log));
    #[cfg(windows)]
    {
        r.loopback = loopback;
    }
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
    #[cfg(windows)]
    if let Some(l) = r.loopback.take() {
        l.stop();
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
    let target = codec::get(&r.codec);
    let file = format!("{id}.{}", target.ext);
    let dest = captures_dir().join(&file);
    // Join (or just remux) the segments into a web-friendly MP4.
    let list = work_dir(&id).join("list.txt");
    let body: String = segs
        .iter()
        .map(|p| format!("file '{}'\n", p.to_string_lossy().replace('\\', "/").replace('\'', "'\\''")))
        .collect();
    std::fs::write(&list, body).map_err(|e| e.to_string())?;
    let mut cmd = command(&s.ffmpeg());
    cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i"]).arg(&list);
    match target.mode {
        codec::Mode::Direct => {
            cmd.args(["-c", "copy"]);
        }
        codec::Mode::Transcode => {
            cmd.args(codec::video_args(target, s)).args(codec::audio_args(target));
        }
        codec::Mode::Gif => {
            cmd.args(["-filter_complex", &codec::gif_filter(s), "-loop", "0"]).args(codec::audio_args(target));
        }
    }
    if target.ext == "mp4" {
        cmd.args(["-movflags", "+faststart"]);
    }
    let st = cmd
        .arg(&dest)
        .stdin(Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    if !st.success() {
        return Err(format!("Could not finish the video. The raw recording is kept in {}", work_dir(&id).display()));
    }
    let thumb = format!("{id}.thumb.jpg");
    let _ = command(&s.ffmpeg())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-ss", "0.3", "-i"])
        .arg(&dest)
        .args(["-frames:v", "1", "-vf", "scale=360:-2"])
        .arg(captures_dir().join(&thumb))
        .stdin(Stdio::null())
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// Records a few seconds of the primary screen with mic + desktop sound and checks the result.
    /// Run with `cargo test -- --ignored live_recording` while sound is playing.
    #[test]
    #[ignore]
    fn live_recording() {
        let mut s = Settings::default();
        s.video_codec = "h264".into();
        let mode = std::env::var("LIVE_MODE").unwrap_or_default();
        s.record_mic = mode != "desktop";
        s.record_system_audio = mode != "mic";
        let mut r = Recording {
            id: "test-live".into(),
            grab: Rect { x: 0, y: 0, w: 1280, h: 720 },
            crop: Rect { x: 0, y: 0, w: 1280, h: 720 },
            screen_index: 0,
            mon_origin: (0, 0),
            gdi_fallback: false,
            screen_rect: Rect { x: 0, y: 0, w: 1280, h: 720 },
            segments: vec![],
            child: None,
            loopback: None,
            seg_started: None,
            elapsed: Duration::ZERO,
            opened_camera: false,
            codec: s.video_codec.clone(),
        };
        start_segment(&s, &mut r).expect("start");
        println!("gdi fallback: {}", r.gdi_fallback);
        std::thread::sleep(Duration::from_secs(6));
        stop_segment(&mut r);
        let seg = r.segments[0].clone();
        let out = std::env::temp_dir().join(format!("mycapture-live{mode}.mkv"));
        std::fs::copy(&seg, &out).unwrap();
        println!("{}", out.display());
        discard(r);
    }
}
