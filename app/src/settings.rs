use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UploadSettings {
    /// "none" | "dropbox" | "onedrive" | "gdrive" | "server" | "s3" | "http"
    pub provider: String,
    pub server_url: String,
    pub server_token: String,
    pub s3_endpoint: String,
    pub s3_region: String,
    pub s3_bucket: String,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    pub s3_prefix: String,
    pub s3_public_url: String,
    pub s3_path_style: bool,
    pub http_url: String,
    pub http_field: String,
    /// One `Header: value` per line.
    pub http_headers: String,
    /// Dotted JSON path to the URL in the response (e.g. `data.link`); empty = whole body.
    pub http_url_path: String,
    /// Own OAuth apps for the cloud drives (empty = the key built into this release).
    pub dropbox_app_key: String,
    pub onedrive_client_id: String,
    pub gdrive_client_id: String,
    pub gdrive_client_secret: String,
}

impl Default for UploadSettings {
    fn default() -> Self {
        Self {
            provider: "none".into(),
            server_url: String::new(),
            server_token: String::new(),
            s3_endpoint: String::new(),
            s3_region: "auto".into(),
            s3_bucket: String::new(),
            s3_access_key: String::new(),
            s3_secret_key: String::new(),
            s3_prefix: "mycapture/".into(),
            s3_public_url: String::new(),
            s3_path_style: true,
            http_url: String::new(),
            http_field: "file".into(),
            http_headers: String::new(),
            http_url_path: String::new(),
            dropbox_app_key: String::new(),
            onedrive_client_id: String::new(),
            gdrive_client_id: String::new(),
            gdrive_client_secret: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub hotkey: String,
    /// "auto" (system language) | "en" | "es"
    pub language: String,
    pub ffmpeg_path: String,
    pub fps: u32,
    pub record_mic: bool,
    pub mic_device: String,
    /// Also record what plays through the speakers (Windows loopback, PulseAudio monitor on Linux).
    pub record_system_audio: bool,
    pub camera_on_record: bool,
    pub camera_size: u32,
    pub camera_shape: String,
    pub countdown: u32,
    /// Output format id, see `codec::CODECS`.
    pub video_codec: String,
    /// "small" | "balanced" | "high"
    pub video_quality: String,
    /// Downscale recordings taller than this (0 = keep original size).
    pub max_height: u32,
    pub gif_fps: u32,
    /// What to put on the clipboard right after a screenshot: "none" | "image" | "link"
    pub auto_copy: String,
    pub show_moon: bool,
    /// How fast the moon's surface turns: "still" | "slow" | "normal" | "fast" | "faster"
    pub moon_speed: String,
    /// Which body the launcher shows: "moon" | "sun" | "mercury" | … | "pluto" (see ui/bodies.js), or "random" to roll one
    pub moon_body: String,
    /// Look for a new version on GitHub when myCapture starts.
    pub check_updates: bool,
    /// Newest version Settings was already opened for, so a found update is announced once.
    pub announced_update: String,
    /// Launch myCapture when the user signs in.
    pub start_at_login: bool,
    /// Screen edge the moon docks to: "top" | "bottom" | "left" | "right"
    pub moon_edge: String,
    /// Last drop point of the moon (physical px); picks the monitor and the spot along the edge.
    pub moon_x: Option<i32>,
    pub moon_y: Option<i32>,
    pub upload: UploadSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "CommandOrControl+Shift+J".into(),
            language: "auto".into(),
            ffmpeg_path: String::new(),
            fps: 30,
            record_mic: true,
            mic_device: String::new(),
            record_system_audio: true,
            camera_on_record: false,
            camera_size: 220,
            camera_shape: "circle".into(),
            countdown: 3,
            video_codec: "h264".into(),
            video_quality: "balanced".into(),
            max_height: 0,
            gif_fps: 12,
            auto_copy: "image".into(),
            show_moon: true,
            moon_speed: "normal".into(),
            moon_body: "moon".into(),
            check_updates: true,
            announced_update: String::new(),
            start_at_login: true,
            moon_edge: "top".into(),
            moon_x: None,
            moon_y: None,
            upload: UploadSettings::default(),
        }
    }
}

pub fn config_dir() -> PathBuf {
    let d = dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("myCapture");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn captures_dir() -> PathBuf {
    let d = dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("myCapture").join("captures");
    let _ = std::fs::create_dir_all(&d);
    d
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(config_dir().join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(config_dir().join("settings.json"), s).map_err(|e| e.to_string())
    }

    /// ffmpeg to run: the configured path, else the copy shipped with the installer, else PATH.
    pub fn ffmpeg(&self) -> String {
        if !self.ffmpeg_path.trim().is_empty() {
            return self.ffmpeg_path.trim().into();
        }
        bundled_ffmpeg().unwrap_or_else(|| "ffmpeg".into())
    }
}

pub fn bundled_ffmpeg() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    [dir.join(name), dir.join("resources").join(name), dir.join("../Resources").join(name)]
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().to_string())
}
