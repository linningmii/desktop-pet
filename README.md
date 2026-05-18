# Desktop Pet

A cross-platform desktop pet. The current primary implementation is a Tauri v2 native app, with the earlier Electron prototype kept in the repository as a fallback/reference.

## Current Status

- Windows is the main tested platform.
- macOS structure is in place through Tauri, but still needs real-device verification.
- The pet lives in the tray/menu bar and can only be exited from there.
- It walks around the desktop, avoids the cursor, supports idle/walk sprite frames, and exposes size/speed/activity controls from the tray menu.

## Native Tauri App

Install dependencies:

```powershell
npm install
```

Run the native app in development:

```powershell
npm run native:dev
```

Build the native Windows installer:

```powershell
npm run native:build
```

The Tauri installer is generated under:

```text
src-tauri/target/release/bundle/nsis/
```

## Electron Prototype

The Electron prototype is still available:

```powershell
npm start
npm run dist:win
```

It is useful for comparing behavior, but the Tauri version is the intended path because it avoids bundling Chromium and produces a much smaller installer.

## Assets

Runtime sprites are kept in:

```text
assets/pet/
native-ui/assets/pet/
```

The raw motion source frames are intentionally ignored from git. Commit only the processed runtime frames unless the raw sources are explicitly needed.

## Notes

Do not commit local build outputs, installers, signing certificates, or `.env` files. The `.gitignore` excludes the usual generated artifacts and private key/certificate formats.
