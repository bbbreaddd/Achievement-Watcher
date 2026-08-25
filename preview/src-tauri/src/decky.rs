use serde::Serialize;
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
};

const PLUGIN_DIRECTORY: &str = "AchievementWatcher";
const VERSION: &str = "0.1.1";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub decky_installed: bool,
    pub companion_installed: bool,
    pub installed_version: Option<String>,
    pub available_version: &'static str,
    pub authentication_required: bool,
    pub polkit_available: bool,
}

fn plugin_root() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("homebrew/plugins"))
        .filter(|path| path.is_dir())
}

fn plugin_path(root: &Path) -> PathBuf {
    root.join(PLUGIN_DIRECTORY)
}

fn writable_by_current_user(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    let Ok(process) = std::fs::metadata("/proc/self") else {
        return false;
    };
    metadata.uid() == process.uid() && metadata.mode() & 0o200 != 0
}

pub fn status() -> Status {
    let root = plugin_root();
    let installed_path = root.as_deref().map(plugin_path);
    let installed_version = installed_path
        .as_deref()
        .and_then(|path| std::fs::read_to_string(path.join("package.json")).ok())
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|package| package.get("version")?.as_str().map(str::to_owned));
    let authentication_required = root
        .as_deref()
        .is_some_and(|path| !writable_by_current_user(path));
    Status {
        decky_installed: root.is_some(),
        companion_installed: installed_path.is_some_and(|path| path.is_dir()),
        installed_version,
        available_version: VERSION,
        authentication_required,
        polkit_available: Path::new("/usr/bin/pkexec").is_file(),
    }
}

pub fn install() -> Result<(), String> {
    let root = plugin_root().ok_or_else(|| "Decky Loader was not detected".to_string())?;
    let staging = std::env::temp_dir().join(format!(
        "achievement-watcher-decky-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ));
    write_package(&staging)?;
    let destination = plugin_path(&root);
    let result = if writable_by_current_user(&root) {
        replace_package(&staging, &destination)
    } else if Path::new("/usr/bin/pkexec").is_file() {
        elevated_replace(&staging, &destination)
    } else {
        Err("Decky's plugin folder requires administrator access, but pkexec is unavailable".into())
    };
    let _ = std::fs::remove_dir_all(&staging);
    result
}

pub fn remove() -> Result<(), String> {
    let root = plugin_root().ok_or_else(|| "Decky Loader was not detected".to_string())?;
    let destination = plugin_path(&root);
    if !destination.exists() {
        return Ok(());
    }
    if writable_by_current_user(&root) {
        return std::fs::remove_dir_all(destination).map_err(|error| error.to_string());
    }
    if !Path::new("/usr/bin/pkexec").is_file() {
        return Err(
            "Decky's plugin folder requires administrator access, but pkexec is unavailable".into(),
        );
    }
    let script = std::env::temp_dir().join(format!(
        "achievement-watcher-decky-remove-{}-{}.sh",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ));
    std::fs::write(
        &script,
        "#!/bin/sh\nset -eu\ntarget=$1\nrm -rf -- \"$target\"\nsystemctl restart plugin_loader.service\n",
    )
    .map_err(|error| error.to_string())?;
    let status = Command::new("/usr/bin/pkexec")
        .args(["/bin/sh"])
        .arg(&script)
        .arg(destination)
        .status()
        .map_err(|error| format!("could not start administrator authentication: {error}"));
    let _ = std::fs::remove_file(script);
    status?
        .success()
        .then_some(())
        .ok_or_else(|| "Decky companion removal was cancelled or failed".into())
}

fn write_package(staging: &Path) -> Result<(), String> {
    std::fs::create_dir_all(staging.join("dist")).map_err(|error| error.to_string())?;
    for (path, content) in [
        (
            "plugin.json",
            include_bytes!("../../decky-companion/plugin.json").as_slice(),
        ),
        (
            "package.json",
            include_bytes!("../../decky-companion/package.json").as_slice(),
        ),
        (
            "dist/index.js",
            include_bytes!("../../decky-companion/dist/index.js").as_slice(),
        ),
        ("LICENSE", include_bytes!("../../../LICENSE").as_slice()),
    ] {
        std::fs::write(staging.join(path), content).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn replace_package(staging: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        std::fs::remove_dir_all(destination).map_err(|error| error.to_string())?;
    }
    copy_directory(staging, destination)
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn elevated_replace(staging: &Path, destination: &Path) -> Result<(), String> {
    let script = staging.join("install.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nset -eu\nsource=$1\ntarget=$2\nrm -rf -- \"$target\"\nmkdir -p -- \"$target\"\ncp -R -- \"$source/.\" \"$target/\"\nrm -f -- \"$target/install.sh\"\nsystemctl restart plugin_loader.service\n",
    )
    .map_err(|error| error.to_string())?;
    let status = Command::new("/usr/bin/pkexec")
        .args(["/bin/sh"])
        .arg(&script)
        .arg(staging)
        .arg(destination)
        .status()
        .map_err(|error| format!("could not start administrator authentication: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "Decky companion installation was cancelled or failed".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_a_complete_decky_package() {
        let staging = std::env::temp_dir().join(format!(
            "achievement-watcher-decky-package-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&staging);
        write_package(&staging).unwrap();

        assert!(staging.join("plugin.json").is_file());
        assert!(staging.join("package.json").is_file());
        assert!(staging.join("dist/index.js").is_file());
        assert!(staging.join("LICENSE").is_file());

        std::fs::remove_dir_all(staging).unwrap();
    }
}
