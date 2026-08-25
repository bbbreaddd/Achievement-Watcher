use std::collections::HashSet;

#[cfg(windows)]
pub fn running_names() -> HashSet<String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
    };
    let mut result = HashSet::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return result;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let length = entry
                    .szExeFile
                    .iter()
                    .position(|character| *character == 0)
                    .unwrap_or(entry.szExeFile.len());
                result.insert(String::from_utf16_lossy(&entry.szExeFile[..length]).to_lowercase());
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    result
}

#[cfg(target_os = "linux")]
pub fn running_names() -> HashSet<String> {
    linux_processes(std::path::Path::new("/proc")).0
}

#[cfg(target_os = "linux")]
pub fn running_steam_app_id() -> Option<String> {
    linux_processes(std::path::Path::new("/proc")).1
}

#[cfg(target_os = "linux")]
fn linux_processes(root: &std::path::Path) -> (HashSet<String>, Option<String>) {
    let mut names = HashSet::new();
    let mut app_ids = std::collections::HashMap::<String, usize>::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return (names, None);
    };
    for directory in entries.filter_map(Result::ok).filter(|entry| {
        entry
            .file_name()
            .to_string_lossy()
            .chars()
            .all(|character| character.is_ascii_digit())
    }) {
        let path = directory.path();
        if let Ok(name) = std::fs::read_to_string(path.join("comm")) {
            names.insert(name.trim().to_ascii_lowercase());
        }
        if let Ok(executable) = std::fs::read_link(path.join("exe"))
            && let Some(name) = executable.file_name().and_then(|name| name.to_str())
        {
            names.insert(name.to_ascii_lowercase());
        }
        if let Ok(environment) = std::fs::read(path.join("environ"))
            && let Some(app_id) = steam_app_id_from_environment(&environment)
        {
            *app_ids.entry(app_id).or_default() += 1;
        }
    }
    let app_id = app_ids
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(app_id, _)| app_id);
    (names, app_id)
}

#[cfg(target_os = "linux")]
fn steam_app_id_from_environment(environment: &[u8]) -> Option<String> {
    [b"SteamGameId=".as_slice(), b"SteamAppId=".as_slice()]
        .into_iter()
        .find_map(|prefix| {
            environment.split(|byte| *byte == 0).find_map(|entry| {
                let value = entry.strip_prefix(prefix)?;
                (!value.is_empty() && value.iter().all(u8::is_ascii_digit) && value != b"0")
                    .then(|| String::from_utf8_lossy(value).into_owned())
            })
        })
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn running_names() -> HashSet<String> {
    HashSet::new()
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    #[test]
    fn prefers_the_steam_game_id_environment_value() {
        let environment = b"SteamAppId=123\0SteamGameId=504230\0";
        assert_eq!(
            super::steam_app_id_from_environment(environment).as_deref(),
            Some("504230")
        );
    }
}
