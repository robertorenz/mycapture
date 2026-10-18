# myCapture

A Jing-style screen capture tool written in Rust ([Tauri 2](https://tauri.app)), for Windows, macOS and Linux.

A shiny little moon peeks out from the edge of your screen, half showing. Hover it and three buttons pop out: **Capture**, **History** and **More**.

![moon](app/icons/128x128.png)

## Features

- **Screenshots**: freeze the screen, drag an area or click a window to grab it, then fine-tune with resize handles and a magnifier.
- **Annotate**: Snagit-style arrows (classic, double, curved, block, tapered, dashed, pointer, line) and callouts (filled bubble, outlined bubble, square label, speech balloon), plus boxes, ellipses, highlighter, pen, text, numbered steps and pixelate, with undo and redo. If an arrow or callout goes past the edge of the screenshot, the canvas grows to fit it.
- **Screen video**: record any area to MP4 (H.264 at CRF 23 + AAC 128 kbps, so roughly 5–15 MB per minute of desktop work) with your microphone. Recordings are written as crash-safe MKV segments and joined into the MP4 when you finish. You can pause, resume, finish or discard, and there is a countdown before it starts.
- **Webcam bubble**: puts your camera in a corner of the recorded area. It can be a circle or a rounded square, you can resize it, and you can drag it anywhere. It is recorded along with the screen.
- **Paste anywhere**: screenshots go to the clipboard as images, and videos and images can be copied as files. Paste them straight into WhatsApp, Teams, Slack, email and so on.
- **Share links**: upload to the bundled self-hosted server, to S3-compatible storage (Cloudflare R2, AWS S3, Backblaze B2, MinIO) or to any multipart upload endpoint. The link is copied to your clipboard.
- **History** of every capture, with copy, share and delete.
- Global hotkey (default `Ctrl/⌘+Shift+J`), a tray icon, and a dockable moon you can drag to the top, bottom, left or right edge of any monitor.

## Requirements

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

GitHub Actions builds the app and the server on all three platforms on every push.

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

| Where | Action |
| --- | --- |
| Moon | Hover to open the menu, click to capture, drag to dock it on another edge |
| Capture screen | Drag to select an area, click a window to select it, **Enter** for an image, **V** for a video, **Esc** to cancel |
| Recording bar | Pause/resume, camera, **Finish**, discard; the hotkey also finishes the recording |
| Editor | `A C R E H P T N B` switch tools (C = callout; drag from the spot to point at, to where the bubble goes), `Ctrl+Z`/`Ctrl+Y` undo/redo, `Ctrl+C` copy, `Ctrl+S` save as |

Captures are stored in:

- Windows: `%APPDATA%\myCapture\captures`
- macOS: `~/Library/Application Support/myCapture/captures`
- Linux: `~/.local/share/myCapture/captures`

## Platform notes

- **Windows**: fully tested. The recording frame and control bar are hidden from the recording.
- **macOS**: the first time you use myCapture, grant it *Screen Recording*, *Camera* and *Microphone* access in System Settings → Privacy & Security. Video is recorded with avfoundation.
- **Linux**: needs an X11 session (ffmpeg `x11grab`), and audio comes from PulseAudio/PipeWire `default`. Wayland sessions can take screenshots, but cannot record video yet.

## Layout

```
app/      Tauri app (Rust): capture, recording, clipboard, upload, windows
ui/       Plain HTML/CSS/JS for the moon, capture overlay, editor, history, settings, camera
server/   mycapture-server (axum)
```

## License

MIT. Inspired by TechSmith's classic Jing.
