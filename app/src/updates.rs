//! Updates from GitHub Releases: the release workflow publishes a signed `latest.json`, and the
//! updater plugin checks it, downloads the installer, verifies the signature and installs it.
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};

/// The update found by the last check, kept so "Install" uses exactly that one.
#[derive(Default)]
pub struct Pending(pub Mutex<Option<Update>>);

#[derive(Clone, Serialize)]
pub struct Info {
    pub current: String,
    /// The newer version, if there is one.
    pub version: Option<String>,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Clone, Serialize)]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

pub async fn check<R: Runtime>(app: &AppHandle<R>) -> Result<Info, String> {
    let current = app.package_info().version.to_string();
    let found = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| tr!("Could not check for updates: {}", e))?;
    let info = Info {
        current,
        version: found.as_ref().map(|u| u.version.clone()),
        notes: found.as_ref().and_then(|u| u.body.clone()),
        date: found.as_ref().and_then(|u| u.date).map(|d| d.date().to_string()),
    };
    *app.state::<Pending>().0.lock().unwrap() = found;
    Ok(info)
}

/// Download, verify and install the update found by `check`, then restart into it.
pub async fn install<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let update = app.state::<Pending>().0.lock().unwrap().take().ok_or_else(|| tr!("Check for updates first."))?;
    let mut downloaded = 0u64;
    let progress = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.emit("update-progress", Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| tr!("The update could not be installed: {}", e))?;
    // Windows has already handed over to the installer, which starts the new version.
    app.restart();
}
