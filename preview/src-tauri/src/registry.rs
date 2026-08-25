use aw_core::AchievementObservation;

use std::path::PathBuf;
#[cfg(windows)]
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
};

#[cfg(windows)]
pub fn steam_install_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for (root, key_name) in [
        (HKEY_CURRENT_USER, r"SOFTWARE\Valve\Steam"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam"),
    ] {
        let Ok(key) = RegKey::predef(root).open_subkey(key_name) else {
            continue;
        };
        for value_name in ["SteamPath", "InstallPath"] {
            if let Ok(value) = key.get_value::<String, _>(value_name) {
                let path = PathBuf::from(value.replace('/', r"\"));
                if !paths.iter().any(|existing| existing == &path) {
                    paths.push(path);
                }
            }
        }
    }
    paths
}

#[cfg(target_os = "linux")]
pub fn steam_install_paths() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let mut paths = [
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".steam/root"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ]
    .into_iter()
    .filter_map(|path| path.canonicalize().ok())
    .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn steam_install_paths() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(windows)]
pub fn documents_path() -> Option<PathBuf> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders")
        .ok()?
        .get_value::<String, _>("Personal")
        .ok()
        .map(|value| {
            let mut expanded = value;
            for (name, replacement) in std::env::vars() {
                expanded = expanded.replace(&format!("%{name}%"), &replacement);
            }
            PathBuf::from(expanded)
        })
}

#[cfg(target_os = "linux")]
pub fn documents_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Documents"))
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn documents_path() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn dword(key: &RegKey, name: &str) -> Option<u32> {
    key.get_value::<u32, _>(name).ok()
}

#[cfg(windows)]
pub fn observations(green_luma: bool, luma_play: bool) -> Vec<AchievementObservation> {
    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    let mut result = Vec::new();
    if green_luma {
        for variant in ["GLR", "GL2020", "GL2024", "GL2025"] {
            let Ok(apps) = current_user.open_subkey(format!(r"SOFTWARE\{variant}\AppID")) else {
                continue;
            };
            for app_id in apps.enum_keys().filter_map(Result::ok) {
                let Ok(app) = apps.open_subkey(&app_id) else {
                    continue;
                };
                if dword(&app, "SkipStatsAndAchievements").unwrap_or(1) != 0 {
                    continue;
                }
                let Ok(achievements) = app.open_subkey("Achievements") else {
                    continue;
                };
                for value in achievements.enum_values().filter_map(Result::ok) {
                    let name = value.0;
                    if name.ends_with("_Time") {
                        continue;
                    }
                    let achieved = dword(&achievements, &name).unwrap_or(0) != 0;
                    let unlock_time =
                        dword(&achievements, &format!("{name}_Time")).unwrap_or(0) as i64;
                    result.push(observation(
                        &format!("registry-greenluma-{}", variant.to_ascii_lowercase()),
                        &app_id,
                        &name,
                        achieved,
                        unlock_time,
                    ));
                }
            }
        }
    }
    if luma_play {
        let Ok(users) = current_user.open_subkey(r"SOFTWARE\LumaPlay") else {
            return result;
        };
        for user_id in users.enum_keys().filter_map(Result::ok) {
            let Ok(user) = users.open_subkey(&user_id) else {
                continue;
            };
            for app_id in user.enum_keys().filter_map(Result::ok) {
                let Ok(achievements) = user.open_subkey(format!(r"{app_id}\Achievements")) else {
                    continue;
                };
                for value in achievements.enum_values().filter_map(Result::ok) {
                    let name = value.0;
                    let achievement_id = name.strip_prefix("ACH_").unwrap_or(&name);
                    result.push(observation(
                        &format!("registry-lumaplay-{user_id}"),
                        &format!("UPLAY{app_id}"),
                        achievement_id,
                        dword(&achievements, &name).unwrap_or(0) != 0,
                        0,
                    ));
                }
            }
        }
    }
    result
}

#[cfg(windows)]
fn observation(
    source_id: &str,
    game_id: &str,
    achievement_id: &str,
    achieved: bool,
    unlock_time: i64,
) -> AchievementObservation {
    AchievementObservation {
        source_id: source_id.into(),
        origin_source_id: None,
        game_id: game_id.into(),
        achievement_id: achievement_id.into(),
        achieved,
        hidden: false,
        global_percent_hundredths: None,
        trophy_grade: None,
        current_progress: i64::from(achieved),
        max_progress: 1,
        unlock_time,
        display_name: None,
        description: None,
        icon: None,
    }
}

#[cfg(not(windows))]
pub fn observations(_green_luma: bool, _luma_play: bool) -> Vec<AchievementObservation> {
    Vec::new()
}

#[cfg(windows)]
pub fn configure_startup(enabled: bool) -> Result<(), String> {
    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    let (run, _) = current_user
        .create_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run")
        .map_err(|error| error.to_string())?;
    if enabled {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        run.set_value(
            "Achievement Watcher",
            &format!("\"{}\"", executable.display()),
        )
        .map_err(|error| error.to_string())
    } else {
        match run.delete_value("Achievement Watcher") {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[cfg(target_os = "linux")]
pub fn configure_startup(enabled: bool) -> Result<(), String> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or_else(|| "The Linux configuration directory could not be determined".to_string())?;
    let directory = config.join("autostart");
    let path = directory.join("achievement-watcher.desktop");
    if !enabled {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        };
    }
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    std::fs::write(path, linux_desktop_entry(&executable)).map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
fn linux_desktop_entry(executable: &std::path::Path) -> String {
    let escaped = executable
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$");
    format!(
        "[Desktop Entry]\nType=Application\nName=Achievement Watcher\nExec=\"{escaped}\"\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn configure_startup(_enabled: bool) -> Result<(), String> {
    Err("Starting automatically is unavailable on the current platform".into())
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    #[test]
    fn quotes_linux_autostart_executables() {
        let entry = super::linux_desktop_entry(std::path::Path::new(
            "/home/player/Achievement Watcher/$preview",
        ));
        assert!(entry.contains("Exec=\"/home/player/Achievement Watcher/\\$preview\""));
        assert!(entry.contains("Terminal=false"));
    }
}
