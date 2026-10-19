//! Video formats the user can pick, and the ffmpeg arguments behind them.
use crate::i18n;
use crate::record::command;
use crate::settings::Settings;
use serde::Serialize;

/// How a format is produced.
#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    /// Encoded while recording; finishing only remuxes (fast).
    Direct,
    /// Recorded to a fast intermediate, converted when the recording finishes.
    Transcode,
    /// Converted to an animated GIF when the recording finishes.
    Gif,
}

pub struct Codec {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub desc: &'static str,
    /// ffmpeg encoder name used for the availability check.
    pub encoder: &'static str,
    pub ext: &'static str,
    pub mode: Mode,
    /// Hardware encoders are verified with a one-frame test encode.
    pub hardware: bool,
}

pub const CODECS: &[Codec] = &[
    Codec { id: "h264", label: "H.264 MP4", group: "Software", desc: "Plays everywhere: WhatsApp, Teams, browsers, phones.", encoder: "libx264", ext: "mp4", mode: Mode::Direct, hardware: false },
    Codec { id: "hevc", label: "H.265 / HEVC MP4", group: "Software", desc: "About half the size of H.264. Plays on recent Windows, macOS, iPhone and Android; some web apps can't preview it.", encoder: "libx265", ext: "mp4", mode: Mode::Transcode, hardware: false },
    Codec { id: "av1", label: "AV1 MP4", group: "Software", desc: "Smallest files. Needs recent browsers and apps; converting takes longer when you finish.", encoder: "libsvtav1", ext: "mp4", mode: Mode::Transcode, hardware: false },
    Codec { id: "vp9", label: "VP9 WebM", group: "Software", desc: "Open web format with Opus audio. Good for websites; not accepted by WhatsApp.", encoder: "libvpx-vp9", ext: "webm", mode: Mode::Transcode, hardware: false },
    Codec { id: "gif", label: "Animated GIF", group: "Other", desc: "Loops inline in any chat or email. No sound, and best kept short.", encoder: "gif", ext: "gif", mode: Mode::Gif, hardware: false },
    Codec { id: "h264_nvenc", label: "H.264 MP4 · NVIDIA GPU", group: "Hardware (GPU)", desc: "H.264 encoded on an NVIDIA graphics card: very light on the CPU.", encoder: "h264_nvenc", ext: "mp4", mode: Mode::Direct, hardware: true },
    Codec { id: "h264_qsv", label: "H.264 MP4 · Intel Quick Sync", group: "Hardware (GPU)", desc: "H.264 encoded on Intel graphics: very light on the CPU.", encoder: "h264_qsv", ext: "mp4", mode: Mode::Direct, hardware: true },
    Codec { id: "h264_amf", label: "H.264 MP4 · AMD GPU", group: "Hardware (GPU)", desc: "H.264 encoded on an AMD graphics card: very light on the CPU.", encoder: "h264_amf", ext: "mp4", mode: Mode::Direct, hardware: true },
    Codec { id: "h264_videotoolbox", label: "H.264 MP4 · Apple VideoToolbox", group: "Hardware (GPU)", desc: "H.264 encoded by the Mac's media engine.", encoder: "h264_videotoolbox", ext: "mp4", mode: Mode::Direct, hardware: true },
    Codec { id: "hevc_nvenc", label: "H.265 MP4 · NVIDIA GPU", group: "Hardware (GPU)", desc: "Small HEVC files encoded on an NVIDIA graphics card.", encoder: "hevc_nvenc", ext: "mp4", mode: Mode::Direct, hardware: true },
    Codec { id: "hevc_qsv", label: "H.265 MP4 · Intel Quick Sync", group: "Hardware (GPU)", desc: "Small HEVC files encoded on Intel graphics.", encoder: "hevc_qsv", ext: "mp4", mode: Mode::Direct, hardware: true },
    Codec { id: "hevc_amf", label: "H.265 MP4 · AMD GPU", group: "Hardware (GPU)", desc: "Small HEVC files encoded on an AMD graphics card.", encoder: "hevc_amf", ext: "mp4", mode: Mode::Direct, hardware: true },
    Codec { id: "hevc_videotoolbox", label: "H.265 MP4 · Apple VideoToolbox", group: "Hardware (GPU)", desc: "Small HEVC files encoded by the Mac's media engine.", encoder: "hevc_videotoolbox", ext: "mp4", mode: Mode::Direct, hardware: true },
];

pub fn get(id: &str) -> &'static Codec {
    CODECS.iter().find(|c| c.id == id).unwrap_or(&CODECS[0])
}

/// 0 = smaller file, 1 = balanced, 2 = best quality.
fn level(s: &Settings) -> usize {
    match s.video_quality.as_str() {
        "small" => 0,
        "high" => 2,
        _ => 1,
    }
}

fn pick<T: ToString>(s: &Settings, v: [T; 3]) -> String {
    v[level(s)].to_string()
}

/// Video encoder arguments for the final format.
pub fn video_args(c: &Codec, s: &Settings) -> Vec<String> {
    let v: Vec<String> = match c.id {
        "h264" => vec!["-c:v".into(), "libx264".into(), "-preset".into(), "veryfast".into(), "-crf".into(), pick(s, [28, 23, 18])],
        "hevc" => vec!["-c:v".into(), "libx265".into(), "-preset".into(), "fast".into(), "-crf".into(), pick(s, [30, 26, 22])],
        "av1" => vec!["-c:v".into(), "libsvtav1".into(), "-preset".into(), "8".into(), "-crf".into(), pick(s, [42, 35, 28])],
        "vp9" => vec![
            "-c:v".into(), "libvpx-vp9".into(), "-b:v".into(), "0".into(), "-crf".into(), pick(s, [40, 33, 26]),
            "-deadline".into(), "good".into(), "-cpu-used".into(), "4".into(), "-row-mt".into(), "1".into(),
        ],
        "h264_nvenc" | "hevc_nvenc" => vec![
            "-c:v".into(), c.encoder.into(), "-preset".into(), "p4".into(), "-rc".into(), "vbr".into(),
            "-cq".into(), pick(s, [32, 26, 20]), "-b:v".into(), "0".into(),
        ],
        "h264_qsv" | "hevc_qsv" => vec!["-c:v".into(), c.encoder.into(), "-global_quality".into(), pick(s, [32, 26, 20])],
        "h264_amf" | "hevc_amf" => {
            let q = [32, 26, 20][level(s)];
            vec!["-c:v".into(), c.encoder.into(), "-rc".into(), "cqp".into(), "-qp_i".into(), q.to_string(), "-qp_p".into(), (q + 2).to_string()]
        }
        "h264_videotoolbox" | "hevc_videotoolbox" => vec!["-c:v".into(), c.encoder.into(), "-q:v".into(), pick(s, [45, 60, 75])],
        _ => vec![],
    };
    let mut v = v;
    v.extend(["-pix_fmt".into(), if c.id.ends_with("_qsv") { "nv12".into() } else { "yuv420p".into() }]);
    if c.id.starts_with("hevc") {
        v.extend(["-tag:v".into(), "hvc1".into()]);
    }
    v
}

pub fn audio_args(c: &Codec) -> Vec<String> {
    match c.ext {
        "webm" => vec!["-c:a".into(), "libopus".into(), "-b:a".into(), "96k".into()],
        "gif" => vec!["-an".into()],
        _ => vec!["-c:a".into(), "aac".into(), "-b:a".into(), "128k".into()],
    }
}

/// Encoder arguments used while recording (into Matroska segments).
pub fn capture_args(c: &Codec, s: &Settings) -> Vec<String> {
    if c.mode == Mode::Direct {
        let mut a = video_args(c, s);
        a.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "128k".into()]);
        a
    } else {
        // Near-lossless and cheap, so recording keeps up; quality is decided at conversion.
        ["-c:v", "libx264", "-preset", "ultrafast", "-crf", "16", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k"]
            .map(String::from)
            .to_vec()
    }
}

/// Optional downscale filter (`scale=...`) for the Max resolution setting.
pub fn scale_filter(s: &Settings) -> Option<String> {
    (s.max_height > 0).then(|| format!("scale=-2:'min(ih,{})':flags=lanczos", s.max_height))
}

/// GIF filter chain: frame rate, size limit, and a per-recording palette.
pub fn gif_filter(s: &Settings) -> String {
    let h = if s.max_height > 0 { s.max_height.min(720) } else { 720 };
    format!(
        "fps={},scale=-2:'min(ih,{h})':flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4",
        s.gif_fps.clamp(5, 30)
    )
}

#[derive(Serialize)]
pub struct CodecInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub desc: &'static str,
    pub available: bool,
}

/// Which formats this ffmpeg can produce. Hardware encoders get a tiny test encode,
/// since ffmpeg lists them even when there is no matching GPU.
pub fn available(s: &Settings) -> Vec<CodecInfo> {
    let listed = command(&s.ffmpeg())
        .args(["-hide_banner", "-encoders"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let has = |enc: &str| listed.lines().any(|l| l.split_whitespace().nth(1) == Some(enc));
    let handles: Vec<_> = CODECS
        .iter()
        .map(|c| {
            let present = has(c.encoder);
            let ffmpeg = s.ffmpeg();
            let probe = c.hardware && present;
            let enc = c.encoder;
            std::thread::spawn(move || {
                if !probe {
                    return present;
                }
                command(&ffmpeg)
                    .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=black:s=320x240:d=0.2", "-frames:v", "2", "-c:v", enc, "-f", "null", "-"])
                    .stdin(std::process::Stdio::null())
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
            })
        })
        .collect();
    CODECS
        .iter()
        .zip(handles)
        .map(|(c, h)| CodecInfo { id: c.id, label: i18n::t(c.label), group: i18n::t(c.group), desc: i18n::t(c.desc), available: h.join().unwrap_or(false) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes a short synthetic clip into every available format with the app's own arguments.
    #[test]
    fn every_available_format_encodes() {
        let mut s = Settings::default();
        let dir = std::env::temp_dir().join("mycapture-codec-test");
        let _ = std::fs::create_dir_all(&dir);
        let src = dir.join("src.mkv");
        let ok = command(&s.ffmpeg())
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=640x360:d=2:r=30", "-f", "lavfi", "-i", "sine=d=2"])
            .args(capture_args(get("hevc"), &s))
            .arg(&src)
            .status()
            .map(|st| st.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("ffmpeg not available; skipping");
            return;
        }
        for info in available(&s) {
            println!("{:<20} {}", info.id, if info.available { "available" } else { "-" });
            if !info.available {
                continue;
            }
            let c = get(info.id);
            s.video_codec = c.id.into();
            let out = dir.join(format!("out_{}.{}", c.id, c.ext));
            let mut cmd = command(&s.ffmpeg());
            cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-i"]).arg(&src);
            match c.mode {
                Mode::Gif => cmd.args(["-filter_complex", &gif_filter(&s), "-loop", "0"]).args(audio_args(c)),
                _ => cmd.args(video_args(c, &s)).args(audio_args(c)),
            };
            let st = cmd.arg(&out).status().unwrap();
            let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
            println!("    -> {} bytes", size);
            assert!(st.success() && size > 0, "{} failed", c.id);
        }
    }
}
