<p align="center"><img src="assets/branding/lumi-256.png" width="112" alt="Lumi icon"></p>

# Lumi

Lumi is a Windows desktop notch with media controls, tasks, focus timers, water reminders, a shelf for files and text, and an animated mascot named Mimo.

## Install

1. Download and run the Windows x64 installer from [GitHub Releases](https://github.com/dbustosrc/Lumi/releases).
2. Open Lumi from the Start menu.
3. Open Settings from the system tray or the expanded notch.

Lumi requires Windows 10 or 11 on x64 with hardware graphics support. If Windows reports a missing Visual C++ runtime, install the [Microsoft x64 redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist). If you enable Start with Windows, keep the executable at its registered location.

## Features

- **Music.** Control Windows media sessions or Music Assistant, view synced lyrics from LRCLIB, and adjust Windows output volume. Music Assistant takes priority while playing.
- **Tasks.** Schedule and repeat tasks. Select a marked date in the month or week view to jump to its first task. Optional voice input requires a Wyoming Whisper server.
- **Focus and water.** Run focus and break timers, set hydration goals and reminders, and view your drinking history.
- **Shelf.** Store file references and text cards. Open files, copy text, or drag items out.
- **Mimo.** The mascot reacts to the cursor, Bluetooth headphones, and Windows notification banners in the compact notch.
- **Cursor activity.** Enable an activity event every 30 seconds from Settings or the tray. The cursor stays in place, and the feature is off by default.
- **Desktop settings.** Choose hover or click to expand, select a monitor and scale, and configure full-screen hiding and system indicators. The interface is available in English and Spanish.

## Local data

Lumi stores preferences, tasks, water history, Shelf items and logs in `%APPDATA%\Lumi`. Configure optional Music Assistant and Wyoming connections in Settings.

## Build from source

Install Rust 1.92 or later with the `x86_64-pc-windows-msvc` toolchain, Visual Studio C++ Build Tools, a Windows SDK with `rc.exe`, and PowerShell 7.

```powershell
git clone https://github.com/dbustosrc/Lumi.git
cd Lumi
pwsh -NoProfile -File ./build.ps1
```

The script creates `target/release/lumi.exe`. It may restart a running instance from this checkout and clears generated build files and Lumi logs. Use `-WhatIf` to preview these actions.

To build the installer in `dist`, install Inno Setup 7 and run `pwsh -NoProfile -File ./build.ps1 -Installer`.

## License

Lumi was created by David R. Bustos.

Lumi is licensed under [GPL-2.0-only](LICENSE). See [third-party notices](third-party-licenses/README.md) for dependency licenses.
