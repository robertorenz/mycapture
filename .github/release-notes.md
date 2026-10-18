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

### Install

- **Windows**: run `myCapture_*_x64-setup.exe`. It installs for your user only, so no admin rights are needed, and **ffmpeg is included**. When setup finishes it can start myCapture, and the moon appears on the top edge of your screen. The `.msi` is for managed or IT deployments.
- **macOS**: open the `.dmg` and drag myCapture to Applications. Install ffmpeg for video recording with `brew install ffmpeg`.
- **Linux**: the `.deb` installs ffmpeg automatically. For the AppImage or `.rpm`, install `ffmpeg` with your package manager.

### Before you start
- These builds are **not code-signed** yet:
  - **Windows** shows a SmartScreen warning: choose *More info → Run anyway*.
  - **macOS** blocks the app the first time: right-click it and choose *Open*. If macOS says the app is damaged, run `xattr -cr /Applications/myCapture.app`. Then allow *Screen Recording*, *Camera* and *Microphone* in System Settings → Privacy & Security.
- **Linux**: video recording needs an X11 session.
