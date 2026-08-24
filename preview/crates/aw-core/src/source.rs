use crate::{Result, SourceKind, SourceLocation};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};
use walkdir::WalkDir;

pub const ACHIEVEMENT_FILES: &[&str] = &[
    "achievements.ini",
    "achievements.json",
    "achiev.ini",
    "stats.ini",
    "Achievements.Bin",
    "achieve.dat",
    "Achievements.ini",
    "stats.bin",
    "CreamAPI.Achievements.cfg",
    "user_stats.ini",
];

pub fn discover_files(locations: &[SourceLocation]) -> Vec<PathBuf> {
    let mut files = BTreeSet::new();
    for source in locations.iter().filter(|source| source.enabled) {
        let max_depth = if source.kind == SourceKind::Rpcs3 {
            8
        } else {
            10
        };
        for entry in WalkDir::new(&source.path)
            .max_depth(max_depth)
            .follow_links(false)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            let is_goldberg_schema = name.eq_ignore_ascii_case("achievements.json")
                && entry
                    .path()
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|value| value.to_str())
                    .is_some_and(|parent| parent.eq_ignore_ascii_case("steam_settings"));
            if is_goldberg_schema {
                continue;
            }
            if ACHIEVEMENT_FILES
                .iter()
                .any(|candidate| name.eq_ignore_ascii_case(candidate))
                || (source.kind == SourceKind::Rpcs3 && name.eq_ignore_ascii_case("TROPUSR.DAT"))
            {
                files.insert(entry.into_path());
            }
        }
    }
    files.into_iter().collect()
}

pub fn infer_game_id(path: &Path) -> Option<String> {
    infer_configured_game_id(path)
        .or_else(|| infer_remote_game_id(path))
        .or_else(|| {
            path.ancestors()
                .skip(1)
                .filter_map(|parent| parent.file_name()?.to_str())
                .find(|name| {
                    !name.is_empty() && name.chars().all(|character| character.is_ascii_digit())
                })
                .map(str::to_owned)
        })
        .or_else(|| path.parent()?.file_name()?.to_str().map(str::to_owned))
}

fn infer_remote_game_id(path: &Path) -> Option<String> {
    let components: Vec<_> = path
        .ancestors()
        .skip(1)
        .filter_map(|directory| directory.file_name()?.to_str())
        .collect();
    let remote = components
        .iter()
        .position(|name| name.eq_ignore_ascii_case("remote"))?;
    components
        .get(remote + 1)
        .filter(|name| name.chars().all(|character| character.is_ascii_digit()))
        .map(|name| (*name).to_owned())
}

fn infer_configured_game_id(path: &Path) -> Option<String> {
    const CONFIGS: [&str; 8] = [
        "ALI213.ini",
        "valve.ini",
        "hlm.ini",
        "ds.ini",
        "steam_api.ini",
        "SteamConfig.ini",
        "tenoke.ini",
        "UniverseLAN.ini",
    ];
    for directory in path.ancestors().skip(1).take(7) {
        for config in CONFIGS {
            let Ok(content) = std::fs::read_to_string(directory.join(config)) else {
                continue;
            };
            for raw in content.lines() {
                let line = raw.trim();
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                if !["appid", "app_id", "id"]
                    .iter()
                    .any(|candidate| key.trim().eq_ignore_ascii_case(candidate))
                {
                    continue;
                }
                let id = value
                    .split('#')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .trim_matches(['"', '\'']);
                if !id.is_empty() && id.chars().all(|character| character.is_ascii_digit()) {
                    return Some(id.to_owned());
                }
            }
        }
    }
    None
}

pub fn read_when_stable(path: &Path, attempts: usize, delay: Duration) -> Result<Vec<u8>> {
    let mut previous = None;
    for _ in 0..attempts {
        let metadata = std::fs::metadata(path)?;
        let fingerprint = (metadata.len(), metadata.modified().ok());
        if previous.as_ref() == Some(&fingerprint) {
            return Ok(std::fs::read(path)?);
        }
        previous = Some(fingerprint);
        thread::sleep(delay);
    }
    Ok(std::fs::read(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_numeric_app_id_from_ancestors() {
        assert_eq!(
            infer_game_id(Path::new("C:/saves/400/stats/achievements.ini")).as_deref(),
            Some("400")
        );
    }

    #[test]
    fn infers_app_id_from_emulator_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let stats = directory.path().join("Profile/User/Stats");
        std::fs::create_dir_all(&stats).unwrap();
        std::fs::write(
            directory.path().join("ALI213.ini"),
            "[Settings]\nAppID=504230\n",
        )
        .unwrap();
        assert_eq!(
            infer_game_id(&stats.join("Achievements.Bin")).as_deref(),
            Some("504230")
        );
    }

    #[test]
    fn infers_empress_app_id_before_remote_account_id() {
        assert_eq!(
            infer_game_id(Path::new(
                "C:/Users/User/AppData/Roaming/EMPRESS/504230/remote/76561198000000000/achievements.json"
            ))
            .as_deref(),
            Some("504230")
        );
    }

    #[test]
    fn ignores_goldberg_schema_but_keeps_live_json() {
        let directory = tempfile::tempdir().unwrap();
        let schema = directory.path().join("game/steam_settings");
        let live = directory.path().join("saves/400");
        std::fs::create_dir_all(&schema).unwrap();
        std::fs::create_dir_all(&live).unwrap();
        std::fs::write(schema.join("achievements.json"), "[]").unwrap();
        std::fs::write(live.join("achievements.json"), "{}").unwrap();
        let files = discover_files(&[SourceLocation {
            id: "test".into(),
            kind: SourceKind::SteamEmulator,
            path: directory.path().to_path_buf(),
            enabled: true,
            notify: true,
        }]);
        assert_eq!(files, vec![live.join("achievements.json")]);
    }
}
