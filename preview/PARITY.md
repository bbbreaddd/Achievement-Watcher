# Achievement Watcher parity tracker

The Electron application under `app/` and `watchdog/` is the behavioral and visual specification.
An item is complete only when it is functional in an installed Windows build; visible but inert controls do not count.

## Application shell and library

- [ ] Original 30 px frameless title bar, window controls, watcher state and tray lifecycle
- [ ] Profile avatar/name and aggregate achievement/game/completion statistics
- [ ] PlayStation trophy totals
- [ ] Original landscape and portrait game grids
- [ ] Alphabetical, completion and most-recent sorting
- [ ] Search, zero-percent hiding and empty/loading states
- [ ] Source emblem on every game without confusing metadata with progress origin
- [ ] Game context menu: launch, executable configuration, refresh, blacklist and cache actions
- [ ] Per-game duplicate source selection and configurable merging

## Achievement view

- [ ] Original game header, source, completion counter, playtime and last-played data
- [ ] Separate collapsible unlocked and locked lists
- [ ] Unlock-time, progress and rarity sorting
- [ ] Hidden-achievement behavior and reveal control
- [ ] Progress bars, global rarity, trophy grades and source-correct artwork
- [ ] Search, highlighted notification target and scroll-to-top behavior

## Settings

- [ ] Original General, Notification, Souvenir, Folder, Source, Advanced and Debug categories
- [ ] Language, username, hidden/zero filters, duplicate merge, timestamp merge and portrait mode
- [ ] Notification enablement, progress notifications, descriptions, rumble, playtime and sounds
- [ ] Popup preset, placement, duration, transport and fullscreen routing controls
- [ ] Screenshot folder/overwrite and optional OBS replay settings
- [ ] Default/custom source folders, per-folder notification toggle and Smart Find
- [ ] Official Steam installed/owned modes and account selector
- [ ] Steam emulator, GreenLuma, RPCS3, LumaPlay and Watchdog-cache source toggles
- [ ] Encrypted optional Steam API key, blacklist controls and action hooks
- [ ] Notification, progress, playtime, Game Bar, OBS and source diagnostics

## Sources and background services

- [x] Common Steam emulator files
- [x] Official Steam authenticated read helper, local cache trigger and public/API fallback
- [x] RPCS3 trophy parsing
- [ ] Full original emulator path/file compatibility matrix
- [ ] GreenLuma registry variants
- [ ] Nemirtingas Epic and GOG sources with ID mapping
- [ ] LumaPlay/Uplay registry and cache sources
- [ ] Watchdog notification-cache import
- [ ] Duplicate merge rules and blacklist
- [ ] Playtime tracking and game launch configuration

## Unlock actions and delivery

- [x] Durable unlock/progress transition queue
- [x] Custom desktop popup with Windows fallback
- [x] Optional Xbox Game Bar companion transport
- [x] Screenshot capture
- [ ] Original notification preset gallery, animation and sound selection
- [ ] Optional OBS replay-buffer save; OBS absence must never block other delivery
- [ ] Controller rumble, GNTP, WebSocket and custom executable actions
- [ ] Correct fullscreen routing, retry diagnostics and notification deep links

## Release quality

- [ ] Legacy settings/cache/blacklist/custom-folder migration
- [ ] Localization and Steam-language metadata
- [ ] Installer contains the Steam helper/runtime and optional companion instructions
- [ ] Startup/background lifecycle and update behavior
- [ ] Memory below 100 MB at idle where WebView2 permits, with measured CPU and wakeups
- [ ] End-to-end Windows tests for every source and notification transport
