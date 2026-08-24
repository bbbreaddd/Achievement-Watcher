use aw_core::{AchievementObservation, SourceLocation};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SteamSnapshot {
    app_id: u32,
    achievements: Vec<SteamAchievement>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SteamAchievement {
    api_name: String,
    display_name: String,
    description: String,
    #[serde(default)]
    hidden: bool,
    achieved: bool,
    unlock_time: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamAccount {
    pub account_id: String,
    pub steam_id: String,
    pub name: String,
    pub most_recent: bool,
    pub avatar_path: Option<PathBuf>,
}

pub fn accounts(locations: &[SourceLocation]) -> Vec<SteamAccount> {
    let mut accounts = Vec::new();
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
                });
            } else if values.len() >= 2
                && let Some(account) = current.as_mut()
            {
                match values[0].as_str() {
                    "PersonaName" => account.name = values[1].clone(),
                    "AccountName" if account.name.is_empty() => account.name = values[1].clone(),
                    "MostRecent" => account.most_recent = values[1] == "1",
                    _ => {}
                }
            }
        }
        if let Some(account) = current {
            accounts.push(account);
        }
    }
    accounts.sort_by_key(|account| !account.most_recent);
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

pub fn installed_app_ids(location: &SourceLocation) -> Vec<String> {
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
    let mut ids = Vec::new();
    for directory in steamapps {
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
                ids.push(id.to_owned());
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

pub fn read_client_snapshot(
    source_id: &str,
    game_id: &str,
) -> Result<Vec<AchievementObservation>, String> {
    let app_id = game_id.parse::<u32>().map_err(|_| "invalid Steam App ID")?;
    let helper = helper_path()?;
    let mut command = Command::new(&helper);
    command.arg(game_id);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command
        .output()
        .map_err(|error| format!("could not start {}: {error}", helper.display()))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let snapshot: SteamSnapshot = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid Steam helper response: {error}"))?;
    if snapshot.app_id != app_id {
        return Err("Steam helper returned data for the wrong app".into());
    }
    Ok(snapshot
        .achievements
        .into_iter()
        .map(|achievement| AchievementObservation {
            source_id: source_id.to_owned(),
            origin_source_id: None,
            game_id: game_id.to_owned(),
            achievement_id: achievement.api_name,
            achieved: achievement.achieved,
            hidden: achievement.hidden,
            global_percent_hundredths: None,
            trophy_grade: None,
            current_progress: 0,
            max_progress: 0,
            unlock_time: i64::from(achievement.unlock_time),
            display_name: Some(achievement.display_name),
            description: Some(achievement.description),
            icon: None,
        })
        .collect())
}

fn helper_path() -> Result<PathBuf, String> {
    let current = std::env::current_exe().map_err(|error| error.to_string())?;
    let directory = current
        .parent()
        .ok_or("application executable has no directory")?;
    let name = if cfg!(windows) {
        "achievement-watcher-steam-helper.exe"
    } else {
        "achievement-watcher-steam-helper"
    };
    let path = directory.join(name);
    path.exists()
        .then_some(path)
        .ok_or_else(|| "Steam helper is not installed beside Achievement Watcher".into())
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

#[cfg(not(windows))]
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
}
