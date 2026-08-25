use aw_core::SourceLocation;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamAccount {
    pub account_id: String,
    pub steam_id: String,
    pub name: String,
    pub most_recent: bool,
    pub local_user_match: bool,
    pub avatar_path: Option<PathBuf>,
}

pub fn accounts(locations: &[SourceLocation]) -> Vec<SteamAccount> {
    let mut accounts = Vec::new();
    let local_user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    for location in locations
        .iter()
        .filter(|location| location.kind == aw_core::SourceKind::Steam)
    {
        let Some(root) = location.path.parent().and_then(Path::parent) else {
            continue;
        };
        let Ok(content) = std::fs::read_to_string(root.join("config/loginusers.vdf")) else {
            continue;
        };
        let mut current: Option<SteamAccount> = None;
        for line in content.lines() {
            let values = quoted_values(line);
            if values.len() == 1
                && values[0].len() == 17
                && values[0].chars().all(|c| c.is_ascii_digit())
            {
                if let Some(account) = current.take() {
                    accounts.push(account);
                }
                let steam_id = values[0].clone();
                let account_id = steam_id
                    .parse::<u64>()
                    .ok()
                    .and_then(|id| id.checked_sub(76_561_197_960_265_728))
                    .unwrap_or_default()
                    .to_string();
                current = Some(SteamAccount {
                    account_id,
                    avatar_path: ["png", "jpg", "jpeg"]
                        .into_iter()
                        .map(|extension| {
                            root.join("config/avatarcache")
                                .join(format!("{steam_id}.{extension}"))
                        })
                        .find(|path| path.is_file()),
                    steam_id,
                    name: String::new(),
                    most_recent: false,
                    local_user_match: false,
                });
            } else if values.len() >= 2
                && let Some(account) = current.as_mut()
            {
                match values[0].as_str() {
                    "PersonaName" => {
                        account.name = values[1].clone();
                        account.local_user_match |= values[1].eq_ignore_ascii_case(&local_user);
                    }
                    "AccountName" => {
                        account.local_user_match |= values[1].eq_ignore_ascii_case(&local_user);
                        if account.name.is_empty() {
                            account.name = values[1].clone();
                        }
                    }
                    "MostRecent" => account.most_recent = values[1] == "1",
                    _ => {}
                }
            }
        }
        if let Some(account) = current {
            accounts.push(account);
        }
    }
    accounts.sort_by_key(|account| (!account.most_recent, !account.local_user_match));
    accounts.dedup_by(|left, right| left.steam_id == right.steam_id);
    accounts
}

fn quoted_values(line: &str) -> Vec<String> {
    line.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

pub fn stats_file_identity(path: &Path) -> Option<(String, String)> {
    let stem = path.file_stem()?.to_str()?;
    let parts: Vec<_> = stem.split('_').collect();
    if parts.len() != 3 || !parts[0].eq_ignore_ascii_case("UserGameStats") {
        return None;
    }
    (parts[1].chars().all(|c| c.is_ascii_digit()) && parts[2].chars().all(|c| c.is_ascii_digit()))
        .then(|| (parts[1].to_owned(), parts[2].to_owned()))
}

pub fn stats_files(location: &SourceLocation) -> Vec<PathBuf> {
    std::fs::read_dir(&location.path)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| stats_file_identity(path).is_some())
        .collect()
}

pub fn installed_games(location: &SourceLocation) -> Vec<(String, String)> {
    let mut games = Vec::new();
    for directory in steamapps_directories(location) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(id) = name
                .strip_prefix("appmanifest_")
                .and_then(|name| name.strip_suffix(".acf"))
            else {
                continue;
            };
            if id.chars().all(|character| character.is_ascii_digit()) {
                let name = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|content| {
                        content.lines().find_map(|line| {
                            let values = quoted_values(line);
                            (values.len() >= 2 && values[0].eq_ignore_ascii_case("name"))
                                .then(|| values[1].clone())
                        })
                    })
                    .unwrap_or_else(|| id.to_owned());
                if !is_steam_runtime(&name) {
                    games.push((id.to_owned(), name));
                }
            }
        }
    }
    games.sort_by(|left, right| left.0.cmp(&right.0));
    games.dedup_by(|left, right| left.0 == right.0);
    games
}

pub fn is_steam_runtime(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    name.starts_with("proton ")
        || name.starts_with("steam linux runtime")
        || name == "steamworks common redistributables"
}

pub fn steamapps_directories(location: &SourceLocation) -> Vec<PathBuf> {
    let Some(steam_root) = location.path.parent().and_then(Path::parent) else {
        return Vec::new();
    };
    let mut steamapps = vec![steam_root.join("steamapps")];
    if let Ok(content) = std::fs::read_to_string(steam_root.join("steamapps/libraryfolders.vdf")) {
        for line in content.lines() {
            let values = quoted_values(line);
            if values.len() >= 2 && values[0].eq_ignore_ascii_case("path") {
                let library = PathBuf::from(values[1].replace(r"\\", r"\"));
                steamapps.push(library.join("steamapps"));
            }
        }
    }
    steamapps.sort();
    steamapps.dedup();
    steamapps
}

#[cfg(target_os = "linux")]
pub fn proton_source_roots(location: &SourceLocation) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for steamapps in steamapps_directories(location) {
        let Ok(prefixes) = std::fs::read_dir(steamapps.join("compatdata")) else {
            continue;
        };
        for prefix in prefixes.filter_map(Result::ok) {
            let users = prefix.path().join("pfx/drive_c/users");
            let Ok(user_directories) = std::fs::read_dir(users) else {
                continue;
            };
            for user in user_directories
                .filter_map(Result::ok)
                .map(|entry| entry.path())
            {
                for relative in [
                    "AppData/Roaming/Goldberg SteamEmu Saves",
                    "AppData/Roaming/GSE Saves",
                    "AppData/Roaming/EMPRESS",
                    "AppData/Roaming/Steam/CODEX",
                    "AppData/Roaming/SmartSteamEmu",
                    "AppData/Roaming/CreamAPI",
                    "AppData/Local/SKIDROW",
                    "AppData/Local/anadius/LSX emu/achievement_watcher",
                    "Documents/SKIDROW",
                    "Documents/Steam/CODEX",
                    "Documents/Steam/RUNE",
                    "Documents/EMPRESS",
                ] {
                    let candidate = user.join(relative);
                    if candidate.is_dir() {
                        roots.push(candidate);
                    }
                }
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

pub fn installed_app_ids(location: &SourceLocation) -> Vec<String> {
    installed_games(location)
        .into_iter()
        .map(|(game_id, _)| game_id)
        .collect()
}

#[cfg(windows)]
pub fn running_app_id() -> Option<String> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    let key: Vec<u16> = "Software\\Valve\\Steam\0".encode_utf16().collect();
    let value_name: Vec<u16> = "RunningAppID\0".encode_utf16().collect();
    let mut app_id = 0_u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value_name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut app_id as *mut u32).cast(),
            &mut size,
        )
    };
    (status == 0 && app_id > 0).then(|| app_id.to_string())
}

#[cfg(target_os = "linux")]
pub fn running_app_id() -> Option<String> {
    crate::process::running_steam_app_id()
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn running_app_id() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_official_steam_cache_names() {
        assert_eq!(
            stats_file_identity(Path::new("UserGameStats_430715348_504230.bin")),
            Some(("430715348".into(), "504230".into()))
        );
        assert!(stats_file_identity(Path::new("achievements.ini")).is_none());
    }

    #[test]
    fn extracts_vdf_tokens_without_treating_whitespace_as_data() {
        assert_eq!(
            quoted_values("\t\"PersonaName\"\t\t\"Green\""),
            ["PersonaName", "Green"]
        );
    }

    #[test]
    fn reads_installed_game_names_from_manifests() {
        let root = std::env::temp_dir().join(format!(
            "achievement-watcher-steam-manifest-{}",
            std::process::id()
        ));
        let stats = root.join("appcache/stats");
        let steamapps = root.join("steamapps");
        std::fs::create_dir_all(&stats).unwrap();
        std::fs::create_dir_all(&steamapps).unwrap();
        std::fs::write(
            steamapps.join("appmanifest_400.acf"),
            "\"AppState\"\n{\n\t\"appid\" \"400\"\n\t\"name\" \"Portal\"\n}",
        )
        .unwrap();
        std::fs::write(
            steamapps.join("appmanifest_1493710.acf"),
            "\"AppState\"\n{\n\t\"appid\" \"1493710\"\n\t\"name\" \"Proton Experimental\"\n}",
        )
        .unwrap();
        let location = SourceLocation {
            id: "steam".into(),
            kind: aw_core::SourceKind::Steam,
            path: stats,
            enabled: true,
            notify: true,
        };

        assert_eq!(
            installed_games(&location),
            [("400".into(), "Portal".into())]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn finds_known_emulator_saves_inside_proton_prefixes() {
        let root = std::env::temp_dir().join(format!(
            "achievement-watcher-proton-sources-{}",
            std::process::id()
        ));
        let stats = root.join("appcache/stats");
        let saves = root.join(
            "steamapps/compatdata/504230/pfx/drive_c/users/steamuser/AppData/Roaming/Goldberg SteamEmu Saves",
        );
        std::fs::create_dir_all(&stats).unwrap();
        std::fs::create_dir_all(&saves).unwrap();
        let location = SourceLocation {
            id: "steam".into(),
            kind: aw_core::SourceKind::Steam,
            path: stats,
            enabled: true,
            notify: true,
        };

        assert_eq!(proton_source_roots(&location), [saves]);
        std::fs::remove_dir_all(root).unwrap();
    }
}
