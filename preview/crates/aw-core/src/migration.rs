use crate::{
    AppSettings, MigrationReport, Result, SourceKind, SourceLocation, Store, parser::parse_json,
};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::Path,
    time::UNIX_EPOCH,
};

pub fn import_legacy(store: &mut Store, legacy_root: &Path) -> Result<MigrationReport> {
    // Keep the import idempotent while allowing newer builds to replay expanded
    // migrations against databases that recorded an older, less complete pass.
    // Bump this suffix whenever the set or meaning of imported legacy data changes.
    let key = format!("{}#parity-v2", legacy_root.to_string_lossy());
    if let Some(report) = store.migration_report(&key)? {
        import_game_metadata_if_changed(store, legacy_root)?;
        return Ok(report);
    }

    let mut report = MigrationReport::default();
    let mut settings = store.load_settings()?;
    let options = legacy_root.join("cfg/options.ini");
    if let Ok(content) = fs::read_to_string(&options) {
        apply_legacy_settings(&content, &mut settings);
        report.imported_settings = true;
    }

    let user_dirs = legacy_root.join("cfg/userdir.db");
    if let Ok(content) = fs::read_to_string(user_dirs) {
        match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(serde_json::Value::Array(entries)) => {
                for (index, entry) in entries.into_iter().enumerate() {
                    let Some(path) = entry.get("path").and_then(|value| value.as_str()) else {
                        continue;
                    };
                    settings.source_locations.push(SourceLocation {
                        id: format!("legacy-{index}"),
                        kind: SourceKind::SteamEmulator,
                        path: path.into(),
                        enabled: true,
                        notify: entry
                            .get("notify")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(true),
                    });
                    report.imported_sources += 1;
                }
            }
            Ok(_) => report
                .warnings
                .push("Legacy userdir.db was not an array".into()),
            Err(error) => report
                .warnings
                .push(format!("Could not parse legacy userdir.db: {error}")),
        }
    }
    if let Ok(content) = fs::read_to_string(legacy_root.join("cfg/exeList.db"))
        && let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(&content)
    {
        for entry in entries {
            let Some(game_id) = entry.get("appid").and_then(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| value.as_i64().map(|id| id.to_string()))
            }) else {
                continue;
            };
            let Some(executable) = entry
                .get("exe")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            settings.game_launch_configs.insert(
                game_id,
                crate::GameLaunchConfig {
                    executable: executable.into(),
                    arguments: entry
                        .get("args")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .into(),
                },
            );
        }
    }
    settings
        .source_locations
        .sort_by(|a, b| a.path.cmp(&b.path));
    settings.source_locations.dedup_by(|a, b| a.path == b.path);
    store.save_settings(&settings)?;

    if let Ok(content) = fs::read_to_string(legacy_root.join("cfg/exclusion.db"))
        && let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(&content)
    {
        for item in items {
            let id = item
                .as_str()
                .map(str::to_owned)
                .or_else(|| item.as_i64().map(|id| id.to_string()));
            if let Some(id) = id.filter(|id| !settings.blacklisted_game_ids.contains(id)) {
                settings.blacklisted_game_ids.push(id);
                report.imported_blacklist_entries += 1;
            }
        }
        store.save_settings(&settings)?;
    }

    let data_dir = legacy_root.join("steam_cache/data");
    if let Ok(entries) = fs::read_dir(data_dir) {
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("db") {
                continue;
            }
            let Some(game_id) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            match fs::read_to_string(&path)
                .ok()
                .and_then(|content| parse_json(&content, "watchdog-cache", game_id).ok())
            {
                Some(observations) => {
                    report.imported_observations += observations.len();
                    store.record_observations(&observations, true)?;
                }
                None => report
                    .warnings
                    .push(format!("Skipped unreadable cache {}", path.display())),
            }
        }
    }

    store.save_migration_report(&key, &report)?;
    import_game_metadata_if_changed(store, legacy_root)?;
    Ok(report)
}

fn import_game_metadata_if_changed(store: &Store, legacy_root: &Path) -> Result<()> {
    let key = legacy_metadata_key(legacy_root)?;
    if store.migration_report(&key)?.is_some() {
        return Ok(());
    }
    import_game_metadata(store, legacy_root)?;
    store.save_migration_report(&key, &MigrationReport::default())
}

fn legacy_metadata_key(legacy_root: &Path) -> Result<String> {
    let schema_root = legacy_root.join("steam_cache/schema");
    let mut files = Vec::new();
    match fs::read_dir(&schema_root) {
        Ok(languages) => {
            for language in languages {
                let language = language?;
                if !language.file_type()?.is_dir() {
                    continue;
                }
                for entry in fs::read_dir(language.path())? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.extension().and_then(|value| value.to_str()) == Some("db") {
                        files.push(path);
                    }
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    files.sort();
    let mut signature = DefaultHasher::new();
    for path in files {
        path.hash(&mut signature);
        if let Ok(metadata) = path.metadata() {
            metadata.len().hash(&mut signature);
            metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos())
                .hash(&mut signature);
        }
    }
    Ok(format!(
        "{}#metadata-v1-{:016x}",
        legacy_root.to_string_lossy(),
        signature.finish()
    ))
}

fn import_game_metadata(store: &Store, legacy_root: &Path) -> Result<()> {
    let schema_root = legacy_root.join("steam_cache/schema");
    let Ok(languages) = fs::read_dir(schema_root) else {
        return Ok(());
    };
    for language in languages.filter_map(|entry| entry.ok()) {
        let Ok(files) = fs::read_dir(language.path()) else {
            continue;
        };
        for file in files.filter_map(|entry| entry.ok()) {
            let path = file.path();
            if path.extension().and_then(|value| value.to_str()) != Some("db") {
                continue;
            }
            let Some(game_id) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let Some(object) = fs::read_to_string(&path)
                .ok()
                .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
                .and_then(|value| value.as_object().cloned())
            else {
                continue;
            };
            let Some(name) = object.get("name").and_then(|value| value.as_str()) else {
                continue;
            };
            let icon = object
                .get("img")
                .and_then(|value| value.get("header"))
                .and_then(|value| value.as_str())
                .or_else(|| {
                    object
                        .get("img")
                        .and_then(|value| value.get("icon"))
                        .and_then(|value| value.as_str())
                });
            let icon = icon.map(|value| steam_image_url(game_id, value, true));
            store.save_game_metadata(game_id, name, icon.as_deref())?;
            let achievements = object
                .get("achievement")
                .and_then(|value| value.get("list"))
                .or_else(|| object.get("achievements"))
                .and_then(|value| value.as_array());
            if let Some(achievements) = achievements {
                for achievement in achievements {
                    let Some(item) = achievement.as_object() else {
                        continue;
                    };
                    let id = ["apiname", "id", "name"]
                        .iter()
                        .find_map(|key| item.get(*key).and_then(|value| value.as_str()));
                    let Some(id) = id else { continue };
                    let display_name = item
                        .get("displayName")
                        .or_else(|| item.get("display_name"))
                        .and_then(|value| value.as_str());
                    let description = item.get("description").and_then(|value| value.as_str());
                    let icon = item
                        .get("icon")
                        .or_else(|| item.get("iconUnlocked"))
                        .and_then(|value| value.as_str());
                    let icon = icon.map(|value| steam_image_url(game_id, value, false));
                    let locked_icon = item
                        .get("icongray")
                        .or_else(|| item.get("iconGray"))
                        .or_else(|| item.get("iconLocked"))
                        .and_then(|value| value.as_str())
                        .map(|value| steam_image_url(game_id, value, false));
                    store.save_achievement_metadata(
                        game_id,
                        id,
                        display_name,
                        description,
                        icon.as_deref(),
                        locked_icon.as_deref(),
                        item.get("hidden")
                            .and_then(|value| {
                                value
                                    .as_bool()
                                    .or_else(|| value.as_i64().map(|value| value != 0))
                            })
                            .unwrap_or(false),
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn steam_image_url(game_id: &str, value: &str, header: bool) -> String {
    if value.starts_with("http://") || value.starts_with("https://") {
        return value.to_string();
    }
    if header || value.eq_ignore_ascii_case("header.jpg") {
        format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{game_id}/header.jpg")
    } else {
        let hash = value.trim_end_matches(".jpg");
        format!(
            "https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/{game_id}/{hash}.jpg"
        )
    }
}

fn apply_legacy_settings(content: &str, settings: &mut AppSettings) {
    let mut section = "";
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match (section, key.trim()) {
            ("general", "username") => {
                settings.username = value.trim_matches('"').to_string();
                settings.username_customized = Some(true);
            }
            ("general", "skippedVersion")
                if !value.trim_matches('"').eq_ignore_ascii_case("none") =>
            {
                settings.skipped_update_version = Some(value.trim_matches('"').to_string())
            }
            ("achievement", "lang") => settings.language = value.trim_matches('"').to_string(),
            ("achievement", "thumbnailPortrait") => settings.thumbnail_portrait = parse_bool(value),
            ("achievement", "showHidden") => settings.show_hidden = parse_bool(value),
            ("achievement", "mergeDuplicate") => settings.merge_duplicate = parse_bool(value),
            ("achievement", "timeMergeRecentFirst") => {
                settings.time_merge_recent_first = parse_bool(value)
            }
            ("achievement", "hideZero") => settings.hide_zero = parse_bool(value),
            ("achievement_source", "steamEmu") => {
                settings.steam_emulator_enabled = parse_bool(value)
            }
            ("achievement_source", "greenLuma") => settings.green_luma_enabled = parse_bool(value),
            ("achievement_source", "rpcs3") => settings.rpcs3_enabled = parse_bool(value),
            ("achievement_source", "lumaPlay") => settings.luma_play_enabled = parse_bool(value),
            ("achievement_source", "gog") => settings.gog_enabled = parse_bool(value),
            ("achievement_source", "epic") => settings.epic_enabled = parse_bool(value),
            ("achievement_source", "importCache") => {
                settings.watchdog_cache_enabled = parse_bool(value)
            }
            ("achievement_source", "legitSteam") => match value.trim_matches('"') {
                "1" => {
                    settings.steam_enabled = true;
                    settings.steam_library_mode = "installed".into();
                }
                "2" => {
                    settings.steam_enabled = true;
                    settings.steam_library_mode = "owned".into();
                }
                _ => settings.steam_enabled = false,
            },
            ("steam", "apiKey") if !value.trim_matches('"').is_empty() => {
                let value = value.trim_matches('"');
                settings.steam_api_key =
                    decrypt_legacy_api_key(value).unwrap_or_else(|| value.to_string());
            }
            ("notification", "notify") => settings.notification_enabled = parse_bool(value),
            ("notification", "rumble") => settings.rumble_enabled = parse_bool(value),
            ("notification", "notifyOnProgress") => settings.notify_on_progress = parse_bool(value),
            ("notification", "playtime") => settings.notify_on_playtime = parse_bool(value),
            ("notification_toast", "customToastAudio") => {
                settings.notification_sound = match value.trim_matches('"') {
                    "0" => "none",
                    "1" => "windows",
                    // The original custom path lived outside options.ini. Keep
                    // notifications audible until the user chooses it again.
                    "2" => "windows",
                    _ => "steam_deck",
                }
                .into()
            }
            ("notification_transport", "websocket") => {
                settings.websocket_enabled = parse_bool(value)
            }
            ("notification_transport", "gntp") => settings.gntp_enabled = parse_bool(value),
            ("notification_advanced", "timeTreshold") => {
                if let Ok(seconds) = value.parse::<u32>() {
                    settings.notification_max_age_seconds = seconds;
                }
            }
            ("notification_advanced", "checkIfProcessIsRunning") => {
                settings.notification_require_running_game = parse_bool(value)
            }
            ("souvenir_screenshot", "screenshot") => {
                settings.screenshot_enabled = value.eq_ignore_ascii_case("true")
            }
            ("souvenir_screenshot", "overwrite_image") => {
                settings.screenshot_overwrite = parse_bool(value)
            }
            ("souvenir_screenshot", "custom_dir") if !value.trim_matches('"').is_empty() => {
                settings.screenshot_directory = Some(value.trim_matches('"').into())
            }
            ("souvenir_video", "video") => {
                settings.obs_replay_enabled = value.trim_matches('"') != "0"
            }
            ("action", "target") if !value.trim_matches('"').is_empty() => {
                settings.custom_action_enabled = true;
                settings.custom_action_executable = value.trim_matches('"').into();
            }
            ("action", "cwd") if !value.trim_matches('"').is_empty() => {
                settings.custom_action_working_directory = Some(value.trim_matches('"').into())
            }
            ("action", "hide") => settings.custom_action_hide_window = parse_bool(value),
            ("overlay", "duration") => {
                if let Ok(scale) = value.parse::<f64>() {
                    settings.notification_duration_percent = scale.clamp(10.0, 500.0) as u16;
                }
            }
            ("overlay", "scale") => {
                if let Ok(scale) = value.parse::<u16>() {
                    settings.notification_scale_percent = scale.clamp(50, 150);
                    settings.achievement_overlay_scale_percent = scale.clamp(50, 200);
                }
            }
            ("overlay", "hotkey") if !value.trim_matches('"').is_empty() => {
                settings.achievement_overlay_hotkey = value.trim_matches('"').to_string()
            }
            ("overlay", "position") => {
                settings.notification_position = match value.trim_matches('"') {
                    "center-top" | "top-center" => "top_center",
                    "center-bot" | "center-bottom" | "bottom-center" => "bottom_center",
                    "left-top" | "top-left" => "top_left",
                    "left-bottom" | "bottom-left" => "bottom_left",
                    "right-bottom" | "bottom-right" => "bottom_right",
                    _ => "top_right",
                }
                .into()
            }
            ("overlay", "preset") => {
                settings.notification_preset = match value
                    .trim_matches('"')
                    .to_ascii_lowercase()
                    .replace([' ', '-'], "_")
                    .as_str()
                {
                    "smoothpop" | "smooth_pop" => "smooth_pop",
                    "xboxone" | "xbox_one" => "xbox_one",
                    "xbox360" | "xbox_360" => "xbox_360",
                    "ps5enhanced" | "ps5_enhanced" => "ps5_enhanced",
                    "ps4" => "ps4",
                    "ps5" => "ps5",
                    "raposo" => "raposo",
                    "xqjan" => "xqjan",
                    "steam" => "steam",
                    _ => "default",
                }
                .into()
            }
            _ => {}
        }
    }
    if let (Some(overlay), Some(native)) = (
        legacy_bool(content, "notification_transport", "chromium"),
        legacy_bool(content, "notification_transport", "toast"),
    ) {
        settings.notification_mode = match (overlay, native) {
            (true, true) => crate::NotificationMode::OverlayWithNativeFallback,
            (true, false) => crate::NotificationMode::OverlayOnly,
            (false, true) => crate::NotificationMode::NativeOnly,
            (false, false) => {
                settings.notification_enabled = false;
                crate::NotificationMode::OverlayOnly
            }
        };
    }
}

fn legacy_bool(content: &str, requested_section: &str, requested_key: &str) -> Option<bool> {
    let mut section = "";
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
        } else if section == requested_section
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == requested_key
        {
            return Some(parse_bool(value));
        }
    }
    None
}

fn decrypt_legacy_api_key(value: &str) -> Option<String> {
    use aes::Aes256;
    use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};

    let (iv, ciphertext) = value.split_once(':')?;
    let iv = hex::decode(iv).ok()?;
    let ciphertext = hex::decode(ciphertext).ok()?;
    let plaintext =
        cbc::Decryptor::<Aes256>::new_from_slices(b"xfW!+Bn3E@Luu#^vj3$7wZRqRgACQeCu", &iv)
            .ok()?
            .decrypt_padded_vec_mut::<Pkcs7>(&ciphertext)
            .ok()?;
    String::from_utf8(plaintext).ok()
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().trim_matches('"').to_ascii_lowercase().as_str(),
        "true" | "1" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn decrypts_original_aes_api_keys_for_dpapi_migration() {
        let encrypted = concat!(
            "00000000000000000000000000000000:",
            "add81aa524cfee4110219e204dac6093fb361576ded039032d5c51c5004dd7305",
            "b1e4d009d02e2a71593aee1e439a261"
        );
        assert_eq!(
            decrypt_legacy_api_key(encrypted).as_deref(),
            Some("0123456789abcdef0123456789abcdef")
        );
    }

    #[test]
    fn migration_is_read_only_and_idempotent() {
        let legacy = tempdir().unwrap();
        fs::create_dir_all(legacy.path().join("cfg")).unwrap();
        let options = legacy.path().join("cfg/options.ini");
        fs::write(&options, "[general]\nusername=Green\n[achievement]\nmergeDuplicate=false\nhideZero=true\n[achievement_source]\nlegitSteam=2\n[steam]\napiKey=00000000000000000000000000000000:add81aa524cfee4110219e204dac6093fb361576ded039032d5c51c5004dd7305b1e4d009d02e2a71593aee1e439a261\n[notification]\nrumble=false\n[notification_toast]\ncustomToastAudio=0\n[notification_transport]\nchromium=false\ntoast=true\nwebsocket=true\ngntp=false\n[overlay]\nposition=center-bot\npreset=PS5enhanced\n[souvenir_screenshot]\nscreenshot=false\n[action]\ntarget=C:\\\\Tools\\\\unlock.exe\ncwd=C:\\\\Tools\nhide=false").unwrap();
        fs::write(legacy.path().join("cfg/exclusion.db"), "[480,\"1234\"]").unwrap();
        let before = fs::read(&options).unwrap();
        let mut store = Store::open_memory().unwrap();
        let first = import_legacy(&mut store, legacy.path()).unwrap();
        let second = import_legacy(&mut store, legacy.path()).unwrap();
        assert!(first.imported_settings);
        assert_eq!(first, second);
        assert_eq!(before, fs::read(&options).unwrap());
        assert!(!store.load_settings().unwrap().screenshot_enabled);
        let settings = store.load_settings().unwrap();
        assert_eq!(settings.username, "Green");
        assert!(!settings.merge_duplicate);
        assert!(settings.hide_zero);
        assert!(settings.steam_enabled);
        assert_eq!(settings.steam_library_mode, "owned");
        assert!(!settings.rumble_enabled);
        assert_eq!(settings.notification_position, "bottom_center");
        assert_eq!(settings.notification_preset, "ps5_enhanced");
        assert_eq!(
            settings.notification_mode,
            crate::NotificationMode::NativeOnly
        );
        assert_eq!(settings.notification_sound, "none");
        assert!(settings.websocket_enabled);
        assert!(!settings.gntp_enabled);
        assert_eq!(settings.steam_api_key, "0123456789abcdef0123456789abcdef");
        assert!(settings.custom_action_enabled);
        assert_eq!(
            settings.custom_action_executable,
            PathBuf::from(r"C:\\Tools\\unlock.exe")
        );
        assert_eq!(
            settings.custom_action_working_directory,
            Some(PathBuf::from(r"C:\\Tools"))
        );
        assert!(!settings.custom_action_hide_window);
        assert_eq!(settings.blacklisted_game_ids, ["480", "1234"]);
    }

    #[test]
    fn imports_cached_game_names_even_after_the_main_migration_ran() {
        let legacy = tempdir().unwrap();
        let schema = legacy.path().join("steam_cache/schema/english");
        fs::create_dir_all(&schema).unwrap();
        let mut store = Store::open_memory().unwrap();

        import_legacy(&mut store, legacy.path()).unwrap();
        fs::write(
            schema.join("400.db"),
            r#"{"name":"Portal","img":{"header":"https://example.test/portal.jpg"},"achievement":{"list":[{"apiname":"ESCAPE_00","displayName":"Lab Rat","description":"Acquire the portal device.","icon":"https://example.test/lab-rat.jpg"}]}}"#,
        )
        .unwrap();
        import_legacy(&mut store, legacy.path()).unwrap();

        assert_eq!(
            store.game_metadata("400").unwrap(),
            Some((
                "Portal".into(),
                Some("https://example.test/portal.jpg".into())
            ))
        );
        let mut observations = vec![crate::AchievementObservation {
            source_id: "test".into(),
            origin_source_id: None,
            game_id: "400".into(),
            achievement_id: "escape_00".into(),
            achieved: true,
            hidden: false,
            global_percent_hundredths: None,
            trophy_grade: None,
            current_progress: 0,
            max_progress: 0,
            unlock_time: 1,
            display_name: None,
            description: None,
            icon: None,
        }];
        store.enrich_observations(&mut observations).unwrap();
        assert_eq!(observations[0].display_name.as_deref(), Some("Lab Rat"));

        store
            .save_game_metadata("400", "Locally corrected title", None)
            .unwrap();
        import_legacy(&mut store, legacy.path()).unwrap();
        assert_eq!(
            store.game_metadata("400").unwrap().unwrap().0,
            "Locally corrected title"
        );
    }

    #[test]
    fn reruns_after_an_older_migration_revision() {
        let legacy = tempdir().unwrap();
        fs::create_dir_all(legacy.path().join("cfg")).unwrap();
        fs::write(
            legacy.path().join("cfg/options.ini"),
            "[general]\nusername=Imported User",
        )
        .unwrap();
        let mut store = Store::open_memory().unwrap();
        store
            .save_migration_report(
                &legacy.path().to_string_lossy(),
                &MigrationReport::default(),
            )
            .unwrap();

        let report = import_legacy(&mut store, legacy.path()).unwrap();

        assert!(report.imported_settings);
        assert_eq!(store.load_settings().unwrap().username, "Imported User");
    }
}
