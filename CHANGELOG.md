# Changelog

## 1.6.0

- Search for songs from the Music page when Music Assistant is connected. Select a result to play it on the configured player or on Lumi.
- Send and receive text and links through Quick Share. Drag files or selected text onto the notch, or paste them into Shelf or Quick Share with Ctrl+V.
- Keep the selected tray open after a drop. Shelf, Quick Share, and the task list share a dark background, and both Quick Share trays use Shelf's dashed outline.
- Reduce memory use by deferring hardware graphics setup until the notch first expands, bounding the Sendspin audio buffer, and resizing Windows media artwork before rendering.

## 1.5.0

- Added a Pages setting to reorder notch pages by dragging, or disable pages and their background features.
- Added optional motion blur when opening the notch or switching pages, and smoother settings switches.
- Display now maps each monitor by its Windows position and resolution, with a notch preview on the selected monitor. Status ring options are now in Display, and calendar options are in To-do.
- Expanded World Clock from 12 to 65 cities.
- Reduced background polling and loaded Shelf image previews on demand.
- Prevented multiple instances of Lumi from running in the same Windows session.

## 1.4.0

- Added Quick Share for sending and receiving files with nearby Android devices. Drop files on the notch to choose Shelf or Quick Share. Review incoming requests and confirm the code before accepting.
- Added an animated device radar and separate trays for outgoing and received files. Clearing a tray or removing an item keeps the file on disk.
- Added Classic and Floating notch styles, an optional glass effect, and battery, water, or task progress options for the status ring.
- Added a Widgets page with two configurable slots for calendar, water, focus, world clock, battery, moon, or countdown widgets.
- Fixed Music Assistant playback switching devices when a remote queue changes or resumes.

## 1.3.0

- Added Mimo animations to quick panels for Bluetooth headphones, charging, and reminders.
- Kept the notch compact while Mimo reacts to file drops and voice capture when the full panel is closed.
- Improved Mimo's eye animations and how reactions pause and resume across notch transitions.

## 1.2.0

- Reworked Mimo's compact-notch animations for waking up, stalking the cursor, and reacting to Bluetooth headphones and notification banners.
- Added distinct Mimo reactions for focus timers, hydration, tasks, the Shelf, charging, and voice capture.

## 1.1.0

- Added an authenticated local API to list, create, edit, and delete tasks. Access tokens are managed in To-do settings.
- Added an offline index of third-party components and their license texts in About.
- Changed Lumi's license to GPL-3.0-only.

## 1.0.0 · 2026-11-03

First public release of Lumi for Windows x64.

- Music Assistant and Windows media controls with synced lyrics and output volume.
- Scheduled and recurring tasks, calendar navigation, and optional Wyoming Whisper voice input.
- Focus and break timers, hydration reminders, and a shelf for files and text.
- Mimo reacts to the cursor, Bluetooth headphones, and Windows notification banners.
- Monitor and scale selection, full-screen hiding, and English and Spanish interfaces.
- Optional mouse activity every 30 seconds without visible cursor movement.
