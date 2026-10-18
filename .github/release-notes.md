## myCapture

A Jing-style screen capture tool: a shiny moon peeking from your screen edge, screenshots with annotations, screen video with your webcam in the corner, and one-click copy or share links you can paste into WhatsApp, Teams, Slack or email.

### Downloads

| Platform | File |
| --- | --- |
| Windows | `myCapture_*_x64-setup.exe` (installer) or `myCapture_*_x64_en-US.msi` |
| macOS (Apple Silicon) | `myCapture_*_aarch64.dmg` |
| macOS (Intel) | `myCapture_*_x64.dmg` |
| Linux | `.AppImage`, `.deb` or `.rpm` |
| Sharing server | `mycapture-server-<platform>`, see the README |

### Before you start

- **Video recording needs ffmpeg.** Windows: `winget install Gyan.FFmpeg` · macOS: `brew install ffmpeg` · Linux: `sudo apt install ffmpeg`.
- These builds are **not code-signed** yet:
  - **Windows** shows a SmartScreen warning: choose *More info → Run anyway*.
  - **macOS** blocks the app the first time: right-click it and choose *Open*. If macOS says the app is damaged, run `xattr -cr /Applications/myCapture.app`. Then allow *Screen Recording*, *Camera* and *Microphone* in System Settings → Privacy & Security.
- **Linux**: video recording needs an X11 session.
