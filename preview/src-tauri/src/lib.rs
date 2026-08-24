mod game_bar;
mod gntp;
mod obs;
mod process;
mod registry;
mod rumble;
mod steam;
mod websocket;

use aw_core::{
    AppSettings, DeliveryReceipt, GameSummary, MigrationReport, NotificationEvent,
    NotificationMode, Store, migration, parser, source,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::Utc;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::Duration,
};
use tauri::{
    AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::NotificationExt;

struct AppState {
    store: Mutex<Store>,
    watcher: Mutex<Option<RecommendedWatcher>>,
    awaiting_overlay: Mutex<HashSet<i64>>,
    current_overlay: Mutex<Option<NotificationEvent>>,
    launched_games: Mutex<HashSet<String>>,
    game_bar: game_bar::GameBarBridge,
    websocket: websocket::Bridge,
    data_dir: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    version: String,
    release_url: String,
    installer_name: String,
}

#[derive(Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    draft: bool,
    assets: Vec<GithubReleaseAsset>,
}

struct DownloadableUpdate {
    info: UpdateInfo,
    installer_url: String,
    checksum_url: String,
}

type CommandResult<T> = Result<T, String>;

#[tauri::command]
fn load_settings(state: State<'_, AppState>) -> CommandResult<AppSettings> {
    state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)
}

#[tauri::command]
fn read_profile_avatar(path: PathBuf) -> CommandResult<String> {
    let metadata = std::fs::metadata(&path).map_err(error)?;
    if !metadata.is_file() || metadata.len() > 5 * 1024 * 1024 {
        return Err("Avatar must be an image smaller than 5 MB".into());
    }
    let bytes = std::fs::read(&path).map_err(error)?;
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        return Err("Avatar must be a PNG, JPEG, or WebP image".into());
    };
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

#[tauri::command]
fn read_notification_audio(path: PathBuf) -> CommandResult<String> {
    let metadata = std::fs::metadata(&path).map_err(error)?;
    if !metadata.is_file() || metadata.len() > 12 * 1024 * 1024 {
        return Err("Notification audio must be a file smaller than 12 MB".into());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mime = match extension.as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" | "aac" => "audio/mp4",
        _ => return Err("Supported audio formats are WAV, MP3, OGG, FLAC, M4A, and AAC".into()),
    };
    let bytes = std::fs::read(path).map_err(error)?;
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

#[tauri::command]
fn import_steam_avatar(app: AppHandle, steam_id: String) -> CommandResult<PathBuf> {
    use std::io::Read;

    if steam_id.len() != 17 || !steam_id.chars().all(|character| character.is_ascii_digit()) {
        return Err("Steam account ID is invalid".into());
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .user_agent("Achievement-Watcher/0.1")
        .build();
    let profile = agent
        .get(&format!(
            "https://steamcommunity.com/profiles/{steam_id}?xml=1"
        ))
        .call()
        .map_err(error)?
        .into_string()
        .map_err(error)?;
    let avatar_url = profile
        .split("<avatarFull><![CDATA[")
        .nth(1)
        .and_then(|value| value.split("]]></avatarFull>").next())
        .filter(|value| value.starts_with("https://"))
        .ok_or_else(|| "Steam profile did not provide an avatar".to_string())?;
    let mut bytes = Vec::new();
    agent
        .get(avatar_url)
        .call()
        .map_err(error)?
        .into_reader()
        .take(5 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() > 5 * 1024 * 1024
        || !(bytes.starts_with(b"\x89PNG\r\n\x1a\n")
            || bytes.starts_with(&[0xff, 0xd8, 0xff])
            || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP")))
    {
        return Err("Steam returned an unsupported avatar image".into());
    }
    let extension = if bytes.starts_with(b"\x89PNG") {
        "png"
    } else if bytes.starts_with(b"RIFF") {
        "webp"
    } else {
        "jpg"
    };
    let directory = app.path().app_data_dir().map_err(error)?;
    std::fs::create_dir_all(&directory).map_err(error)?;
    let path = directory.join(format!("profile-avatar.{extension}"));
    std::fs::write(&path, bytes).map_err(error)?;
    Ok(path)
}

#[tauri::command]
fn open_windows_settings(page: String) -> CommandResult<()> {
    let uri = match page.as_str() {
        "notifications" => "ms-settings:notifications",
        "focus_assist" => "ms-settings:quiethours",
        _ => return Err("Unsupported Windows settings page".into()),
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("explorer.exe")
            .arg(uri)
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(error)?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = uri;
        Err("Windows settings are available only on Windows".into())
    }
}

fn latest_preview_update() -> CommandResult<Option<DownloadableUpdate>> {
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION")).map_err(error)?;
    let releases = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .user_agent("Achievement-Watcher/0.1")
        .build()
        .get("https://api.github.com/repos/darktakayanagi/Achievement-Watcher/releases?per_page=20")
        .call()
        .map_err(error)?
        .into_json::<Vec<GithubRelease>>()
        .map_err(error)?;
    for release in releases {
        if release.draft {
            continue;
        }
        let Some(version_text) = release.tag_name.strip_prefix("preview-v") else {
            continue;
        };
        let Ok(version) = semver::Version::parse(version_text) else {
            continue;
        };
        if version <= current {
            continue;
        }
        let Some(installer) = release.assets.iter().find(|asset| {
            asset.name.to_ascii_lowercase().ends_with("-setup.exe")
                || asset.name.to_ascii_lowercase().ends_with(".setup.exe")
        }) else {
            continue;
        };
        let checksum_name = format!("{}.sha256", installer.name);
        let Some(checksum) = release
            .assets
            .iter()
            .find(|asset| asset.name == checksum_name)
        else {
            continue;
        };
        return Ok(Some(DownloadableUpdate {
            info: UpdateInfo {
                version: version.to_string(),
                release_url: release.html_url,
                installer_name: installer.name.clone(),
            },
            installer_url: installer.browser_download_url.clone(),
            checksum_url: checksum.browser_download_url.clone(),
        }));
    }
    Ok(None)
}

#[tauri::command]
fn check_for_updates(
    state: State<'_, AppState>,
    manual: Option<bool>,
) -> CommandResult<Option<UpdateInfo>> {
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    if !manual.unwrap_or(false) && !settings.check_for_updates {
        return Ok(None);
    }
    let update = latest_preview_update()?;
    Ok(update.and_then(|update| {
        (manual.unwrap_or(false)
            || settings.skipped_update_version.as_deref() != Some(update.info.version.as_str()))
        .then_some(update.info)
    }))
}

#[tauri::command]
fn install_update(app: AppHandle) -> CommandResult<()> {
    use std::io::Read;
    let update = latest_preview_update()?
        .ok_or_else(|| "No newer preview release is available".to_string())?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(60))
        .user_agent("Achievement-Watcher/0.1")
        .build();
    let expected = agent
        .get(&update.checksum_url)
        .call()
        .map_err(error)?
        .into_string()
        .map_err(error)?;
    let expected = expected
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("The release checksum is invalid".into());
    }
    let mut bytes = Vec::new();
    agent
        .get(&update.installer_url)
        .call()
        .map_err(error)?
        .into_reader()
        .take(200 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() > 200 * 1024 * 1024 {
        return Err("The update installer is unexpectedly large".into());
    }
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != expected {
        return Err("The downloaded update failed checksum verification".into());
    }
    let path = std::env::temp_dir().join(&update.info.installer_name);
    std::fs::write(&path, bytes).map_err(error)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new(&path)
            .creation_flags(0x0000_0008)
            .spawn()
            .map_err(error)?;
        app.exit(0);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("Preview updates are currently distributed as Windows NSIS installers".into())
    }
}

#[tauri::command]
fn open_release_page(url: String) -> CommandResult<()> {
    if !url.starts_with("https://github.com/darktakayanagi/Achievement-Watcher/releases/") {
        return Err("Unsupported release URL".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("explorer.exe")
            .arg(url)
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(error)?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err("Opening release pages is available only on Windows".into())
    }
}

#[tauri::command]
fn open_game_website(game_id: String, website: String) -> CommandResult<()> {
    if game_id.is_empty() || !game_id.chars().all(|character| character.is_ascii_digit()) {
        return Err("Web links are available only for games with a Steam app ID".into());
    }
    let url = match website.as_str() {
        "steam" => format!("https://store.steampowered.com/app/{game_id}/"),
        "steamdb" => format!("https://steamdb.info/app/{game_id}/"),
        "pcgamingwiki" => format!("https://pcgamingwiki.com/api/appid.php?appid={game_id}"),
        _ => return Err("Unsupported game website".into()),
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("explorer.exe")
            .arg(url)
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(error)?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err("Opening game websites is available only on Windows".into())
    }
}

#[tauri::command]
fn open_project_page(project: String) -> CommandResult<()> {
    let url = match project.as_str() {
        "fork" => "https://github.com/darktakayanagi/Achievement-Watcher",
        "original" => "https://github.com/xan105/Achievement-Watcher",
        "wiki" => "https://github.com/xan105/Achievement-Watcher/wiki",
        _ => return Err("Unsupported project page".into()),
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("explorer.exe")
            .arg(url)
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(error)?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err("Opening project pages is available only on Windows".into())
    }
}

#[tauri::command]
fn export_goldberg_achievements(
    state: State<'_, AppState>,
    source_id: String,
    game_id: String,
    path: PathBuf,
) -> CommandResult<usize> {
    let mut achievements = {
        let store = state.store.lock().map_err(lock_error)?;
        let mut values: Vec<_> = store
            .observations()
            .map_err(error)?
            .into_iter()
            .filter(|item| {
                item.game_id == game_id && (source_id == "merged" || item.source_id == source_id)
            })
            .collect();
        store.enrich_observations(&mut values).map_err(error)?;
        values
    };
    achievements.sort_by(|left, right| left.achievement_id.cmp(&right.achievement_id));
    achievements.dedup_by(|left, right| {
        left.achievement_id
            .eq_ignore_ascii_case(&right.achievement_id)
    });
    if achievements.is_empty() {
        return Err("No achievements are available to export".into());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "Choose a valid export path".to_string())?;
    std::fs::create_dir_all(parent).map_err(error)?;
    let images = parent.join("images");
    std::fs::create_dir_all(&images).map_err(error)?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .user_agent("Achievement-Watcher/0.1")
        .build();
    let mut output = Vec::with_capacity(achievements.len());
    for achievement in &achievements {
        let mut item = serde_json::json!({
            "name": achievement.achievement_id,
            "displayName": achievement.display_name.as_deref().unwrap_or(&achievement.achievement_id),
            "description": achievement.description.as_deref().unwrap_or(""),
            "hidden": achievement.hidden,
        });
        if let Some(icon) = achievement.icon.as_deref() {
            let filename = format!("{}.png", sanitize(&achievement.achievement_id));
            let destination = images.join(&filename);
            let copied = if icon.starts_with("http://") || icon.starts_with("https://") {
                agent
                    .get(icon)
                    .call()
                    .ok()
                    .and_then(|response| {
                        let mut reader = response.into_reader();
                        let mut file = std::fs::File::create(&destination).ok()?;
                        std::io::copy(&mut reader, &mut file).ok()
                    })
                    .is_some()
            } else {
                std::fs::copy(icon, &destination).is_ok()
            };
            if copied {
                item["icon"] = serde_json::Value::String(format!("images/{filename}"));
                item["icongray"] = serde_json::Value::String(format!("images/{filename}"));
            }
        }
        output.push(item);
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&output).map_err(error)?).map_err(error)?;
    Ok(output.len())
}

#[tauri::command]
fn open_data_location(state: State<'_, AppState>, location: String) -> CommandResult<()> {
    let select_file = location == "notification_log";
    let target = match location.as_str() {
        "data" => state.data_dir.clone(),
        "notification_log" => state.data_dir.join("notification.log"),
        "screenshots" => state
            .store
            .lock()
            .map_err(lock_error)?
            .load_settings()
            .map_err(error)?
            .screenshot_directory
            .unwrap_or_else(|| state.data_dir.join("screenshots")),
        _ => return Err("Unsupported application data location".into()),
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("explorer.exe");
        if select_file {
            if !target.exists() {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&target)
                    .map_err(error)?;
            }
            command.arg(format!("/select,{}", target.display()));
        } else {
            std::fs::create_dir_all(&target).map_err(error)?;
            command.arg(&target);
        }
        command.creation_flags(0x0800_0000).spawn().map_err(error)?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = select_file;
        let _ = target;
        Err("Opening application data is available only on Windows".into())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Diagnostics {
    app_version: &'static str,
    observation_count: usize,
    game_count: usize,
    enabled_source_count: usize,
    missing_source_count: usize,
    pending_notifications: u32,
    failed_notifications: u32,
    recent_errors: Vec<String>,
    notification_log: PathBuf,
}

#[tauri::command]
fn diagnostics(state: State<'_, AppState>) -> CommandResult<Diagnostics> {
    let store = state.store.lock().map_err(lock_error)?;
    let settings = store.load_settings().map_err(error)?;
    let observations = store.observations().map_err(error)?;
    let game_count = observations
        .iter()
        .map(|item| item.game_id.as_str())
        .collect::<HashSet<_>>()
        .len();
    let enabled: Vec<_> = settings
        .source_locations
        .iter()
        .filter(|location| location.enabled && source_kind_enabled(&settings, location.kind))
        .collect();
    let missing_source_count = enabled
        .iter()
        .filter(|location| !location.path.exists())
        .count();
    let (pending_notifications, failed_notifications) =
        store.notification_queue_counts().map_err(error)?;
    Ok(Diagnostics {
        app_version: env!("CARGO_PKG_VERSION"),
        observation_count: observations.len(),
        game_count,
        enabled_source_count: enabled.len(),
        missing_source_count,
        pending_notifications,
        failed_notifications,
        recent_errors: store.recent_delivery_errors(5).map_err(error)?,
        notification_log: state.data_dir.join("notification.log"),
    })
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> CommandResult<()> {
    configure_overlay_shortcut(&app, &settings)?;
    if !cfg!(dev) {
        registry::configure_startup(settings.run_at_login)?;
    }
    state.websocket.configure(
        settings.websocket_enabled,
        &settings.websocket_host,
        settings.websocket_port,
    )?;
    state
        .store
        .lock()
        .map_err(lock_error)?
        .save_settings(&settings)
        .map_err(error)?;
    configure_watcher(&app, &state, &settings)?;
    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
fn import_legacy(
    state: State<'_, AppState>,
    legacy_root: Option<PathBuf>,
) -> CommandResult<MigrationReport> {
    let root = legacy_root.unwrap_or_else(default_legacy_root);
    let mut store = state.store.lock().map_err(lock_error)?;
    migration::import_legacy(&mut store, &root).map_err(error)
}

#[tauri::command]
fn list_games(state: State<'_, AppState>) -> CommandResult<Vec<GameSummary>> {
    let store = state.store.lock().map_err(lock_error)?;
    let settings = store.load_settings().map_err(error)?;
    let enabled_sources: HashSet<_> = settings
        .source_locations
        .iter()
        .filter(|source| source.enabled && source_kind_enabled(&settings, source.kind))
        .map(|source| source.id.as_str())
        .collect();
    let source_kinds: BTreeMap<_, _> = settings
        .source_locations
        .iter()
        .map(|source| (source.id.as_str(), source.kind))
        .collect();
    let observations: Vec<_> = store
        .observations()
        .map_err(error)?
        .into_iter()
        .filter(|observation| {
            !settings.blacklisted_game_ids.contains(&observation.game_id)
                && observation_source_enabled(
                    &settings,
                    &enabled_sources,
                    observation.source_id.as_str(),
                )
        })
        .collect();
    let merged_kinds: BTreeMap<String, aw_core::SourceKind> = observations
        .iter()
        .filter_map(|observation| {
            source_kinds
                .get(observation.source_id.as_str())
                .copied()
                .or_else(|| inferred_source_kind(&observation.source_id))
                .map(|kind| (observation.game_id.clone(), kind))
        })
        .fold(BTreeMap::new(), |mut kinds, (game_id, kind)| {
            let current = kinds.entry(game_id).or_insert(kind);
            if source_priority(kind) < source_priority(*current) {
                *current = kind;
            }
            kinds
        });
    let observations = if settings.merge_duplicate {
        aw_core::merge_observations(observations, settings.time_merge_recent_first)
    } else {
        observations
    };
    let mut games: BTreeMap<(String, String), GameSummary> = BTreeMap::new();
    for observation in observations {
        let entry = games
            .entry((observation.source_id.clone(), observation.game_id.clone()))
            .or_insert_with(|| GameSummary {
                source_id: observation.source_id.clone(),
                source_kind: if observation.source_id == "merged" {
                    merged_kinds.get(&observation.game_id).copied()
                } else {
                    source_kinds
                        .get(observation.source_id.as_str())
                        .copied()
                        .or_else(|| inferred_source_kind(&observation.source_id))
                },
                game_id: observation.game_id.clone(),
                name: observation.game_id.clone(),
                unlocked: 0,
                total: 0,
                platinum: 0,
                gold: 0,
                silver: 0,
                bronze: 0,
                last_unlock_time: 0,
                playtime_seconds: 0,
                last_played: 0,
                icon: observation.icon.clone(),
                tracked: true,
            });
        entry.total += 1;
        entry.unlocked += u32::from(observation.achieved);
        if observation.achieved {
            match observation.trophy_grade.as_deref() {
                Some("platinum") => entry.platinum += 1,
                Some("gold") => entry.gold += 1,
                Some("silver") => entry.silver += 1,
                Some("bronze") => entry.bronze += 1,
                _ => {}
            }
        }
        if observation.achieved {
            entry.last_unlock_time = entry.last_unlock_time.max(observation.unlock_time);
        }
        if entry.icon.is_none() {
            entry.icon = observation.icon;
        }
    }
    for game in games.values_mut() {
        if let Some((name, icon)) = store.game_metadata(&game.game_id).map_err(error)? {
            game.name = name;
            if icon.is_some() {
                game.icon = icon;
            }
        }
        if game.icon.is_none()
            && game.source_kind.is_some_and(source_uses_steam_metadata)
            && game
                .game_id
                .chars()
                .all(|character| character.is_ascii_digit())
        {
            game.icon = Some(format!(
                "https://cdn.cloudflare.steamstatic.com/steam/apps/{}/header.jpg",
                game.game_id
            ));
        }
        (game.playtime_seconds, game.last_played) =
            store.game_activity(&game.game_id).map_err(error)?;
    }
    let tracked_ids: HashSet<_> = games.keys().map(|(_, game_id)| game_id.clone()).collect();
    if settings.show_cached_games {
        for mut game in store.catalog_games().map_err(error)? {
            if !tracked_ids.contains(&game.game_id)
                && !settings.blacklisted_game_ids.contains(&game.game_id)
            {
                (game.playtime_seconds, game.last_played) =
                    store.game_activity(&game.game_id).map_err(error)?;
                games.insert((game.source_id.clone(), game.game_id.clone()), game);
            }
        }
    }
    let mut result: Vec<_> = games.into_values().collect();
    if settings.hide_zero {
        result.retain(|game| game.unlocked > 0 || !game.tracked);
    }
    result.sort_by_key(|game| game.name.to_lowercase());
    Ok(result)
}

fn source_priority(kind: aw_core::SourceKind) -> u8 {
    match kind {
        aw_core::SourceKind::Steam => 0,
        aw_core::SourceKind::SteamEmulator => 1,
        aw_core::SourceKind::GreenLuma => 2,
        aw_core::SourceKind::Rpcs3 => 3,
        aw_core::SourceKind::Epic => 4,
        aw_core::SourceKind::Gog => 5,
        aw_core::SourceKind::LumaPlay => 6,
        aw_core::SourceKind::WatchdogCache => 7,
    }
}

fn source_uses_steam_metadata(kind: aw_core::SourceKind) -> bool {
    matches!(
        kind,
        aw_core::SourceKind::Steam
            | aw_core::SourceKind::SteamEmulator
            | aw_core::SourceKind::GreenLuma
            | aw_core::SourceKind::WatchdogCache
    )
}

fn source_kind_enabled(settings: &AppSettings, kind: aw_core::SourceKind) -> bool {
    match kind {
        aw_core::SourceKind::Steam => settings.steam_enabled,
        aw_core::SourceKind::SteamEmulator => settings.steam_emulator_enabled,
        aw_core::SourceKind::GreenLuma => settings.green_luma_enabled,
        aw_core::SourceKind::Rpcs3 => settings.rpcs3_enabled,
        aw_core::SourceKind::Epic => settings.epic_enabled,
        aw_core::SourceKind::Gog => settings.gog_enabled,
        aw_core::SourceKind::LumaPlay => settings.luma_play_enabled,
        aw_core::SourceKind::WatchdogCache => settings.watchdog_cache_enabled,
    }
}

fn inferred_source_kind(source_id: &str) -> Option<aw_core::SourceKind> {
    if source_id.starts_with("registry-greenluma-") {
        Some(aw_core::SourceKind::GreenLuma)
    } else if source_id.starts_with("registry-lumaplay-") {
        Some(aw_core::SourceKind::LumaPlay)
    } else if source_id == "legacy" || source_id == "watchdog-cache" {
        Some(aw_core::SourceKind::WatchdogCache)
    } else {
        None
    }
}

fn observation_source_enabled(
    settings: &AppSettings,
    enabled_sources: &HashSet<&str>,
    source_id: &str,
) -> bool {
    enabled_sources.contains(source_id)
        || inferred_source_kind(source_id).is_some_and(|kind| source_kind_enabled(settings, kind))
}

#[tauri::command]
fn detect_sources(deep: Option<bool>) -> Vec<aw_core::SourceLocation> {
    let mut candidates: Vec<(aw_core::SourceKind, PathBuf)> = Vec::new();
    let mut add = |root: Option<std::ffi::OsString>, paths: &[&str], kind| {
        if let Some(root) = root.map(PathBuf::from) {
            for path in paths {
                candidates.push((kind, root.join(path)));
            }
        }
    };
    add(
        std::env::var_os("APPDATA"),
        &[
            "Goldberg SteamEmu Saves",
            "GSE Saves",
            "EMPRESS",
            "Steam/CODEX",
            "SmartSteamEmu",
            "CreamAPI",
        ],
        aw_core::SourceKind::SteamEmulator,
    );
    add(
        std::env::var_os("APPDATA"),
        &["NemirtingasEpicEmu"],
        aw_core::SourceKind::Epic,
    );
    add(
        std::env::var_os("APPDATA"),
        &["NemirtingasGalaxyEmu"],
        aw_core::SourceKind::Gog,
    );
    add(
        std::env::var_os("APPDATA"),
        &["rpcs3/dev_hdd0/home"],
        aw_core::SourceKind::Rpcs3,
    );
    add(
        std::env::var_os("LOCALAPPDATA"),
        &["SKIDROW", "anadius/LSX emu/achievement_watcher"],
        aw_core::SourceKind::SteamEmulator,
    );
    add(
        std::env::var_os("USERPROFILE"),
        &["Documents/SKIDROW"],
        aw_core::SourceKind::SteamEmulator,
    );
    add(
        std::env::var_os("PUBLIC"),
        &[
            "Documents/OnlineFix",
            "Documents/Steam/RUNE",
            "Documents/Steam/CODEX",
            "Documents/EMPRESS",
            "EMPRESS",
        ],
        aw_core::SourceKind::SteamEmulator,
    );
    add(
        std::env::var_os("PROGRAMDATA"),
        &["Steam"],
        aw_core::SourceKind::SteamEmulator,
    );
    add(
        std::env::var_os("ProgramFiles(x86)"),
        &["Steam/appcache/stats"],
        aw_core::SourceKind::Steam,
    );
    add(
        std::env::var_os("ProgramFiles"),
        &["Steam/appcache/stats"],
        aw_core::SourceKind::Steam,
    );
    if let Some(documents) = registry::documents_path() {
        candidates.push((
            aw_core::SourceKind::SteamEmulator,
            documents.join("SKIDROW"),
        ));
        candidates.push((
            aw_core::SourceKind::SteamEmulator,
            documents.join("SkidRow"),
        ));
    }
    for root in registry::steam_install_paths() {
        candidates.push((aw_core::SourceKind::Steam, root.join("appcache/stats")));
    }
    if deep.unwrap_or(false) {
        candidates.extend(smart_find_source_roots());
    }
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|(kind, path)| {
            seen.insert((*kind as u8, path.to_string_lossy().to_ascii_lowercase()))
        })
        .filter(|(_, path)| path.exists())
        .filter_map(|(kind, path)| {
            let location = aw_core::SourceLocation {
                id: stable_source_id(kind, &path),
                kind,
                path,
                enabled: true,
                notify: true,
            };
            (if kind == aw_core::SourceKind::Steam {
                !steam::stats_files(&location).is_empty()
                    || !steam::installed_app_ids(&location).is_empty()
            } else {
                !source::discover_files(std::slice::from_ref(&location)).is_empty()
            })
            .then_some(location)
        })
        .collect()
}

#[cfg(windows)]
fn smart_find_source_roots() -> Vec<(aw_core::SourceKind, PathBuf)> {
    use walkdir::WalkDir;
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;

    const CONFIGS: [&str; 8] = [
        "ali213.ini",
        "valve.ini",
        "hlm.ini",
        "ds.ini",
        "steam_api.ini",
        "steamconfig.ini",
        "tenoke.ini",
        "universelan.ini",
    ];
    let drives = unsafe { GetLogicalDrives() };
    let mut roots = Vec::new();
    let documents = registry::documents_path();
    for index in 0..26_u32 {
        if drives & (1 << index) == 0 {
            continue;
        }
        let root = PathBuf::from(format!("{}:\\", (b'A' + index as u8) as char));
        let entries = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| smart_find_entry_allowed(entry))
            .filter_map(Result::ok);
        for entry in entries {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let kind = if name == "rpcs3.exe" {
                Some(aw_core::SourceKind::Rpcs3)
            } else if CONFIGS.contains(&name.as_str()) {
                Some(aw_core::SourceKind::SteamEmulator)
            } else {
                None
            };
            if let (Some(kind), Some(parent)) = (kind, entry.path().parent()) {
                roots.push((kind, parent.to_path_buf()));
                if kind == aw_core::SourceKind::SteamEmulator
                    && let (Some(documents), Ok(content)) =
                        (documents.as_deref(), std::fs::read_to_string(entry.path()))
                {
                    let app_id = emulator_config_value(&content, &["appid", "app_id", "id"]);
                    let player = emulator_config_value(&content, &["playername"]);
                    let save_type = emulator_config_value(&content, &["savetype"]);
                    if ["ali213.ini", "valve.ini", "steamconfig.ini"].contains(&name.as_str())
                        && save_type.as_deref() == Some("1")
                        && let (Some(app_id), Some(player)) = (app_id.as_deref(), player.as_deref())
                    {
                        roots.push((
                            kind,
                            documents
                                .join("VALVE")
                                .join(app_id)
                                .join(player)
                                .join("Stats"),
                        ));
                    }
                    let user_data = emulator_config_value(&content, &["userdatafolder"]);
                    let user_name = emulator_config_value(&content, &["username"]);
                    if ["hlm.ini", "ds.ini", "steam_api.ini"].contains(&name.as_str())
                        && user_data
                            .as_deref()
                            .is_some_and(|value| value.eq_ignore_ascii_case("mydocs"))
                        && let (Some(app_id), Some(user_name)) =
                            (app_id.as_deref(), user_name.as_deref())
                    {
                        roots.push((
                            kind,
                            documents.join(user_name).join(app_id).join("SteamEmu"),
                        ));
                    }
                }
            }
        }
    }
    roots
}

#[cfg(any(windows, test))]
fn emulator_config_value(content: &str, keys: &[&str]) -> Option<String> {
    content.lines().find_map(|line| {
        let (key, value) = line.trim().split_once('=')?;
        keys.iter()
            .any(|candidate| key.trim().eq_ignore_ascii_case(candidate))
            .then(|| {
                value
                    .split(['#', ';'])
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .trim_matches(['"', '\''])
                    .to_string()
            })
            .filter(|value| !value.is_empty())
    })
}

#[cfg(windows)]
fn smart_find_entry_allowed(entry: &walkdir::DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    ![
        "$Recycle.Bin",
        "$RECYCLE.BIN",
        "System Volume Information",
        "Recovery",
        "MSOCache",
        "Windows",
        "WinSxS",
        "node_modules",
        ".git",
    ]
    .iter()
    .any(|ignored| name.eq_ignore_ascii_case(ignored))
}

#[cfg(not(windows))]
fn smart_find_source_roots() -> Vec<(aw_core::SourceKind, PathBuf)> {
    Vec::new()
}

fn stable_source_id(kind: aw_core::SourceKind, path: &Path) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in path.to_string_lossy().to_lowercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("auto-{}-{hash:016x}", source_priority(kind))
}

#[tauri::command]
fn steam_accounts(state: State<'_, AppState>) -> CommandResult<Vec<steam::SteamAccount>> {
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    Ok(steam::accounts(&settings.source_locations))
}

fn active_game_id(state: &State<'_, AppState>) -> Option<String> {
    steam::running_app_id().or_else(|| {
        state
            .launched_games
            .lock()
            .ok()
            .and_then(|games| games.iter().next().cloned())
    })
}

#[tauri::command]
fn current_overlay_game_id(state: State<'_, AppState>) -> CommandResult<String> {
    active_game_id(&state).ok_or_else(|| "No monitored game is currently running".into())
}

#[tauri::command]
fn toggle_achievement_overlay(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    toggle_achievement_overlay_inner(&app, &state)
}

fn toggle_achievement_overlay_inner(
    app: &AppHandle,
    state: &State<'_, AppState>,
) -> CommandResult<()> {
    if let Some(window) = app.get_webview_window("achievement-overlay") {
        return window.destroy().map_err(error);
    }
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    if !settings.achievement_overlay_enabled {
        return Err("Enable the in-game achievement overlay first".into());
    }
    active_game_id(state).ok_or_else(|| "No monitored game is currently running".to_string())?;
    WebviewWindowBuilder::new(
        app,
        "achievement-overlay",
        WebviewUrl::App("index.html".into()),
    )
    .title("Achievements Overlay")
    .inner_size(720.0, 620.0)
    .min_inner_size(420.0, 260.0)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(true)
    .visible(false)
    .build()
    .map(|_| ())
    .map_err(error)
}

fn configure_overlay_shortcut(app: &AppHandle, settings: &AppSettings) -> CommandResult<()> {
    app.global_shortcut().unregister_all().map_err(error)?;
    if settings.achievement_overlay_enabled {
        let shortcut = settings.achievement_overlay_hotkey.trim();
        if shortcut.is_empty() {
            return Err("The achievement overlay shortcut cannot be empty".into());
        }
        app.global_shortcut()
            .register(shortcut)
            .map_err(|register_error| {
                format!(
                    "Could not register achievement overlay shortcut {shortcut}: {register_error}"
                )
            })?;
    }
    Ok(())
}

#[tauri::command]
fn close_achievement_overlay(app: AppHandle) -> CommandResult<()> {
    if let Some(window) = app.get_webview_window("achievement-overlay") {
        window.destroy().map_err(error)?;
    }
    Ok(())
}

#[tauri::command]
fn launch_game(app: AppHandle, state: State<'_, AppState>, game_id: String) -> CommandResult<()> {
    let (settings, has_official_steam_source) = {
        let store = state.store.lock().map_err(lock_error)?;
        let settings = store.load_settings().map_err(error)?;
        let configured: BTreeMap<_, _> = settings
            .source_locations
            .iter()
            .map(|location| (location.id.as_str(), location.kind))
            .collect();
        let has_official_steam_source =
            store
                .observations()
                .map_err(error)?
                .iter()
                .any(|observation| {
                    observation.game_id == game_id
                        && configured
                            .get(observation.source_id.as_str())
                            .copied()
                            .or_else(|| inferred_source_kind(&observation.source_id))
                            == Some(aw_core::SourceKind::Steam)
                });
        (settings, has_official_steam_source)
    };
    let Some(config) = settings.game_launch_configs.get(&game_id) else {
        if has_official_steam_source && game_id.chars().all(|character| character.is_ascii_digit())
        {
            return launch_steam_uri(&game_id);
        }
        return Err("Configure this game's executable first".into());
    };
    if !config.executable.is_file() {
        return Err(format!(
            "Game executable was not found: {}",
            config.executable.display()
        ));
    }
    let mut command = Command::new(&config.executable);
    command
        .args(split_command_line(&config.arguments))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(directory) = config.executable.parent() {
        command.current_dir(directory);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    let mut child = command.spawn().map_err(error)?;
    state
        .launched_games
        .lock()
        .map_err(lock_error)?
        .insert(game_id.clone());
    if settings.notify_on_playtime && settings.notification_enabled {
        let event = playtime_notification(&state, &game_id, false);
        if let Err(message) = deliver_transient(&app, &state, event) {
            notification_log(&state, &format!("Playtime notification skipped: {message}"));
        }
    }
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        if child.wait().is_ok() {
            let seconds = started.elapsed().as_secs() as i64;
            let state = app.state::<AppState>();
            if let Ok(store) = state.store.lock() {
                let _ = store.record_play_session(&game_id, seconds, Utc::now().timestamp());
            }
            if let Ok(mut launched) = state.launched_games.lock() {
                launched.remove(&game_id);
            }
            if state
                .store
                .lock()
                .ok()
                .and_then(|store| store.load_settings().ok())
                .is_some_and(|settings| {
                    settings.notify_on_playtime && settings.notification_enabled
                })
            {
                let event = playtime_notification(&state, &game_id, true);
                if let Err(message) = deliver_transient(&app, &state, event) {
                    notification_log(&state, &format!("Playtime notification skipped: {message}"));
                }
            }
            let _ = app.emit("library-changed", ());
        }
    });
    Ok(())
}

#[cfg(windows)]
fn launch_steam_uri(game_id: &str) -> CommandResult<()> {
    use std::os::windows::process::CommandExt;
    Command::new("explorer.exe")
        .arg(format!("steam://run/{game_id}"))
        .creation_flags(0x0800_0000)
        .spawn()
        .map_err(error)?;
    Ok(())
}

#[cfg(not(windows))]
fn launch_steam_uri(_game_id: &str) -> CommandResult<()> {
    Err("Launching Steam games is available only on Windows".into())
}

fn split_command_line(value: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in value.chars() {
        match character {
            '"' => quoted = !quoted,
            character if character.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
            }
            character => current.push(character),
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

#[tauri::command]
fn list_achievements(
    state: State<'_, AppState>,
    source_id: String,
    game_id: String,
) -> CommandResult<Vec<aw_core::AchievementObservation>> {
    let store = state.store.lock().map_err(lock_error)?;
    let settings = store.load_settings().map_err(error)?;
    let mut observations: Vec<_> = if source_id == "catalog" {
        store.catalog_achievements(&game_id).map_err(error)?
    } else if source_id == "merged" {
        let enabled_sources: HashSet<_> = settings
            .source_locations
            .iter()
            .filter(|source| source.enabled && source_kind_enabled(&settings, source.kind))
            .map(|source| source.id.as_str())
            .collect();
        aw_core::merge_observations(
            store
                .observations()
                .map_err(error)?
                .into_iter()
                .filter(|item| {
                    item.game_id == game_id
                        && observation_source_enabled(
                            &settings,
                            &enabled_sources,
                            item.source_id.as_str(),
                        )
                })
                .collect(),
            settings.time_merge_recent_first,
        )
    } else {
        store
            .observations()
            .map_err(error)?
            .into_iter()
            .filter(|item| item.source_id == source_id && item.game_id == game_id)
            .collect()
    };
    store
        .enrich_observations(&mut observations)
        .map_err(error)?;
    observations.sort_by(|left, right| {
        right.achieved.cmp(&left.achieved).then_with(|| {
            left.display_name
                .as_ref()
                .unwrap_or(&left.achievement_id)
                .cmp(right.display_name.as_ref().unwrap_or(&right.achievement_id))
        })
    });
    Ok(observations)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GameSourceChoice {
    source_id: String,
    source_kind: Option<aw_core::SourceKind>,
}

#[tauri::command]
fn game_sources(
    state: State<'_, AppState>,
    game_id: String,
) -> CommandResult<Vec<GameSourceChoice>> {
    let store = state.store.lock().map_err(lock_error)?;
    let settings = store.load_settings().map_err(error)?;
    let configured: BTreeMap<_, _> = settings
        .source_locations
        .iter()
        .map(|location| (location.id.as_str(), location.kind))
        .collect();
    let enabled: HashSet<_> = settings
        .source_locations
        .iter()
        .filter(|location| location.enabled && source_kind_enabled(&settings, location.kind))
        .map(|location| location.id.as_str())
        .collect();
    let mut choices: BTreeMap<String, Option<aw_core::SourceKind>> = BTreeMap::new();
    for observation in store.observations().map_err(error)? {
        if observation.game_id != game_id
            || !observation_source_enabled(&settings, &enabled, &observation.source_id)
        {
            continue;
        }
        choices
            .entry(observation.source_id.clone())
            .or_insert_with(|| {
                configured
                    .get(observation.source_id.as_str())
                    .copied()
                    .or_else(|| inferred_source_kind(&observation.source_id))
            });
    }
    let mut result: Vec<_> = choices
        .into_iter()
        .map(|(source_id, source_kind)| GameSourceChoice {
            source_id,
            source_kind,
        })
        .collect();
    result.sort_by_key(|choice| choice.source_kind.map(source_priority).unwrap_or(u8::MAX));
    if settings.merge_duplicate && result.len() > 1 {
        result.insert(
            0,
            GameSourceChoice {
                source_id: "merged".into(),
                source_kind: result.first().and_then(|choice| choice.source_kind),
            },
        );
    }
    Ok(result)
}

#[tauri::command]
fn scan_sources(
    app: AppHandle,
    state: State<'_, AppState>,
    establish_baseline: Option<bool>,
) -> CommandResult<usize> {
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    let registry_count =
        sync_registry_sources(&app, &state, &settings, establish_baseline.unwrap_or(false))?;
    for location in settings.source_locations.iter().filter(|location| {
        settings.steam_enabled && location.enabled && location.kind == aw_core::SourceKind::Steam
    }) {
        let mut scanned_games = HashSet::new();
        for path in steam::stats_files(location) {
            if let Some((account_id, game_id)) = steam::stats_file_identity(&path)
                && steam_account_matches(settings.steam_account_id.as_deref(), &account_id)
            {
                scanned_games.insert(game_id.clone());
                if let Err(message) = sync_steam_game(
                    &app,
                    &state,
                    location,
                    &account_id,
                    &game_id,
                    establish_baseline.unwrap_or(false),
                ) {
                    notification_log(
                        &state,
                        &format!("Steam scan skipped app {game_id}: {message}"),
                    );
                }
            }
        }
        if settings.steam_library_mode == "installed" {
            let detected_accounts = steam::accounts(&settings.source_locations);
            let account_id = settings
                .steam_account_id
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    detected_accounts
                        .first()
                        .map(|account| account.account_id.as_str())
                })
                .unwrap_or_default()
                .to_owned();
            for game_id in steam::installed_app_ids(location) {
                if scanned_games.insert(game_id.clone())
                    && let Err(message) =
                        sync_steam_game(&app, &state, location, &account_id, &game_id, true)
                {
                    notification_log(
                        &state,
                        &format!("Steam installed scan skipped app {game_id}: {message}"),
                    );
                }
            }
        } else if settings.steam_library_mode == "owned" {
            let detected_accounts = steam::accounts(&settings.source_locations);
            let account_id = settings
                .steam_account_id
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    detected_accounts
                        .first()
                        .map(|account| account.account_id.as_str())
                })
                .unwrap_or_default();
            match owned_steam_games(account_id, settings.steam_api_key.trim()) {
                Ok(owned) => {
                    for (game_id, name, icon) in owned {
                        if let Ok(store) = state.store.lock() {
                            let _ = store.save_game_metadata(&game_id, &name, icon.as_deref());
                        }
                        if scanned_games.insert(game_id.clone())
                            && let Err(message) =
                                sync_steam_game(&app, &state, location, account_id, &game_id, true)
                        {
                            notification_log(
                                &state,
                                &format!("Steam owned scan skipped app {game_id}: {message}"),
                            );
                        }
                    }
                }
                Err(message) => notification_log(&state, &format!("Steam owned games: {message}")),
            }
        }
    }
    let active_locations: Vec<_> = settings
        .source_locations
        .iter()
        .filter(|location| location.enabled && source_kind_enabled(&settings, location.kind))
        .cloned()
        .collect();
    let files = source::discover_files(&active_locations);
    let total = files.len() + registry_count;
    let mut processed = registry_count;
    for (index, path) in files.into_iter().enumerate() {
        match process_path(&app, &state, &path, establish_baseline.unwrap_or(false)) {
            Ok(()) => processed += 1,
            Err(message) => notification_log(
                &state,
                &format!("Source scan skipped {}: {message}", path.display()),
            ),
        }
        let _ = app.emit(
            "scan-progress",
            serde_json::json!({ "completed": registry_count + index + 1, "total": total }),
        );
    }
    configure_watcher(&app, &state, &settings)?;
    dispatch_pending(&app, &state)?;
    let _ = app.emit("library-changed", ());
    Ok(processed)
}

fn sync_registry_sources(
    app: &AppHandle,
    state: &State<'_, AppState>,
    settings: &AppSettings,
    baseline: bool,
) -> CommandResult<usize> {
    let mut observations =
        registry::observations(settings.green_luma_enabled, settings.luma_play_enabled);
    let count = observations.len();
    if observations.is_empty() {
        return Ok(0);
    }
    let events = {
        let mut store = state.store.lock().map_err(lock_error)?;
        store
            .enrich_observations(&mut observations)
            .map_err(error)?;
        store
            .record_observations(&observations, baseline)
            .map_err(error)?
    };
    handle_created_events(app, state, settings, events, true)?;
    Ok(count)
}

fn handle_created_events(
    app: &AppHandle,
    state: &State<'_, AppState>,
    settings: &AppSettings,
    events: Vec<NotificationEvent>,
    source_notifications: bool,
) -> CommandResult<()> {
    for event in events {
        if !source_notifications {
            if event.id >= 0 {
                state
                    .store
                    .lock()
                    .map_err(lock_error)?
                    .record_delivery(event.id, "source_disabled", Ok(()))
                    .map_err(error)?;
            }
            continue;
        }
        if event.kind == aw_core::NotificationKind::Unlock && event.attempts == 0 {
            let elapsed = Utc::now().timestamp() - event.observation.unlock_time;
            let timestamp_is_stale = event.observation.unlock_time > 0
                && (elapsed < 0 || elapsed > i64::from(settings.notification_max_age_seconds));
            if timestamp_is_stale {
                notification_log(
                    state,
                    &format!("Stale achievement event ignored (timestamp age {elapsed}s)"),
                );
                if event.id >= 0 {
                    state
                        .store
                        .lock()
                        .map_err(lock_error)?
                        .record_delivery(event.id, "stale", Ok(()))
                        .map_err(error)?;
                }
                continue;
            }
            if !notification_game_is_running(state, settings, &event.observation.game_id) {
                notification_log(
                    state,
                    "Achievement event ignored because the configured game is not running",
                );
                if event.id >= 0 {
                    state
                        .store
                        .lock()
                        .map_err(lock_error)?
                        .record_delivery(event.id, "game_not_running", Ok(()))
                        .map_err(error)?;
                }
                continue;
            }
        }
        if event.kind == aw_core::NotificationKind::Unlock && event.attempts == 0 {
            if settings.screenshot_enabled {
                let achievement = event
                    .observation
                    .display_name
                    .as_deref()
                    .unwrap_or(&event.observation.achievement_id);
                let root = settings
                    .screenshot_directory
                    .clone()
                    .unwrap_or_else(|| state.data_dir.join("screenshots"));
                if let Err(message) = capture_primary_display(
                    &root,
                    &event.observation.game_id,
                    achievement,
                    settings.screenshot_overwrite,
                ) {
                    notification_log(state, &format!("Screenshot skipped: {message}"));
                }
            }
            if settings.obs_replay_enabled {
                let obs_settings = settings.clone();
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    match obs::save_replay(&obs_settings).await {
                        Ok(()) => notification_log(&state, "OBS replay buffer saved"),
                        Err(message) => {
                            notification_log(&state, &format!("OBS replay skipped: {message}"))
                        }
                    }
                });
            }
            if settings.custom_action_enabled {
                run_custom_action(state, settings, &event)
                    .map_err(|message| {
                        notification_log(state, &format!("Custom action skipped: {message}"));
                        message
                    })
                    .ok();
            }
        }
        if !settings.notification_enabled
            || (event.kind == aw_core::NotificationKind::Progress && !settings.notify_on_progress)
        {
            if event.id >= 0 {
                state
                    .store
                    .lock()
                    .map_err(lock_error)?
                    .record_delivery(event.id, "disabled", Ok(()))
                    .map_err(error)?;
            }
            continue;
        }
        if event.kind == aw_core::NotificationKind::Unlock
            && event.attempts == 0
            && settings.rumble_enabled
        {
            rumble::pulse(
                settings.rumble_strength_percent,
                settings.rumble_duration_ms,
            );
        }
        if settings.websocket_enabled {
            let game_name = state
                .store
                .lock()
                .ok()
                .and_then(|store| {
                    store
                        .game_metadata(&event.observation.game_id)
                        .ok()
                        .flatten()
                })
                .map(|metadata| metadata.0);
            if let Err(message) = state.websocket.broadcast(&event, game_name.as_deref()) {
                notification_log(state, &format!("WebSocket broadcast skipped: {message}"));
            }
        }
        if settings.gntp_enabled {
            let host = settings.gntp_host.clone();
            let port = settings.gntp_port;
            let gntp_event = event.clone();
            let handle = app.clone();
            std::thread::spawn(move || {
                if let Err(message) = gntp::send(&host, port, &gntp_event) {
                    notification_log(
                        &handle.state::<AppState>(),
                        &format!("GNTP delivery skipped: {message}"),
                    );
                }
            });
        }
        let _ = app.emit("achievement-detected", event);
    }
    Ok(())
}

fn notification_game_is_running(
    state: &State<'_, AppState>,
    settings: &AppSettings,
    game_id: &str,
) -> bool {
    if !settings.notification_require_running_game
        || steam::running_app_id().as_deref() == Some(game_id)
        || foreground_is_fullscreen()
        || state
            .launched_games
            .lock()
            .is_ok_and(|games| games.contains(game_id))
    {
        return true;
    }
    let Some(executable) = settings
        .game_launch_configs
        .get(game_id)
        .and_then(|config| config.executable.file_name())
        .and_then(|name| name.to_str())
    else {
        return true;
    };
    let running = process::running_names();
    let executable = executable.to_ascii_lowercase();
    running.contains(&executable)
        || executable
            .strip_suffix(".exe")
            .is_some_and(|stem| running.contains(&format!("{stem}-win64-shipping.exe")))
}

fn run_custom_action(
    state: &State<'_, AppState>,
    settings: &AppSettings,
    event: &NotificationEvent,
) -> CommandResult<()> {
    if !settings.custom_action_executable.is_file() {
        return Err("configured executable was not found".into());
    }
    let values = [
        ("{game_id}", event.observation.game_id.as_str()),
        (
            "{achievement_id}",
            event.observation.achievement_id.as_str(),
        ),
        (
            "{name}",
            event
                .observation
                .display_name
                .as_deref()
                .unwrap_or_default(),
        ),
        ("{source}", event.observation.source_id.as_str()),
    ];
    let arguments = split_command_line(&settings.custom_action_arguments)
        .into_iter()
        .map(|mut argument| {
            for (token, value) in values {
                argument = argument.replace(token, value);
            }
            argument
        });
    let mut command = Command::new(&settings.custom_action_executable);
    let game_name = state
        .store
        .lock()
        .ok()
        .and_then(|store| {
            store
                .game_metadata(&event.observation.game_id)
                .ok()
                .flatten()
        })
        .map(|metadata| metadata.0)
        .unwrap_or_else(|| event.observation.game_id.clone());
    command
        .args(arguments)
        .env("AW_APPID", &event.observation.game_id)
        .env("AW_GAME", game_name)
        .env("AW_ACHIEVEMENT", &event.observation.achievement_id)
        .env(
            "AW_DISPLAYNAME",
            event
                .observation
                .display_name
                .as_deref()
                .unwrap_or_default(),
        )
        .env(
            "AW_DESCRIPTION",
            event.observation.description.as_deref().unwrap_or_default(),
        )
        .env(
            "AW_ICON",
            event.observation.icon.as_deref().unwrap_or_default(),
        )
        .env("AW_TIME", event.observation.unlock_time.to_string())
        .env("AW_SOURCE", &event.observation.source_id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(directory) = settings
        .custom_action_working_directory
        .as_deref()
        .filter(|directory| directory.is_dir())
    {
        command.current_dir(directory);
    }
    #[cfg(windows)]
    if settings.custom_action_hide_window {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0000_0008 | 0x0800_0000);
    }
    command.spawn().map(|_| ()).map_err(error)
}

#[derive(Deserialize, Default)]
struct CommunityAchievements {
    #[serde(rename = "achievement", default)]
    items: Vec<CommunityAchievement>,
}

#[derive(Deserialize, Default)]
struct CommunityStats {
    #[serde(default)]
    achievements: CommunityAchievements,
}

#[derive(Deserialize)]
struct CommunityAchievement {
    apiname: String,
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "iconClosed")]
    icon_closed: Option<String>,
    #[serde(rename = "iconOpen")]
    icon_open: Option<String>,
    #[serde(rename = "@closed", default)]
    closed: u8,
    #[serde(rename = "unlockTimestamp", default)]
    unlock_timestamp: i64,
}

#[derive(Deserialize, Default)]
struct ApiPlayerStats {
    #[serde(default)]
    achievements: Vec<ApiAchievement>,
}

#[derive(Deserialize, Default)]
struct ApiResponse {
    #[serde(default)]
    playerstats: ApiPlayerStats,
}

#[derive(Deserialize)]
struct ApiAchievement {
    apiname: String,
    achieved: u8,
    #[serde(default)]
    unlocktime: i64,
    name: Option<String>,
    description: Option<String>,
}

#[tauri::command]
fn refresh_metadata(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: Option<String>,
) -> CommandResult<usize> {
    // A game-specific refresh is an explicit user request. Re-fetch every
    // metadata layer instead of treating an existing cache row as success.
    let force_refresh = game_id.is_some();
    let games: BTreeMap<String, aw_core::SourceKind> = {
        let store = state.store.lock().map_err(lock_error)?;
        let settings = store.load_settings().map_err(error)?;
        let configured: BTreeMap<_, _> = settings
            .source_locations
            .iter()
            .map(|location| (location.id.as_str(), location.kind))
            .collect();
        store
            .observations()
            .map_err(error)?
            .into_iter()
            .filter_map(|item| {
                configured
                    .get(item.source_id.as_str())
                    .copied()
                    .or_else(|| inferred_source_kind(&item.source_id))
                    .map(|kind| (item.game_id, kind))
            })
            .filter(|(observed_game_id, _)| {
                game_id
                    .as_deref()
                    .is_none_or(|requested| requested == observed_game_id)
            })
            .fold(BTreeMap::new(), |mut games, (game_id, kind)| {
                let current = games.entry(game_id).or_insert(kind);
                if source_priority(kind) < source_priority(*current) {
                    *current = kind;
                }
                games
            })
    };
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(6))
        .user_agent("Achievement-Watcher/0.1")
        .build();
    let mut updated = 0;
    for (game_id, source_kind) in games {
        if source_kind == aw_core::SourceKind::Epic {
            if import_epic_metadata(&agent, &state, &game_id)? {
                updated += 1;
            }
            continue;
        }
        if source_kind == aw_core::SourceKind::Gog {
            if import_gog_metadata(&agent, &state, &game_id)? {
                updated += 1;
            }
            continue;
        }
        if !source_uses_steam_metadata(source_kind)
            || !game_id.chars().all(|character| character.is_ascii_digit())
        {
            continue;
        }
        let (needs_game, needs_achievements, needs_global_percentages) = {
            let store = state.store.lock().map_err(lock_error)?;
            (
                force_refresh || store.game_metadata(&game_id).map_err(error)?.is_none(),
                force_refresh || !store.has_achievement_metadata(&game_id).map_err(error)?,
                force_refresh || !store.has_global_percentages(&game_id).map_err(error)?,
            )
        };
        if needs_game {
            let url = format!("https://store.steampowered.com/api/appdetails?appids={game_id}");
            if let Ok(response) = agent.get(&url).call()
                && let Ok(value) = response.into_json::<serde_json::Value>()
                && let Some(name) = value
                    .get(&game_id)
                    .and_then(|item| item.get("data"))
                    .and_then(|data| data.get("name"))
                    .and_then(|name| name.as_str())
            {
                let icon = format!(
                    "https://cdn.cloudflare.steamstatic.com/steam/apps/{game_id}/header.jpg"
                );
                state
                    .store
                    .lock()
                    .map_err(lock_error)?
                    .save_game_metadata(&game_id, name, Some(&icon))
                    .map_err(error)?;
                updated += 1;
            }
        }
        if needs_achievements && import_community_schema(&agent, &state, &game_id)? {
            updated += 1;
        }
        if needs_global_percentages && import_global_percentages(&agent, &state, &game_id)? {
            updated += 1;
        }
    }
    if updated > 0 {
        let _ = app.emit("library-changed", ());
    }
    Ok(updated)
}

#[tauri::command]
fn clear_game_metadata(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
) -> CommandResult<()> {
    state
        .store
        .lock()
        .map_err(lock_error)?
        .clear_game_metadata(&game_id)
        .map_err(error)?;
    let _ = app.emit("library-changed", ());
    Ok(())
}

#[tauri::command]
fn reset_game_activity(
    app: AppHandle,
    state: State<'_, AppState>,
    game_id: String,
) -> CommandResult<()> {
    state
        .store
        .lock()
        .map_err(lock_error)?
        .reset_game_activity(&game_id)
        .map_err(error)?;
    let _ = app.emit("library-changed", ());
    Ok(())
}

fn import_global_percentages(
    agent: &ureq::Agent,
    state: &State<'_, AppState>,
    game_id: &str,
) -> CommandResult<bool> {
    let url = format!(
        "https://api.steampowered.com/ISteamUserStats/GetGlobalAchievementPercentagesForApp/v2/?gameid={game_id}"
    );
    let Ok(response) = agent.get(&url).call() else {
        return Ok(false);
    };
    let Ok(value) = response.into_json::<serde_json::Value>() else {
        return Ok(false);
    };
    let Some(items) = value
        .pointer("/achievementpercentages/achievements")
        .and_then(|value| value.as_array())
    else {
        return Ok(false);
    };
    let store = state.store.lock().map_err(lock_error)?;
    let mut saved = false;
    for item in items {
        let Some(name) = item.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(percent) = item.get("percent").and_then(|value| value.as_f64()) else {
            continue;
        };
        store
            .save_global_percent(
                game_id,
                name,
                (percent.clamp(0.0, 100.0) * 100.0).round() as u32,
            )
            .map_err(error)?;
        saved = true;
    }
    Ok(saved)
}

fn import_epic_metadata(
    agent: &ureq::Agent,
    state: &State<'_, AppState>,
    game_id: &str,
) -> CommandResult<bool> {
    let (needs_game, needs_achievements) = {
        let store = state.store.lock().map_err(lock_error)?;
        (
            store.game_metadata(game_id).map_err(error)?.is_none(),
            !store.has_achievement_metadata(game_id).map_err(error)?,
        )
    };
    let mut saved = false;
    if needs_game
        && let Ok(response) = agent
            .get("https://store-content.ak.epicgames.com/api/content/productmapping")
            .call()
        && let Ok(mapping) = response.into_json::<serde_json::Value>()
        && let Some(slug) = mapping.get(game_id).and_then(|value| value.as_str())
        && let Ok(response) = agent
            .get(&format!(
                "https://store-content.ak.epicgames.com/api/en-US/content/products/{slug}"
            ))
            .call()
        && let Ok(product) = response.into_json::<serde_json::Value>()
        && let Some(name) = product.get("productName").and_then(|value| value.as_str())
    {
        state
            .store
            .lock()
            .map_err(lock_error)?
            .save_game_metadata(game_id, name, None)
            .map_err(error)?;
        saved = true;
    }
    if needs_achievements {
        let url = format!(
            "https://api.epicgames.dev/epic/achievements/v1/public/achievements/product/{game_id}/locale/en-us?includeAchievements=true"
        );
        if let Ok(response) = agent.get(&url).call()
            && let Ok(value) = response.into_json::<serde_json::Value>()
            && let Some(items) = value.get("achievements").and_then(|value| value.as_array())
        {
            let store = state.store.lock().map_err(lock_error)?;
            for item in items {
                let achievement = item.get("achievement").unwrap_or(item);
                let Some(id) = achievement.get("name").and_then(|value| value.as_str()) else {
                    continue;
                };
                let display_name = achievement
                    .get("lockedDisplayName")
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
                    .or_else(|| {
                        achievement
                            .get("unlockedDisplayName")
                            .and_then(|value| value.as_str())
                    });
                let description = achievement
                    .get("lockedDescription")
                    .and_then(|value| value.as_str());
                let icon = achievement
                    .get("unlockedIconLink")
                    .and_then(|value| value.as_str());
                let locked_icon = achievement
                    .get("lockedIconLink")
                    .and_then(|value| value.as_str());
                let hidden = achievement
                    .get("hidden")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                store
                    .save_achievement_metadata(
                        game_id,
                        id,
                        display_name,
                        description,
                        icon,
                        locked_icon,
                        hidden,
                    )
                    .map_err(error)?;
                saved = true;
            }
        }
    }
    Ok(saved)
}

fn import_gog_metadata(
    agent: &ureq::Agent,
    state: &State<'_, AppState>,
    game_id: &str,
) -> CommandResult<bool> {
    let url = format!("https://gamesdb.gog.com/platforms/gog/external_releases/{game_id}");
    let Ok(response) = agent.get(&url).call() else {
        return Ok(false);
    };
    let Ok(value) = response.into_json::<serde_json::Value>() else {
        return Ok(false);
    };
    let title = value
        .pointer("/game/title")
        .or_else(|| value.pointer("/game/name"))
        .and_then(|value| value.as_str());
    let steam_id = value
        .pointer("/game/releases")
        .and_then(|value| value.as_array())
        .and_then(|releases| {
            releases.iter().find_map(|release| {
                (release.get("platform_id").and_then(|value| value.as_str()) == Some("steam"))
                    .then(|| release.get("external_id").and_then(|value| value.as_str()))
                    .flatten()
            })
        });
    let canonical_game_id = steam_id.unwrap_or(game_id);
    if let Some(steam_id) = steam_id {
        state
            .store
            .lock()
            .map_err(lock_error)?
            .save_game_alias(game_id, steam_id)
            .map_err(error)?;
    }
    let mut saved = false;
    if let Some(name) = title {
        let icon = steam_id
            .map(|id| format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{id}/header.jpg"));
        state
            .store
            .lock()
            .map_err(lock_error)?
            .save_game_metadata(canonical_game_id, name, icon.as_deref())
            .map_err(error)?;
        saved = true;
    }
    if let Some(steam_id) = steam_id
        && import_community_schema_as(agent, state, steam_id, canonical_game_id)?
    {
        saved = true;
    }
    Ok(saved)
}

fn import_community_schema(
    agent: &ureq::Agent,
    state: &State<'_, AppState>,
    game_id: &str,
) -> CommandResult<bool> {
    import_community_schema_as(agent, state, game_id, game_id)
}

fn import_community_schema_as(
    agent: &ureq::Agent,
    state: &State<'_, AppState>,
    steam_game_id: &str,
    metadata_game_id: &str,
) -> CommandResult<bool> {
    // Public fallback profiles used by SteamAutoCrack's MIT-licensed community scraper.
    const PROFILES: [&str; 8] = [
        "76561198028121353",
        "76561197979911851",
        "76561198017975643",
        "76561197993544755",
        "76561198355953202",
        "76561198001237877",
        "76561198237402290",
        "76561198152618007",
    ];
    for profile in PROFILES {
        let url =
            format!("https://steamcommunity.com/profiles/{profile}/stats/{steam_game_id}/?xml=1");
        let Ok(response) = agent.get(&url).call() else {
            continue;
        };
        let Ok(xml) = response.into_string() else {
            continue;
        };
        let Ok(schema) = quick_xml::de::from_str::<CommunityStats>(&xml) else {
            continue;
        };
        if schema.achievements.items.is_empty() {
            continue;
        }
        let store = state.store.lock().map_err(lock_error)?;
        for achievement in schema.achievements.items {
            store
                .save_achievement_metadata(
                    metadata_game_id,
                    &achievement.apiname,
                    achievement.name.as_deref(),
                    achievement.description.as_deref(),
                    achievement.icon_open.as_deref(),
                    achievement.icon_closed.as_deref(),
                    false,
                )
                .map_err(error)?;
        }
        return Ok(true);
    }
    Ok(false)
}

#[tauri::command]
fn test_notification(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    if settings.notification_mode == NotificationMode::NativeOnly {
        deliver_native(&app, &state, &sample_notification())
    } else {
        show_overlay(&app, &state, sample_notification())
    }
}

#[tauri::command]
fn test_progress_notification(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    deliver_transient(&app, &state, sample_progress_notification())
}

#[tauri::command]
fn test_playtime_notification(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    deliver_transient(&app, &state, sample_playtime_notification())
}

fn deliver_transient(
    app: &AppHandle,
    state: &State<'_, AppState>,
    event: NotificationEvent,
) -> CommandResult<()> {
    if state.current_overlay.lock().map_err(lock_error)?.is_some() {
        return Err("Another custom notification is already visible".into());
    }
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    if settings.notification_mode == NotificationMode::NativeOnly {
        deliver_native(app, state, &event)
    } else {
        show_overlay(app, state, event)
    }
}

fn playtime_notification(
    state: &State<'_, AppState>,
    game_id: &str,
    finished: bool,
) -> NotificationEvent {
    let (name, icon, seconds) = state
        .store
        .lock()
        .ok()
        .map(|store| {
            let metadata = store.game_metadata(game_id).ok().flatten();
            let seconds = store
                .game_activity(game_id)
                .map(|value| value.0)
                .unwrap_or(0);
            (
                metadata
                    .as_ref()
                    .map(|value| value.0.clone())
                    .unwrap_or_else(|| format!("Steam game {game_id}")),
                metadata.and_then(|value| value.1),
                seconds,
            )
        })
        .unwrap_or_else(|| (format!("Steam game {game_id}"), None, 0));
    let description = if finished {
        let hours = seconds / 3_600;
        let minutes = (seconds % 3_600) / 60;
        if hours > 0 {
            format!("Stopped tracking • {hours}h {minutes}m total")
        } else {
            format!("Stopped tracking • {minutes} minutes total")
        }
    } else {
        "Tracking playtime".into()
    };
    NotificationEvent {
        id: -1,
        event_key: format!(
            "playtime:{}:{game_id}:{}",
            if finished { "stop" } else { "start" },
            Utc::now().timestamp()
        ),
        kind: aw_core::NotificationKind::Unlock,
        observation: aw_core::AchievementObservation {
            source_id: "steam".into(),
            origin_source_id: None,
            game_id: game_id.into(),
            achievement_id: "playtime".into(),
            achieved: true,
            hidden: false,
            global_percent_hundredths: None,
            trophy_grade: None,
            current_progress: 0,
            max_progress: 0,
            unlock_time: Utc::now().timestamp(),
            display_name: Some(name),
            description: Some(description),
            icon,
        },
        attempts: 0,
        next_attempt_at: 0,
    }
}

#[tauri::command]
fn test_game_bar(state: State<'_, AppState>) -> CommandResult<()> {
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    if !settings.game_bar_enabled {
        return Err("Enable the Game Bar companion transport first".into());
    }
    state
        .game_bar
        .deliver(&settings.game_bar_token, &sample_notification())
}

#[tauri::command]
fn test_gntp(state: State<'_, AppState>) -> CommandResult<()> {
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    if !settings.gntp_enabled {
        return Err("Enable the GNTP transport first".into());
    }
    gntp::send(
        &settings.gntp_host,
        settings.gntp_port,
        &sample_notification(),
    )
}

#[tauri::command]
async fn test_obs(state: State<'_, AppState>) -> CommandResult<()> {
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    obs::save_replay(&settings).await
}

fn sample_notification() -> NotificationEvent {
    let observation = aw_core::AchievementObservation {
        source_id: "test".into(),
        origin_source_id: None,
        game_id: "400".into(),
        achievement_id: "PORTAL_TRANSMISSION_RECEIVED".into(),
        achieved: true,
        hidden: false,
        global_percent_hundredths: None,
        trophy_grade: None,
        current_progress: 0,
        max_progress: 0,
        unlock_time: Utc::now().timestamp(),
        display_name: Some("Transmission Received".into()),
        description: Some("This is a delivery-path test.".into()),
        icon: None,
    };
    NotificationEvent {
        id: -1,
        event_key: "test".into(),
        kind: aw_core::NotificationKind::Unlock,
        observation,
        attempts: 0,
        next_attempt_at: 0,
    }
}

fn sample_progress_notification() -> NotificationEvent {
    let mut event = sample_notification();
    event.event_key = "progress-test".into();
    event.kind = aw_core::NotificationKind::Progress;
    event.observation.achieved = false;
    event.observation.current_progress = 7;
    event.observation.max_progress = 10;
    event.observation.display_name = Some("Long Jump".into());
    event.observation.description = Some("Achievement progress updated".into());
    event
}

fn sample_playtime_notification() -> NotificationEvent {
    let mut event = sample_notification();
    event.event_key = "playtime-test".into();
    event.observation.achievement_id = "playtime".into();
    event.observation.display_name = Some("Portal".into());
    event.observation.description = Some("Tracking playtime".into());
    event
}

#[tauri::command]
fn acknowledge_notification(
    app: AppHandle,
    state: State<'_, AppState>,
    event_id: i64,
) -> CommandResult<DeliveryReceipt> {
    notification_log(&state, &format!("renderer acknowledged event {event_id}"));
    let window = app
        .get_webview_window("notification")
        .ok_or_else(|| "Notification renderer closed before acknowledgement".to_string())?;
    let position = state
        .store
        .lock()
        .ok()
        .and_then(|store| store.load_settings().ok())
        .map(|settings| settings.notification_position)
        .unwrap_or_else(|| "bottom_right".into());
    position_notification(&window, &position);
    window.show().map_err(error)?;
    state
        .awaiting_overlay
        .lock()
        .map_err(lock_error)?
        .remove(&event_id);
    if event_id >= 0 {
        state
            .store
            .lock()
            .map_err(lock_error)?
            .record_delivery(event_id, "overlay", Ok(()))
            .map_err(error)?;
    }
    let receipt = DeliveryReceipt {
        event_id,
        transport: "overlay".into(),
        success: true,
        error: None,
    };
    let _ = app.emit("notification-status", &receipt);
    Ok(receipt)
}

#[tauri::command]
fn current_notification(state: State<'_, AppState>) -> CommandResult<Option<NotificationEvent>> {
    Ok(state.current_overlay.lock().map_err(lock_error)?.clone())
}

#[tauri::command]
fn close_notification(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    notification_log(&state, "renderer requested close");
    *state.current_overlay.lock().map_err(lock_error)? = None;
    if let Some(window) = app.get_webview_window("notification") {
        window.destroy().map_err(error)?;
    }
    dispatch_pending(&app, &state)
}

#[tauri::command]
fn open_notification_game(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    let event = state
        .current_overlay
        .lock()
        .map_err(lock_error)?
        .clone()
        .ok_or_else(|| "No notification is currently open".to_string())?;
    show_main_window(&app)?;
    app.emit_to(
        "main",
        "open-game",
        serde_json::json!({
            "sourceId": event.observation.source_id,
            "gameId": event.observation.game_id,
            "achievementId": event.observation.achievement_id,
        }),
    )
    .map_err(error)?;
    close_notification(app, state)
}

#[tauri::command]
fn report_notification_error(state: State<'_, AppState>, message: String) {
    notification_log(&state, &format!("renderer error: {message}"));
}

#[tauri::command]
fn capture_screenshot(
    state: State<'_, AppState>,
    game: String,
    achievement: String,
) -> CommandResult<Option<PathBuf>> {
    if !state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?
        .screenshot_enabled
    {
        return Ok(None);
    }
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    let root = settings
        .screenshot_directory
        .clone()
        .unwrap_or_else(|| state.data_dir.join("screenshots"));
    capture_primary_display(&root, &game, &achievement, settings.screenshot_overwrite).map(Some)
}

fn process_path(
    app: &AppHandle,
    state: &State<'_, AppState>,
    path: &Path,
    baseline: bool,
) -> CommandResult<()> {
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    let location = settings
        .source_locations
        .iter()
        .find(|location| path.starts_with(&location.path))
        .ok_or_else(|| format!("No configured source owns {}", path.display()))?;
    if location.kind == aw_core::SourceKind::Steam {
        let (account_id, game_id) = steam::stats_file_identity(path)
            .ok_or_else(|| "Not an official Steam stats cache file".to_string())?;
        if !settings.steam_enabled
            || !steam_account_matches(settings.steam_account_id.as_deref(), &account_id)
        {
            return Ok(());
        }
        return sync_steam_game(app, state, location, &account_id, &game_id, baseline);
    }
    let inferred_game_id = source::infer_game_id(path)
        .ok_or_else(|| format!("Could not infer a game ID from {}", path.display()))?;
    let game_id = state
        .store
        .lock()
        .map_err(lock_error)?
        .canonical_game_id(&inferred_game_id)
        .map_err(error)?;
    source::read_when_stable(path, 5, Duration::from_millis(100)).map_err(error)?;
    let mut observations =
        parser::parse_achievement_file(path, &location.id, &game_id).map_err(error)?;
    let rpcs3_game = if location.kind == aw_core::SourceKind::Rpcs3 {
        parser::enrich_rpcs3_schema(path, &mut observations).map_err(error)?
    } else {
        None
    };
    let events = {
        let mut store = state.store.lock().map_err(lock_error)?;
        if let Some((name, icon)) = rpcs3_game {
            store
                .save_game_metadata(&game_id, &name, icon.as_deref())
                .map_err(error)?;
            for observation in &observations {
                store
                    .save_achievement_metadata(
                        &game_id,
                        &observation.achievement_id,
                        observation.display_name.as_deref(),
                        observation.description.as_deref(),
                        observation.icon.as_deref(),
                        observation.icon.as_deref(),
                        observation.hidden,
                    )
                    .map_err(error)?;
            }
        }
        store
            .enrich_observations(&mut observations)
            .map_err(error)?;
        store
            .record_observations(&observations, baseline)
            .map_err(error)?
    };
    handle_created_events(app, state, &settings, events, location.notify)
}

fn sync_steam_game(
    app: &AppHandle,
    state: &State<'_, AppState>,
    location: &aw_core::SourceLocation,
    account_id: &str,
    game_id: &str,
    baseline: bool,
) -> CommandResult<()> {
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    let mut observations = match steam::read_client_snapshot(&location.id, game_id) {
        Ok(observations) => observations,
        Err(client_error) if settings.steam_public_fallback => read_steam_fallback(
            &location.id,
            account_id,
            game_id,
            settings.steam_api_key.trim(),
        )
        .map_err(|fallback_error| {
            format!("Steam client: {client_error}; Steam fallback: {fallback_error}")
        })?,
        Err(client_error) => return Err(client_error),
    };
    let events = {
        let mut store = state.store.lock().map_err(lock_error)?;
        store
            .enrich_observations(&mut observations)
            .map_err(error)?;
        store
            .record_observations(&observations, baseline)
            .map_err(error)?
    };
    handle_created_events(app, state, &settings, events, true)
}

fn steam_account_matches(configured: Option<&str>, account_id: &str) -> bool {
    configured.is_none_or(|configured| {
        let configured = configured.trim();
        configured.is_empty()
            || configured == account_id
            || configured == steam_id64(account_id).to_string()
    })
}

fn steam_id64(account_id: &str) -> u64 {
    76_561_197_960_265_728 + account_id.parse::<u64>().unwrap_or_default()
}

fn owned_steam_games(
    account_id: &str,
    api_key: &str,
) -> CommandResult<Vec<(String, String, Option<String>)>> {
    if account_id.trim().is_empty() {
        return Err("No signed-in Steam account was detected".into());
    }
    let steam_id = if account_id.len() >= 16 {
        account_id.parse::<u64>().map_err(error)?
    } else {
        steam_id64(account_id)
    };
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .user_agent("Achievement-Watcher/0.1")
        .build();
    if !api_key.is_empty() {
        let value = agent
            .get("https://api.steampowered.com/IPlayerService/GetOwnedGames/v1/")
            .query("key", api_key)
            .query("steamid", &steam_id.to_string())
            .query("include_appinfo", "1")
            .query("include_played_free_games", "1")
            .call()
            .map_err(error)?
            .into_json::<serde_json::Value>()
            .map_err(error)?;
        if let Some(games) = value
            .pointer("/response/games")
            .and_then(|value| value.as_array())
        {
            return Ok(games
                .iter()
                .filter_map(|game| {
                    let app_id = game.get("appid")?.as_u64()?.to_string();
                    let name = game.get("name")?.as_str()?.to_owned();
                    let icon_hash = game.get("img_icon_url").and_then(|value| value.as_str());
                    let icon = icon_hash.filter(|value| !value.is_empty()).map(|hash| {
                        format!("https://media.steampowered.com/steamcommunity/public/images/apps/{app_id}/{hash}.jpg")
                    });
                    Some((app_id, name, icon))
                })
                .collect());
        }
    }
    #[derive(Deserialize, Default)]
    struct GamesList {
        #[serde(rename = "game", default)]
        games: Vec<OwnedGame>,
    }
    #[derive(Deserialize, Default)]
    struct OwnedGames {
        #[serde(default)]
        games: GamesList,
    }
    #[derive(Deserialize)]
    struct OwnedGame {
        #[serde(rename = "appID")]
        app_id: String,
        name: String,
        logo: Option<String>,
    }
    let xml = agent
        .get(&format!(
            "https://steamcommunity.com/profiles/{steam_id}/games?tab=all&xml=1"
        ))
        .call()
        .map_err(error)?
        .into_string()
        .map_err(error)?;
    let games = quick_xml::de::from_str::<OwnedGames>(&xml).map_err(error)?;
    if games.games.games.is_empty() {
        return Err("Steam returned no owned games; the profile may be private".into());
    }
    Ok(games
        .games
        .games
        .into_iter()
        .map(|game| (game.app_id, game.name, game.logo))
        .collect())
}

fn read_steam_fallback(
    source_id: &str,
    account_id: &str,
    game_id: &str,
    api_key: &str,
) -> CommandResult<Vec<aw_core::AchievementObservation>> {
    let steam_id = if account_id.len() >= 16 {
        account_id.parse::<u64>().map_err(error)?
    } else {
        steam_id64(account_id)
    };
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .user_agent("Achievement-Watcher/0.1")
        .build();
    if !api_key.is_empty() {
        let response = agent
            .get("https://api.steampowered.com/ISteamUserStats/GetPlayerAchievements/v1/")
            .query("appid", game_id)
            .query("steamid", &steam_id.to_string())
            .query("key", api_key)
            .query("l", "english")
            .call()
            .map_err(error)?
            .into_json::<ApiResponse>()
            .map_err(error)?;
        if !response.playerstats.achievements.is_empty() {
            return Ok(response
                .playerstats
                .achievements
                .into_iter()
                .map(|achievement| aw_core::AchievementObservation {
                    source_id: source_id.into(),
                    origin_source_id: None,
                    game_id: game_id.into(),
                    achievement_id: achievement.apiname,
                    achieved: achievement.achieved != 0,
                    hidden: false,
                    global_percent_hundredths: None,
                    trophy_grade: None,
                    current_progress: 0,
                    max_progress: 0,
                    unlock_time: achievement.unlocktime,
                    display_name: achievement.name,
                    description: achievement.description,
                    icon: None,
                })
                .collect());
        }
    }
    let url = format!("https://steamcommunity.com/profiles/{steam_id}/stats/{game_id}/?xml=1");
    let xml = agent
        .get(&url)
        .call()
        .map_err(error)?
        .into_string()
        .map_err(error)?;
    let response = quick_xml::de::from_str::<CommunityStats>(&xml).map_err(error)?;
    if response.achievements.items.is_empty() {
        return Err("Steam profile returned no achievement data; it may be private".into());
    }
    Ok(response
        .achievements
        .items
        .into_iter()
        .map(|achievement| aw_core::AchievementObservation {
            source_id: source_id.into(),
            origin_source_id: None,
            game_id: game_id.into(),
            achievement_id: achievement.apiname,
            achieved: achievement.closed != 0,
            hidden: false,
            global_percent_hundredths: None,
            trophy_grade: None,
            current_progress: 0,
            max_progress: 0,
            unlock_time: achievement.unlock_timestamp,
            display_name: achievement.name,
            description: achievement.description,
            icon: achievement.icon_closed,
        })
        .collect())
}

fn configure_watcher(
    app: &AppHandle,
    state: &State<'_, AppState>,
    settings: &AppSettings,
) -> CommandResult<()> {
    let handle = app.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            return;
        }
        for path in event.paths {
            let state = handle.state::<AppState>();
            if process_path(&handle, &state, &path, false).is_ok() {
                let _ = dispatch_pending(&handle, &state);
                let _ = handle.emit("library-changed", ());
            }
        }
    })
    .map_err(error)?;
    for location in settings.source_locations.iter().filter(|location| {
        location.enabled && source_kind_enabled(settings, location.kind) && location.path.exists()
    }) {
        if let Err(watch_error) = watcher.watch(&location.path, RecursiveMode::Recursive) {
            notification_log(
                state,
                &format!(
                    "Could not monitor {}: {watch_error}",
                    location.path.display()
                ),
            );
        }
    }
    *state.watcher.lock().map_err(lock_error)? = Some(watcher);
    Ok(())
}

fn start_background_baseline_scan(app: AppHandle) {
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let settings = match state.store.lock() {
            Ok(store) => match store.load_settings() {
                Ok(settings) => settings,
                Err(message) => {
                    notification_log(&state, &format!("Background scan settings: {message}"));
                    return;
                }
            },
            Err(message) => {
                notification_log(&state, &format!("Background scan settings lock: {message}"));
                return;
            }
        };
        let locations: Vec<_> = settings
            .source_locations
            .iter()
            .filter(|location| location.enabled && source_kind_enabled(&settings, location.kind))
            .cloned()
            .collect();
        for path in source::discover_files(&locations) {
            if let Err(message) = process_path(&app, &state, &path, true) {
                notification_log(
                    &state,
                    &format!("Background scan skipped {}: {message}", path.display()),
                );
            }
        }
        let _ = dispatch_pending(&app, &state);
        let _ = app.emit("library-changed", ());
    });
}

fn start_steam_monitor(app: AppHandle) {
    std::thread::spawn(move || {
        let mut previous_app: Option<String> = None;
        let mut last_activity_tick = std::time::Instant::now();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let running = steam::running_app_id();
            let state = app.state::<AppState>();
            let settings = match state.store.lock() {
                Ok(store) => match store.load_settings() {
                    Ok(settings) => settings,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            let app_changed = running.as_deref() != previous_app.as_deref();
            if app_changed {
                if let Some(stopped) = previous_app.take()
                    && settings.notify_on_playtime
                    && settings.notification_enabled
                {
                    let event = playtime_notification(&state, &stopped, true);
                    if let Err(message) = deliver_transient(&app, &state, event) {
                        notification_log(
                            &state,
                            &format!("Playtime notification skipped: {message}"),
                        );
                    }
                }
                if let Some(started) = running.as_deref()
                    && settings.notify_on_playtime
                    && settings.notification_enabled
                {
                    let event = playtime_notification(&state, started, false);
                    if let Err(message) = deliver_transient(&app, &state, event) {
                        notification_log(
                            &state,
                            &format!("Playtime notification skipped: {message}"),
                        );
                    }
                }
                previous_app = running.clone();
                last_activity_tick = std::time::Instant::now();
            }
            let Some(game_id) = running.as_deref() else {
                continue;
            };
            if previous_app.as_deref() == Some(game_id) {
                let seconds = last_activity_tick.elapsed().as_secs() as i64;
                if seconds > 0
                    && !state
                        .launched_games
                        .lock()
                        .is_ok_and(|games| games.contains(game_id))
                {
                    if let Ok(store) = state.store.lock() {
                        let _ = store.record_play_session(game_id, seconds, Utc::now().timestamp());
                    }
                    last_activity_tick = std::time::Instant::now();
                }
            }
            if !settings.steam_enabled {
                continue;
            }
            let Some(location) = settings.source_locations.iter().find(|location| {
                location.enabled
                    && source_kind_enabled(&settings, location.kind)
                    && location.kind == aw_core::SourceKind::Steam
            }) else {
                continue;
            };
            let detected_account = steam::stats_files(location)
                .into_iter()
                .find_map(|path| steam::stats_file_identity(&path).map(|(account, _)| account));
            let account_id = settings
                .steam_account_id
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .or(detected_account.as_deref())
                .unwrap_or_default();
            let baseline = app_changed;
            match sync_steam_game(&app, &state, location, account_id, game_id, baseline) {
                Ok(()) => {
                    let _ = dispatch_pending(&app, &state);
                    let _ = app.emit("library-changed", ());
                }
                Err(message) => notification_log(&state, &format!("Steam monitor: {message}")),
            }
        }
    });
}

fn start_process_monitor(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_seen: BTreeMap<String, std::time::Instant> = BTreeMap::new();
        let mut emit_counter = 0_u8;
        loop {
            std::thread::sleep(Duration::from_secs(5));
            let state = app.state::<AppState>();
            let settings = match state.store.lock() {
                Ok(store) => match store.load_settings() {
                    Ok(settings) => settings,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            if settings.game_launch_configs.is_empty() {
                last_seen.clear();
                continue;
            }
            let running = process::running_names();
            let launched = state
                .launched_games
                .lock()
                .map(|games| games.clone())
                .unwrap_or_default();
            for (game_id, config) in &settings.game_launch_configs {
                let executable = config
                    .executable
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(str::to_lowercase);
                let is_running = executable
                    .as_ref()
                    .is_some_and(|name| running.contains(name));
                if is_running && !launched.contains(game_id) {
                    let now = std::time::Instant::now();
                    if let Some(previous) = last_seen.insert(game_id.clone(), now) {
                        let seconds = previous.elapsed().as_secs() as i64;
                        if seconds > 0
                            && let Ok(store) = state.store.lock()
                        {
                            let _ =
                                store.record_play_session(game_id, seconds, Utc::now().timestamp());
                        }
                    } else if settings.notify_on_playtime && settings.notification_enabled {
                        let event = playtime_notification(&state, game_id, false);
                        if let Err(message) = deliver_transient(&app, &state, event) {
                            notification_log(
                                &state,
                                &format!("Playtime notification skipped: {message}"),
                            );
                        }
                    }
                } else {
                    if last_seen.remove(game_id).is_some()
                        && settings.notify_on_playtime
                        && settings.notification_enabled
                    {
                        let event = playtime_notification(&state, game_id, true);
                        if let Err(message) = deliver_transient(&app, &state, event) {
                            notification_log(
                                &state,
                                &format!("Playtime notification skipped: {message}"),
                            );
                        }
                    }
                }
            }
            emit_counter = emit_counter.wrapping_add(1);
            if emit_counter >= 12 {
                emit_counter = 0;
                let _ = app.emit("library-changed", ());
            }
        }
    });
}

fn start_registry_monitor(app: AppHandle) {
    std::thread::spawn(move || {
        let mut first_poll = true;
        loop {
            std::thread::sleep(Duration::from_secs(3));
            let state = app.state::<AppState>();
            let settings = match state.store.lock() {
                Ok(store) => match store.load_settings() {
                    Ok(settings) => settings,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            if !settings.green_luma_enabled && !settings.luma_play_enabled {
                first_poll = true;
                continue;
            }
            match sync_registry_sources(&app, &state, &settings, first_poll) {
                Ok(count) => {
                    first_poll = false;
                    if count > 0 {
                        let _ = dispatch_pending(&app, &state);
                        let _ = app.emit("library-changed", ());
                    }
                }
                Err(message) => notification_log(&state, &format!("Registry monitor: {message}")),
            }
        }
    });
}

fn dispatch_pending(app: &AppHandle, state: &State<'_, AppState>) -> CommandResult<()> {
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    let events = state
        .store
        .lock()
        .map_err(lock_error)?
        .pending_events(Utc::now().timestamp(), 25)
        .map_err(error)?;
    for event in events {
        if settings.game_bar_enabled
            && (!settings.game_bar_fullscreen_only || foreground_is_fullscreen())
        {
            match state.game_bar.deliver(&settings.game_bar_token, &event) {
                Ok(()) => {
                    if event.id >= 0 {
                        state
                            .store
                            .lock()
                            .map_err(lock_error)?
                            .record_delivery(event.id, "game_bar", Ok(()))
                            .map_err(error)?;
                    }
                    continue;
                }
                Err(delivery_error) if event.id >= 0 => {
                    state
                        .store
                        .lock()
                        .map_err(lock_error)?
                        .record_delivery(event.id, "game_bar", Err(&delivery_error))
                        .map_err(error)?;
                }
                Err(_) => {}
            }
        }
        match settings.notification_mode {
            NotificationMode::NativeOnly => deliver_native(app, state, &event)?,
            NotificationMode::OverlayOnly | NotificationMode::OverlayWithNativeFallback => {
                if state.current_overlay.lock().map_err(lock_error)?.is_some() {
                    break;
                }
                show_overlay(app, state, event)?;
                break;
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
fn foreground_is_fullscreen() -> bool {
    use windows_sys::Win32::{
        Foundation::RECT,
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect, IsIconic},
    };
    unsafe {
        let window = GetForegroundWindow();
        if window.is_null() || IsIconic(window) != 0 {
            return false;
        }
        let mut window_rect: RECT = std::mem::zeroed();
        if GetWindowRect(window, &mut window_rect) == 0 {
            return false;
        }
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return false;
        }
        let tolerance = 2;
        window_rect.left <= info.rcMonitor.left + tolerance
            && window_rect.top <= info.rcMonitor.top + tolerance
            && window_rect.right >= info.rcMonitor.right - tolerance
            && window_rect.bottom >= info.rcMonitor.bottom - tolerance
    }
}

#[cfg(not(windows))]
fn foreground_is_fullscreen() -> bool {
    false
}

fn show_overlay(
    app: &AppHandle,
    state: &State<'_, AppState>,
    event: NotificationEvent,
) -> CommandResult<()> {
    notification_log(
        state,
        &format!("queueing custom window for event {}", event.id),
    );
    state
        .awaiting_overlay
        .lock()
        .map_err(lock_error)?
        .insert(event.id);
    *state.current_overlay.lock().map_err(lock_error)? = Some(event.clone());
    if app.get_webview_window("notification").is_none() {
        notification_log(state, "creating custom notification renderer");
        let settings = state
            .store
            .lock()
            .ok()
            .and_then(|store| store.load_settings().ok())
            .unwrap_or_default();
        let scale = settings.notification_scale_percent.clamp(50, 150) as f64 / 100.0;
        let (width, height) = notification_preset_size(&settings.notification_preset);
        WebviewWindowBuilder::new(app, "notification", WebviewUrl::App("index.html".into()))
            .title("Achievement unlocked")
            .inner_size(width * scale, height * scale)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .focused(false)
            .visible(false)
            .skip_taskbar(true)
            .resizable(false)
            .build()
            .map_err(error)?;
    } else {
        notification_log(state, "sending event to custom renderer");
        app.emit_to("notification", "notification-request", &event)
            .map_err(|emit_error| {
                notification_log(state, &format!("render event failed: {emit_error}"));
                error(emit_error)
            })?;
    }
    let handle = app.clone();
    let event_for_timeout = event.clone();
    tauri::async_runtime::spawn(async move {
        tokio_sleep(Duration::from_secs(5)).await;
        let state = handle.state::<AppState>();
        let waiting = state
            .awaiting_overlay
            .lock()
            .map(|mut waiting| waiting.remove(&event_for_timeout.id))
            .unwrap_or(false);
        if waiting {
            notification_log(&state, "renderer timed out; destroying hidden window");
            let settings = state
                .store
                .lock()
                .ok()
                .and_then(|store| store.load_settings().ok())
                .unwrap_or_default();
            if settings.notification_mode == NotificationMode::OverlayWithNativeFallback {
                notification_log(&state, "attempting Windows notification fallback");
                let _ = deliver_native(&handle, &state, &event_for_timeout);
            } else if event_for_timeout.id >= 0 {
                let _ = state.store.lock().map(|store| {
                    store.record_delivery(event_for_timeout.id, "overlay", Err("render timeout"))
                });
            }
            if let Ok(mut current) = state.current_overlay.lock() {
                *current = None;
            }
            if let Some(window) = handle.get_webview_window("notification") {
                let _ = window.destroy();
            }
            let _ = dispatch_pending(&handle, &state);
        }
    });
    let close_handle = app.clone();
    let close_id = event.id;
    let watchdog_timeout = state
        .store
        .lock()
        .ok()
        .and_then(|store| store.load_settings().ok())
        .map(|settings| {
            notification_preset_duration(&settings.notification_preset).saturating_mul(u64::from(
                settings.notification_duration_percent.clamp(10, 500),
            )) / 100
                + 4_000
        })
        .unwrap_or(12_000);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(watchdog_timeout));
        let app_for_close = close_handle.clone();
        let _ = close_handle.run_on_main_thread(move || {
            let state = app_for_close.state::<AppState>();
            let still_current = state
                .current_overlay
                .lock()
                .ok()
                .and_then(|current| current.as_ref().map(|item| item.id))
                == Some(close_id);
            if still_current {
                notification_log(&state, "watchdog forced custom window shutdown");
                if let Ok(mut waiting) = state.awaiting_overlay.lock() {
                    waiting.remove(&close_id);
                }
                if let Ok(mut current) = state.current_overlay.lock() {
                    *current = None;
                }
                if let Some(window) = app_for_close.get_webview_window("notification") {
                    let _ = window.destroy();
                }
                let settings = state
                    .store
                    .lock()
                    .ok()
                    .and_then(|store| store.load_settings().ok())
                    .unwrap_or_default();
                if settings.notification_mode == NotificationMode::OverlayWithNativeFallback {
                    notification_log(&state, "watchdog attempting Windows notification fallback");
                    let _ = deliver_native(&app_for_close, &state, &event);
                } else if event.id >= 0 {
                    let _ = state.store.lock().map(|store| {
                        store.record_delivery(event.id, "overlay", Err("window watchdog timeout"))
                    });
                }
                let _ = dispatch_pending(&app_for_close, &state);
            }
        });
    });
    Ok(())
}

fn notification_preset_size(preset: &str) -> (f64, f64) {
    match preset {
        "default" | "original" => (450.0, 150.0),
        "ps4" => (400.0, 200.0),
        "ps5" => (400.0, 150.0),
        "ps5_enhanced" => (450.0, 150.0),
        "xbox_one" => (600.0, 160.0),
        "xbox_360" => (600.0, 150.0),
        "raposo" | "smooth_pop" => (400.0, 150.0),
        "xqjan" => (450.0, 150.0),
        _ => (474.0, 128.0),
    }
}

fn notification_preset_duration(preset: &str) -> u64 {
    match preset {
        "default" | "original" | "raposo" => 6_000,
        "ps4" | "xbox_360" => 5_000,
        "smooth_pop" => 8_000,
        "xbox_one" | "xqjan" => 10_000,
        _ => 4_000,
    }
}

fn position_notification(window: &tauri::WebviewWindow, position: &str) {
    let Ok(size) = window.outer_size() else {
        return;
    };
    let Some((left, top, right_edge, bottom_edge)) = notification_monitor_area(window) else {
        return;
    };
    let right = right_edge.saturating_sub(size.width as i32 + 24);
    let bottom = bottom_edge.saturating_sub(size.height as i32 + 24);
    let center = left + (right_edge - left - size.width as i32) / 2;
    let (x, y) = match position {
        "top_center" => (center, top + 24),
        "bottom_center" => (center, bottom),
        "top_left" => (left + 24, top + 24),
        "top_right" => (right, top + 24),
        "bottom_left" => (left + 24, bottom),
        _ => (right, bottom),
    };
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

#[cfg(windows)]
fn notification_monitor_area(_window: &tauri::WebviewWindow) -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::{
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow,
        },
        UI::WindowsAndMessaging::GetForegroundWindow,
    };
    unsafe {
        let monitor = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY);
        if monitor.is_null() {
            return None;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        Some((
            info.rcWork.left,
            info.rcWork.top,
            info.rcWork.right,
            info.rcWork.bottom,
        ))
    }
}

#[cfg(not(windows))]
fn notification_monitor_area(window: &tauri::WebviewWindow) -> Option<(i32, i32, i32, i32)> {
    let monitor = window.current_monitor().ok().flatten()?;
    let size = monitor.size();
    let position = monitor.position();
    Some((
        position.x,
        position.y,
        position.x + size.width as i32,
        position.y + size.height as i32,
    ))
}

async fn tokio_sleep(duration: Duration) {
    tauri::async_runtime::spawn_blocking(move || std::thread::sleep(duration))
        .await
        .ok();
}

fn deliver_native(
    app: &AppHandle,
    state: &State<'_, AppState>,
    event: &NotificationEvent,
) -> CommandResult<()> {
    notification_log(state, &format!("sending native event {}", event.id));
    let title = event
        .observation
        .display_name
        .as_deref()
        .unwrap_or(&event.observation.achievement_id);
    let settings = state
        .store
        .lock()
        .map_err(lock_error)?
        .load_settings()
        .map_err(error)?;
    let fallback = if event.event_key.starts_with("playtime") {
        "Playtime tracking"
    } else if event.kind == aw_core::NotificationKind::Progress {
        "Achievement progress updated"
    } else {
        "Achievement unlocked"
    };
    let body = if settings.notification_show_description {
        event.observation.description.as_deref().unwrap_or(fallback)
    } else {
        fallback
    };
    let result = app.notification().builder().title(title).body(body).show();
    if event.id >= 0 {
        let store = state.store.lock().map_err(lock_error)?;
        match &result {
            Ok(()) => store
                .record_delivery(event.id, "native", Ok(()))
                .map_err(error)?,
            Err(delivery_error) => {
                let message = delivery_error.to_string();
                store
                    .record_delivery(event.id, "native", Err(&message))
                    .map_err(error)?;
            }
        }
    }
    result.map_err(error)
}

fn notification_log(state: &AppState, message: &str) {
    let path = state.data_dir.join("notification.log");
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "{} {message}", Utc::now().to_rfc3339());
}

#[cfg(windows)]
fn capture_primary_display(
    data_dir: &Path,
    game: &str,
    achievement: &str,
    overwrite: bool,
) -> CommandResult<PathBuf> {
    let monitor = xcap::Monitor::all()
        .map_err(error)?
        .into_iter()
        .find(|monitor| monitor.is_primary().unwrap_or(false))
        .ok_or("Primary monitor not found")?;
    let image = monitor.capture_image().map_err(error)?;
    let directory = data_dir.join(sanitize(game));
    std::fs::create_dir_all(&directory).map_err(error)?;
    let base = sanitize(achievement);
    let mut path = directory.join(format!("{base}.png"));
    if !overwrite && path.exists() {
        path = directory.join(format!("{base}-{}.png", Utc::now().format("%Y%m%d-%H%M%S")));
    }
    let temporary = path.with_extension("png.tmp");
    image.save(&temporary).map_err(error)?;
    std::fs::rename(&temporary, &path).map_err(error)?;
    Ok(path)
}

#[cfg(not(windows))]
fn capture_primary_display(
    _data_dir: &Path,
    _game: &str,
    _achievement: &str,
    _overwrite: bool,
) -> CommandResult<PathBuf> {
    Err("Screenshots are available only in packaged Windows builds".into())
}

fn sanitize(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|character| {
            if "<>:\"/\\|?*".contains(character) || character.is_control() {
                '_'
            } else {
                character
            }
        })
        .collect();
    let value = value.trim_matches([' ', '.']);
    if value.is_empty() {
        "achievement".into()
    } else {
        value.chars().take(120).collect()
    }
}

fn default_legacy_root() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Achievement Watcher")
}

fn lock_error<T>(error: std::sync::PoisonError<T>) -> String {
    format!("Application state lock failed: {error}")
}
fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        let state = app.state::<AppState>();
                        if let Err(message) = toggle_achievement_overlay_inner(app, &state) {
                            notification_log(
                                &state,
                                &format!("Achievement overlay shortcut: {message}"),
                            );
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let store = Store::open(data_dir.join("achievement-watcher.sqlite3"))?;
            app.manage(AppState {
                store: Mutex::new(store),
                watcher: Mutex::new(None),
                awaiting_overlay: Mutex::new(HashSet::new()),
                current_overlay: Mutex::new(None),
                launched_games: Mutex::new(HashSet::new()),
                game_bar: game_bar::GameBarBridge::start(),
                websocket: websocket::Bridge::default(),
                data_dir,
            });
            let startup_settings = app
                .state::<AppState>()
                .store
                .lock()
                .map_err(|error| error.to_string())?
                .load_settings()?;
            let state = app.state::<AppState>();
            if !cfg!(dev)
                && let Err(message) = registry::configure_startup(startup_settings.run_at_login)
            {
                notification_log(&state, &format!("Startup registration: {message}"));
            }
            if let Err(message) = state.websocket.configure(
                startup_settings.websocket_enabled,
                &startup_settings.websocket_host,
                startup_settings.websocket_port,
            ) {
                notification_log(&state, &format!("WebSocket startup: {message}"));
            }
            if let Err(message) = configure_overlay_shortcut(app.handle(), &startup_settings) {
                notification_log(
                    &state,
                    &format!("Achievement overlay shortcut startup: {message}"),
                );
            }
            if let Err(message) = configure_watcher(app.handle(), &state, &startup_settings) {
                notification_log(&state, &format!("Source watcher startup: {message}"));
            }
            if let Err(message) = dispatch_pending(app.handle(), &state) {
                notification_log(&state, &format!("Pending notification startup: {message}"));
            }
            let force_main_window = std::env::args_os().any(|argument| argument == "--show");
            let start_hidden = !cfg!(dev) && !force_main_window && startup_settings.start_minimized;
            if start_hidden && let Some(window) = app.get_webview_window("main") {
                window.destroy()?;
            } else if force_main_window || cfg!(dev) {
                show_main_window(app.handle()).map_err(std::io::Error::other)?;
            }
            if start_hidden {
                start_background_baseline_scan(app.handle().clone());
            }
            start_steam_monitor(app.handle().clone());
            start_process_monitor(app.handle().clone());
            start_registry_monitor(app.handle().clone());
            notification_log(
                &app.state::<AppState>(),
                concat!("application started, version ", env!("CARGO_PKG_VERSION")),
            );
            let open =
                MenuItem::with_id(app, "open", "Open Achievement Watcher", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let tray_icon = app
                .default_window_icon()
                .cloned()
                .ok_or("Packaged application icon is unavailable")?;
            TrayIconBuilder::new()
                .icon(tray_icon)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        let _ = show_main_window(app);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(event, tauri::tray::TrayIconEvent::DoubleClick { .. }) {
                        let _ = show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            read_profile_avatar,
            read_notification_audio,
            import_steam_avatar,
            open_windows_settings,
            check_for_updates,
            install_update,
            open_release_page,
            open_game_website,
            open_project_page,
            export_goldberg_achievements,
            open_data_location,
            diagnostics,
            save_settings,
            import_legacy,
            list_games,
            detect_sources,
            steam_accounts,
            current_overlay_game_id,
            toggle_achievement_overlay,
            close_achievement_overlay,
            launch_game,
            list_achievements,
            game_sources,
            scan_sources,
            refresh_metadata,
            clear_game_metadata,
            reset_game_activity,
            test_notification,
            test_progress_notification,
            test_playtime_notification,
            test_game_bar,
            test_gntp,
            test_obs,
            acknowledge_notification,
            current_notification,
            close_notification,
            open_notification_game,
            report_notification_error,
            capture_screenshot,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Achievement Watcher")
        .run(|app, event| {
            if let tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } = event
                && label == "main"
            {
                let close_to_tray = app
                    .state::<AppState>()
                    .store
                    .lock()
                    .ok()
                    .and_then(|store| store.load_settings().ok())
                    .is_none_or(|settings| settings.close_to_tray);
                api.prevent_close();
                if close_to_tray {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.destroy();
                    }
                } else {
                    app.exit(0);
                }
            }
        });
}

fn show_main_window(app: &AppHandle) -> CommandResult<()> {
    let window = match app.get_webview_window("main") {
        Some(window) => window,
        None => WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title("Achievement Watcher")
            .inner_size(1200.0, 800.0)
            .min_inner_size(900.0, 600.0)
            .decorations(false)
            .build()
            .map_err(error)?,
    };
    window.show().map_err(error)?;
    window.set_focus().map_err(error)
}

#[cfg(test)]
mod tests {
    use super::{emulator_config_value, source_uses_steam_metadata};
    use aw_core::SourceKind;

    #[test]
    fn reads_redirected_emulator_save_configuration() {
        let content = "[Settings]\nAppID = 504230 # Celeste\nPlayerName='green'\nSaveType=1\n";
        assert_eq!(
            emulator_config_value(content, &["appid", "app_id"]).as_deref(),
            Some("504230")
        );
        assert_eq!(
            emulator_config_value(content, &["playername"]).as_deref(),
            Some("green")
        );
    }

    #[test]
    fn steam_artwork_is_not_assumed_for_other_numeric_catalogs() {
        assert!(source_uses_steam_metadata(SourceKind::Steam));
        assert!(source_uses_steam_metadata(SourceKind::SteamEmulator));
        assert!(!source_uses_steam_metadata(SourceKind::Gog));
        assert!(!source_uses_steam_metadata(SourceKind::Epic));
    }
}
