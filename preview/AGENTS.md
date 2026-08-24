# Achievement Watcher Preview

This directory is the rewritten Tauri application. Do not modify the legacy application while working here. Files under `../app/` are read-only compatibility assets unless a task explicitly requires changing them.

## Boundaries

- `src/` owns presentation and short-lived UI state. Keep platform, filesystem, and process work behind Tauri commands.
- `src-tauri/` owns desktop integration and orchestration. Put focused services in named modules; keep `lib.rs` responsible for composition and commands.
- `crates/aw-core/` owns persisted domain data, parsing, source discovery, and merge behavior. It must remain independent of Tauri.
- Local achievement data is authoritative. Network requests may enrich missing names, rarity, and artwork but must not make the library unusable.
- OBS, Xbox Game Bar, screenshots, overlays, and event bridges are optional integrations.

## Code quality

- Prefer complete, readable flows over many tiny helpers. Extract a boundary only when it isolates real state, I/O, or reusable behavior.
- Comments explain non-obvious constraints and failure modes. Do not narrate routine code or include development-conversation context.
- Preserve stored settings and imported data. New defaults apply only when a field has never been saved.
- Do not hide partial failures. If recovery or rollback fails, report both the original failure and the recovery failure.
- Keep the original-inspired UI quiet and functional: no gradients, marketing copy, ornamental cards, or unnecessary animation.

## Validation

Run focused tests while editing, then use these before a checkpoint:

```bash
npm run check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

On Windows or a mapped drive, use `./scripts/dev-windows.ps1 -CheckOnly`. Use `./scripts/dev-windows.ps1` for runtime testing and `-Release` only for installer checkpoints. The script stages source and build artifacts under `%LOCALAPPDATA%\AchievementWatcherBuild`; do not commit staged binaries or caches.
