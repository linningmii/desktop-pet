# Desktop Pet

[![Current release](https://img.shields.io/github/v/release/linningmii/desktop-pet?include_prereleases&label=current%20version)](https://github.com/linningmii/desktop-pet/releases)
[![Build and Release](https://github.com/linningmii/desktop-pet/actions/workflows/release.yml/badge.svg)](https://github.com/linningmii/desktop-pet/actions/workflows/release.yml)
[![Latest release date](https://img.shields.io/github/release-date-pre/linningmii/desktop-pet?label=latest%20release)](https://github.com/linningmii/desktop-pet/releases)
[![Downloads](https://img.shields.io/github/downloads/linningmii/desktop-pet/total?label=downloads)](https://github.com/linningmii/desktop-pet/releases)
[![Repo size](https://img.shields.io/github/repo-size/linningmii/desktop-pet)](https://github.com/linningmii/desktop-pet)

A cross-platform desktop pet built with Tauri v2.

## Status

- Windows is the main tested platform.
- macOS support is planned through Tauri, but still needs real-device verification.
- The pet lives in the system tray/menu bar and can only be exited from there.
- It walks around the desktop, avoids the cursor, supports idle/walk sprite frames, and exposes size/speed/activity controls from the tray menu.

This is an alpha because the core behavior is usable, but cross-platform validation, update/migration behavior, and release automation are still in progress.

## Project Layout

```text
app/          Frontend view and runtime sprite assets
src-tauri/    Rust/Tauri application shell and movement logic
```

The old Electron prototype has been removed from the tracked source.

## Development

Prerequisites:

- Node.js 24 or newer.
- Rust and Cargo from [rustup](https://rustup.rs/).
- Windows: Microsoft C++ Build Tools and WebView2 Runtime. See the [Tauri Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows).
- macOS: Xcode Command Line Tools. macOS packaging still needs real-device verification.

Install the Node-side development tools:

```powershell
npm install
```

Verify Cargo is available:

```powershell
cargo --version
```

Run locally:

```powershell
npm run dev
```

Check the code:

```powershell
npm run check
```

Build the Windows installer:

```powershell
npm run build
```

The installer is generated under:

```text
src-tauri/target/release/bundle/nsis/
```

## Release

GitHub Actions builds and publishes releases from version tags.

Create and push a tag that matches the app version:

```powershell
git tag v<version>
git push origin v<version>
```

The workflow will:

- run checks on Windows,
- build the Tauri NSIS installer,
- create a GitHub Release for the tag,
- mark prerelease tags such as `v0.2.0-alpha.1` as prereleases,
- upload the Windows installer as a release asset.

## Assets

Runtime sprites are kept in:

```text
app/assets/pet/
```

Raw source frames and local build outputs are intentionally ignored from git. Commit only the processed runtime frames unless the raw sources are explicitly needed.

## Notes

Do not commit local build outputs, installers, signing certificates, or `.env` files. The `.gitignore` excludes generated artifacts and private key/certificate formats.
