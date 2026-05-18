# Desktop Pet

A cross-platform desktop pet built with Tauri v2.

## Status

Current version: `0.2.0-alpha.1`

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

Install dependencies:

```powershell
npm install
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

## Assets

Runtime sprites are kept in:

```text
app/assets/pet/
```

Raw source frames and local build outputs are intentionally ignored from git. Commit only the processed runtime frames unless the raw sources are explicitly needed.

## Notes

Do not commit local build outputs, installers, signing certificates, or `.env` files. The `.gitignore` excludes generated artifacts and private key/certificate formats.
