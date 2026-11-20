<p align="center"><img src="assets/branding/lumi-256.png" width="112" alt="Lumi icon"></p>

# Lumi

Lumi is a Windows desktop notch with media controls, tasks, focus timers, water reminders, a shelf for files and text, and an animated mascot named Mimo.

## Install

1. Download and run the Windows x64 installer from [GitHub Releases](https://github.com/dbustosrc/Lumi/releases).
2. Open Lumi from the Start menu.
3. Open Settings from the system tray or the expanded notch.

Lumi requires Windows 10 or 11 on x64 with hardware graphics support. If Windows reports a missing Visual C++ runtime, install the [Microsoft x64 redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist). If you enable Start with Windows, keep the executable at its registered location.

## Features

- **Music.** Control Windows media sessions or Music Assistant, view synced lyrics from LRCLIB, and adjust Windows output volume. When Music Assistant is connected, use the search button beside Lyrics to find and play songs.
- **Tasks.** Schedule and repeat tasks. Select a marked date in the month or week view to jump to its first task. Optional voice input requires a Wyoming Whisper server.
- **Focus and water.** Run focus and break timers, set hydration goals and reminders, and view your drinking history.
- **Shelf.** Drag in or paste file references and text with Ctrl+V. Open files, copy text, or drag items out.
- **Quick Share.** Send and receive files, text, and links with nearby Android devices from the notch.
- **Mimo.** The mascot reacts to the cursor, Bluetooth headphones, and Windows notification banners in the compact notch.
- **Cursor activity.** Enable an activity event every 30 seconds from Settings or the tray. The cursor stays in place, and the feature is off by default.
- **Desktop settings.** Drag to reorder notch pages or switch off pages you do not use. Choose hover or click to expand, select a monitor on the display map, and configure scale, motion blur, full-screen hiding, and system indicators. The interface is available in English and Spanish.

## Local data

Lumi stores preferences, tasks, water history, Shelf items and logs in `%APPDATA%\Lumi`. A local build and an installed copy use the same data when run under the same Windows account. Installing an update preserves these files. Configure optional Music Assistant and Wyoming connections in Settings.

## Quick Share

Open **Settings → Quick Share** to set the PC name and make it visible to nearby devices. Visibility is off by default. Drag files or selected text over the notch, then drop them on **Shelf** to store them or on **Quick Share** to stage a transfer. You can also click Shelf or Quick Share's sending tray and press **Ctrl+V**. In Quick Share, open **Devices** to choose a recipient. When you receive a request, compare the code on both devices before accepting. Drag received files out of the right tray or click received text to copy it. Lumi stores received files in `%APPDATA%\Lumi\quick-share\received`. Clearing either tray removes its entries and keeps the files on disk.

Quick Share uses the [rquickshare native transport](https://github.com/asulwer/rquickshare/tree/25433197491472e7510c4ffe45ba0394834f49ea/core_lib). The Quick Share name and icon identify Google’s sharing feature.

## Task API

With Lumi running, open **Settings → To-do → Task API**, generate an access token, and copy it to your client. Tokens are valid until revoked. Lumi stores only their hashes and lets you revoke each token separately.

The API is available while To-do is enabled in **Settings → Pages**.

Send requests to `http://127.0.0.1:17642` from an application on the same PC. Every request needs `Authorization: Bearer <token>`. Send JSON with `Content-Type: application/json` for creates and updates.

| Method | Path | Success response |
| --- | --- | --- |
| GET | `/v1/tasks` | 200 and an array of tasks |
| GET | `/v1/tasks/{id}` | 200 and the task |
| POST | `/v1/tasks` | 201, the created task, and a `Location` header |
| PATCH | `/v1/tasks/{id}` | 200 and the updated task |
| DELETE | `/v1/tasks/{id}` | 204 with an empty body |

To create a task, send a JSON body such as:

```json
{
  "title": "Review notes",
  "schedule": { "date": "2026-11-04", "time": "14:30:00", "daily": false },
  "color": "blue"
}
```

Only `title` is required on creation. A task defaults to today, no time, no recurrence, and green. Colors are `green`, `blue`, `orange`, `red`, and `purple`. Schedules use local dates and times. Set `schedule.time` to `null` for a task without a time.

A patch can change `title`, `schedule`, `color`, or `is_done`. Omit fields to keep their values. When changing the schedule, send the whole schedule object. For example, `{"is_done":true}` marks the task complete. Task responses include the UUID `id`, schedule, color, completion status, and UTC creation and completion timestamps.

Errors have an `{"error":"code"}` body. An invalid or revoked token returns 401, an unknown task returns 404, malformed JSON returns 400, and a blank title returns 422. API changes appear in To-do and are saved to the same `tasks.json` as changes made in Lumi.

## Build from source

Install Rust 1.92 or later with the `x86_64-pc-windows-msvc` toolchain, Visual Studio C++ Build Tools, a Windows SDK with `rc.exe`, and PowerShell 7. Install the [Protocol Buffers compiler](https://github.com/protocolbuffers/protobuf/releases) and add `protoc.exe` to `PATH` or set `PROTOC` to its full path.

```powershell
git clone https://github.com/dbustosrc/Lumi.git
cd Lumi
pwsh -NoProfile -File ./build.ps1
```

The script creates `target/release/lumi.exe` and removes older build artifacts after a successful build. It may restart a running instance from this checkout. It clears temporary build files even if the build fails and clears Lumi logs after success. Use `-WhatIf` to preview these actions.

To build the installer, install Inno Setup 7 and run `pwsh -NoProfile -File ./build.ps1 -Installer`. After the new installer is ready, the script removes the previous distributions from `dist` and prints the new installer's path. A build without `-Installer` keeps the last installer. Run the new installer to update the Start menu copy. Close any local build before opening the installed copy.

## License

Lumi was created by David R. Bustos.

Lumi is licensed under [GPL-3.0-only](LICENSE). In Lumi, open **Settings → About → Third-party licenses** to read dependency licenses offline. The source repository includes the same [third-party notices](third-party-licenses/README.md).
