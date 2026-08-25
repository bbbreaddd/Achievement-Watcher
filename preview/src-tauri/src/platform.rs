use serde::Serialize;
use std::{path::Path, process::Command};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub os: &'static str,
    pub game_bar: bool,
    pub screenshots: bool,
    pub rumble: bool,
    pub custom_notifications: bool,
    pub automatic_update_install: bool,
}

pub fn capabilities() -> Capabilities {
    Capabilities {
        os: std::env::consts::OS,
        game_bar: cfg!(windows),
        screenshots: cfg!(windows),
        rumble: cfg!(windows),
        custom_notifications: cfg!(any(windows, target_os = "linux")),
        automatic_update_install: cfg!(windows),
    }
}

#[cfg(windows)]
pub fn open(target: impl AsRef<std::ffi::OsStr>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    Command::new("explorer.exe")
        .arg(target)
        .creation_flags(0x0800_0000)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
pub fn open(target: impl AsRef<std::ffi::OsStr>) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not open the requested item with xdg-open: {error}"))
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn open(_target: impl AsRef<std::ffi::OsStr>) -> Result<(), String> {
    Err("Opening this item is unavailable on the current platform".into())
}

pub fn reveal_file(path: &Path) -> Result<(), String> {
    let directory = path
        .parent()
        .ok_or_else(|| "The containing folder is unavailable".to_string())?;
    open(directory)
}
