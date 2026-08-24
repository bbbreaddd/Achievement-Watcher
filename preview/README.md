# Achievement Watcher Preview

This is the side-by-side Tauri/Rust preview. It uses its own SQLite database and only reads legacy Achievement Watcher data during import. OBS remains an optional integration: the app can connect to its WebSocket server and save the replay buffer, but OBS is never required or launched by Achievement Watcher.

## Resource model

The always-on path is the Rust process, filesystem watcher, SQLite connection, and tray icon. Dashboard and notification WebViews are created on demand and destroyed when closed. The engineering budget is under 100 MB total working set in background mode and under 1% idle CPU; the expected background range is 20–50 MB on Windows 10/11. An open dashboard includes WebView2 processes and is expected to use roughly 90–180 MB.

These are targets until measured on packaged Windows builds. To measure a closed-dashboard build, run:

```powershell
./scripts/measure-background.ps1
```

## Development

Requirements: current stable Rust, Node.js 22, npm, and the Tauri 2 Windows prerequisites.

```powershell
npm ci
cargo test -p aw-core
npm run build
npm run tauri build
```

The first scan establishes a quiet baseline so existing achievements do not generate a notification storm. Supported preview inputs include common Steam-emulator JSON/INI formats, GreenLuma-configured directories, SSE binary files, and RPCS3 `TROPUSR.DAT` files. Notifications are persisted before delivery and retry through a native Windows fallback if the styled renderer does not acknowledge startup.

### Fast Windows iteration

When the repository is opened through a mapped Linux/Samba drive, the Windows helper stages the buildable source under `%LOCALAPPDATA%\AchievementWatcherBuild\source` and keeps Cargo artifacts on the same local disk. During development it synchronizes changes from the repository every 750 ms, so Vite hot reload still follows edits made on the mapped drive. This avoids Vite stalls, thousands of compiler reads and writes crossing the share, and Linux/Windows artifacts sharing one `target` directory.

```powershell
# Tauri development mode with frontend hot reload and incremental Rust builds
./scripts/dev-windows.ps1

# Type checks, unit tests, and cargo check without creating an installer
./scripts/dev-windows.ps1 -CheckOnly

# Optimized NSIS build; output path is printed when complete
./scripts/dev-windows.ps1 -Release
```

The main application installer is written to `%LOCALAPPDATA%\AchievementWatcherBuild\target\release\bundle\nsis`. CI publishes that NSIS installer and the separately sideloaded Game Bar package together in the `achievement-watcher-preview-windows` workflow artifact. Other generated files remain under `%LOCALAPPDATA%\AchievementWatcherBuild`. If `sccache` is installed, the script uses it automatically. To discard only this generated cache, run `./scripts/clean-windows-cache.ps1`.

For frontend-only work, use `npm run dev`. For repeated frontend tests, use `npm run test:watch`. Reserve the optimized Tauri/NSIS build for installable checkpoints because release LTO and stripping trade build time for a smaller executable.

## Optional Xbox Game Bar companion

The desktop application remains an ordinary Win32/NSIS install. For fullscreen notifications, `game-bar-companion/` builds a completely separate, sideloaded UWP widget. Enable **Prefer optional Xbox Game Bar companion** in settings, build the companion on Windows with `./game-bar-companion/build-sideload.ps1`, then install the generated package and public certificate with:

```powershell
./game-bar-companion/install-sideload.ps1 -PackagePath ./path/to/companion.appx -CertificatePath ./game-bar-companion/AchievementWatcher.GameBar.cer
```

Open Game Bar with Win+G, select Achievement Watcher, and paste the 64-character pairing token shown by the desktop application. Communication uses a per-user-session named pipe plus the random pairing token; no TCP port is opened. If the widget is disconnected or rejects delivery, the durable queue immediately continues through the selected desktop/native fallback. Use `uninstall-sideload.ps1 -CertificatePath ./game-bar-companion/AchievementWatcher.GameBar.cer` to remove the companion and its exact trusted certificate.
