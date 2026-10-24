# myCapture

A Jing-style screen capture tool written in Rust ([Tauri 2](https://tauri.app)), for Windows, macOS and Linux.

A shiny little moon peeks out from the edge of your screen, half showing. Hover it and three buttons pop out: **Capture**, **History** and **More**.

![moon](app/icons/128x128.png)

## Features

- **Screenshots**: every monitor freezes at once, so you can drag an area or click a window on any screen, then fine-tune the selection with resize handles and a magnifier.
- **Fast save**: turn on Settings → General → **Fast save** and a screenshot is saved the moment you finish picking the area (drag or click a window): no toolbar, no editor. Your *After a screenshot* clipboard choice still applies. Turn it off to get the Image / Video / Scrolling toolbar and the editor back.
- **Your own captures folder**: Settings → General → **Captures folder** → **Change…** saves new screenshots and videos to any folder you pick; **Use default** goes back to the built-in one. History keeps working for captures in either place.
- **Annotate**: Snagit-style arrows (classic, double, curved, block, tapered, dashed, pointer, line) and callouts (filled bubble, outlined bubble, square label, speech balloon), plus boxes, ellipses, highlighter, pen, text, numbered steps and pixelate, with undo and redo. Everything stays editable: click an annotation to select it, drag it to move it, drag its handles (arrow ends, a callout's pointer, box corners), double-click text or a callout to retype it, and press Delete to remove it. Colour, thickness and style changes apply to whatever is selected, and undo/redo covers every edit. Annotations can start or end outside the screenshot, and the canvas grows to fit them.
- **Scrolling capture**: select an area (or click a window), choose **Scrolling** (or press **S**) and scroll the page; myCapture stitches the frames into one tall image. Sticky headers and footers appear once. On Windows, **Auto-scroll** scrolls for you and stops at the bottom. Finish with **Done**, Enter or the capture hotkey.
- **Screen video**: record any area with your microphone and the desktop sound (whatever your speakers play), mixed into one track. Each can be switched on or off in Settings. On Windows the screen is captured with Desktop Duplication (GPU), so recordings keep a steady frame rate. The format is your choice: **H.264 MP4** (default; H.264 at CRF 23 + AAC 128 kbps, so roughly 5–15 MB per minute of desktop work), **H.265/HEVC**, **AV1**, **VP9 WebM** or **animated GIF**. GPU encoding (NVIDIA NVENC, Intel Quick Sync, AMD AMF, Apple VideoToolbox) is also available, along with Smaller / Balanced / Best quality and a maximum resolution. Only formats your ffmpeg can actually produce are offered. Recordings are written as crash-safe MKV segments and joined into the MP4 when you finish. You can pause, resume, finish or discard, and there is a countdown before it starts.
- **Webcam bubble**: puts your camera in a corner of the recorded area. It can be a circle or a rounded square, you can resize it, and you can drag it anywhere. It is recorded along with the screen.
- **Paste anywhere**: screenshots go to the clipboard as images, and videos and images can be copied as files. Paste them straight into WhatsApp, Teams, Slack, email and so on.
- **Share links**: upload to **Dropbox**, **OneDrive** or **Google Drive** (sign in once in your browser), to the bundled self-hosted server, to S3-compatible storage (Cloudflare R2, AWS S3, Backblaze B2, MinIO) or to any multipart upload endpoint. The link is copied to your clipboard.
- **Updates from GitHub**: Settings → About checks the latest GitHub release, shows its notes, and installs the new version with one click, restarting into it. It checks automatically at startup (can be turned off) and opens About once when something new is out. Updates are signed, so only builds from this repository are installed.
- **English and Spanish**: Settings → General → Language follows the system by default, or pick English or Español. Menus, buttons, messages and the tray menu all switch.
- **History** of every capture, with copy, share and delete.
- Global hotkey (default `Ctrl/⌘+Shift+J`), a tray icon, and a dockable moon you can drag to the top, bottom, left or right edge of any monitor.
- Pick the launcher's look in **Settings → General → Launcher look**: the moon (default), the sun, Mercury, Venus, Earth, Mars, Jupiter, Saturn, Uranus, Neptune, Pluto, the dwarf planet Ceres, a comet with sparkles streaming down its tails, or **Random**, which rolls a different body when the app starts and then on the schedule you pick under **Change the random pick** (every 5, 15 or 30 minutes, hourly — the default —, every 6 hours, daily, or only at start). Each one turns slowly and speeds up on hover.

## Install

Download the installer for your platform from [Releases](https://github.com/robertorenz/mycapture/releases):

- **Windows**: `myCapture_x.y.z_x64-setup.exe`. It installs per user (no admin rights needed), includes ffmpeg, adds a Start-menu shortcut and an uninstaller, and can start myCapture when you sign in. An `.msi` is available for managed deployment.
- **macOS**: `.dmg` for Apple Silicon or Intel. Install ffmpeg with `brew install ffmpeg`.
- **Linux**: `.deb` (pulls in ffmpeg), `.rpm` or `.AppImage`.

## Requirements (building from source)

- Rust (stable).
- **ffmpeg**, for video recording. It must be on your `PATH`, or you can set its path in Settings → Recording.
  - Windows: `winget install Gyan.FFmpeg`
  - macOS: `brew install ffmpeg`
  - Linux: `sudo apt install ffmpeg`
- Linux build libraries:
  ```sh
  sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev \
    libssl-dev libxcb1-dev libxrandr-dev libdbus-1-dev libpipewire-0.3-dev libwayland-dev libegl-dev libgbm-dev
  ```

## Build and run

```sh
cargo run -p mycapture            # the desktop app
cargo build --release --workspace # release binaries in target/release
```

To build installers (`.msi`/`.exe`, `.dmg`/`.app`, `.deb`/`.AppImage`), install the Tauri CLI and bundle:

```sh
cargo install tauri-cli --version "^2"
cd app && cargo tauri build
```

To ship ffmpeg inside the Windows installer, as the release workflow does, put `ffmpeg.exe` in `app/bin/` and build with `cargo tauri build --config tauri.ffmpeg.conf.json`.

To cut a release, bump the version in `app/tauri.conf.json` and both `Cargo.toml` files, then push a tag such as `v0.2.0`. GitHub Actions builds every installer and attaches it to a draft release.

GitHub Actions builds the app and the server on all three platforms on every push.

## Releases and updates

Push a tag like `v0.3.0` and the release workflow builds every platform, signs the installers for the updater, attaches `latest.json`, and publishes the release. Installed copies (0.3.0 or later) find it at `releases/latest/download/latest.json`.

Signing needs the repository secret `TAURI_SIGNING_PRIVATE_KEY` (the private key created with `cargo tauri signer generate`; the matching public key is in `app/tauri.conf.json`). Keep a backup of the private key: without it, future releases can't be signed and installed copies won't accept them. For a signed local build:

```sh
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/mycapture-updater.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" cargo tauri build
```

## Cloud drives (Dropbox, OneDrive, Google Drive)

Open Settings → Sharing, pick the service and click **Connect**. Set up as many destinations as you like and star one with **Make default**: the **Share** button uploads there, and its arrow menu lets you pick any other destination you set up, or star a new default. Your browser opens the service's sign-in page and then returns to myCapture on `http://localhost:53682/`. Captures go into a `myCapture` folder and are shared with a view-only link. Sign-ins are stored in `cloud.json` next to `settings.json`, not in the settings file itself.

Each service needs an OAuth app registration. A release build picks the keys up from these environment variables (GitHub secrets in the release workflow): `MYCAPTURE_DROPBOX_KEY`, `MYCAPTURE_ONEDRIVE_CLIENT_ID`, `MYCAPTURE_GDRIVE_CLIENT_ID` and `MYCAPTURE_GDRIVE_SECRET`. If a key isn't built in, the Sharing page shows step-by-step instructions for registering your own app and a field to paste its key.

| Service | Register at | Redirect URI | Permissions |
| --- | --- | --- | --- |
| Dropbox | dropbox.com/developers/apps (Scoped access, App folder) | `http://localhost:53682/` | `files.content.write`, `sharing.write` |
| OneDrive | Microsoft Entra → App registrations (any org + personal accounts) | Public client: `http://localhost` | `Files.ReadWrite`, `User.Read`, `offline_access` (asked at sign-in) |
| Google Drive | Google Cloud console → Credentials → OAuth client ID, type Desktop app; enable the Drive API | none needed | `drive.file` (only files myCapture creates) |

## Sharing server

`mycapture-server` is a small upload server that you can run on any VPS or home server, usually behind HTTPS. Each link opens a page with Open Graph tags, so WhatsApp and Teams show a preview thumbnail.

```sh
MYCAPTURE_TOKEN=pick-a-secret \
MYCAPTURE_PUBLIC_URL=https://share.example.com \
MYCAPTURE_DATA=/var/lib/mycapture \
MYCAPTURE_ADDR=0.0.0.0:8080 \
mycapture-server
```

To connect the app, open Settings → Sharing, choose **myCapture server**, and enter the URL and token.

| Variable | Default | Meaning |
| --- | --- | --- |
| `MYCAPTURE_TOKEN` | *(required)* | Bearer token that clients must send |
| `MYCAPTURE_PUBLIC_URL` | `http://localhost:8080` | Base URL used in links |
| `MYCAPTURE_DATA` | `./data` | Storage folder |
| `MYCAPTURE_ADDR` | `0.0.0.0:8080` | Listen address |
| `MYCAPTURE_MAX_MB` | `1024` | Upload size limit |

## Using it

Run `mycapture --capture` to start a capture from a script, a Stream Deck or a custom shortcut. If myCapture is already running, the running copy does the capture.


| Where | Action |
| --- | --- |
| Moon | Hover to open the menu, click to capture, drag to dock it on another edge |
| Capture screen | Drag to select an area, click a window to select it, **Enter** for an image, **V** for a video, **Esc** to cancel |
| Recording bar | Pause/resume, camera, **Finish**, discard; the hotkey also finishes the recording |
| Editor | `V` select/edit, `A C R E H P T N B` drawing tools (C = callout: drag from the spot you're pointing at to where the bubble goes), `Delete` removes the selection, `Ctrl+Z`/`Ctrl+Y` undo/redo, `Ctrl+C` copy, `Ctrl+S` save as |

Captures are stored in:

- Windows: `%APPDATA%\myCapture\captures`
- macOS: `~/Library/Application Support/myCapture/captures`
- Linux: `~/.local/share/myCapture/captures`

## Platform notes

- **Windows**: fully tested. The recording frame and control bar are hidden from the recording.
- **macOS**: the first time you use myCapture, grant it *Screen Recording*, *Camera* and *Microphone* access in System Settings → Privacy & Security. Video is recorded with avfoundation. macOS does not let apps record system sound, so desktop sound needs a loopback device such as BlackHole.
- **Linux**: needs an X11 session (ffmpeg `x11grab`), and audio comes from PulseAudio/PipeWire `default`, and desktop sound from the default output's monitor. Wayland sessions can take screenshots, but cannot record video yet.

## Layout

```
app/      Tauri app (Rust): capture, recording, clipboard, upload, windows
ui/       Plain HTML/CSS/JS for the moon, capture overlay, editor, history, settings, camera
server/   mycapture-server (axum)
```

## Tests

```sh
cargo test -p mycapture          # encoder test for every available video format
cd tests/ui && npm install && npm test   # headless editor tests (uses local Chrome; set CHROME_PATH)
TEST_MP4=path/to/recording.mp4 node tests/ui/video.test.js   # video preview fits and plays
```

## License

MIT. Inspired by TechSmith's classic Jing.
