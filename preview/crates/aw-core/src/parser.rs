use crate::{AchievementObservation, Error, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, fs, path::Path};

pub fn parse_achievement_file(
    path: &Path,
    source_id: &str,
    game_id: &str,
) -> Result<Vec<AchievementObservation>> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if name.eq_ignore_ascii_case("stats.bin") {
        return parse_sse(&fs::read(path)?, source_id, game_id);
    }
    if name.eq_ignore_ascii_case("TROPUSR.DAT") {
        return parse_rpcs3_trophies(&fs::read(path)?, source_id, game_id);
    }
    let content = fs::read_to_string(path)?;
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        parse_json(&content, source_id, game_id)
    } else {
        parse_ini(&content, source_id, game_id)
    }
}

pub fn parse_json(
    content: &str,
    source_id: &str,
    game_id: &str,
) -> Result<Vec<AchievementObservation>> {
    let value: Value = serde_json::from_str(content)?;
    let entries: Vec<(String, Value)> = match value {
        Value::Array(items) => items
            .into_iter()
            .enumerate()
            .map(|(index, value)| (index.to_string(), value))
            .collect(),
        Value::Object(mut object) => {
            for key in ["achievements", "Achievements", "ACHIEVE_DATA"] {
                if let Some(Value::Object(inner)) = object.remove(key) {
                    return normalize_entries(inner.into_iter().collect(), source_id, game_id);
                }
                if let Some(Value::Array(inner)) = object.remove(key) {
                    return normalize_entries(
                        inner
                            .into_iter()
                            .enumerate()
                            .map(|(i, v)| (i.to_string(), v))
                            .collect(),
                        source_id,
                        game_id,
                    );
                }
            }
            object.into_iter().collect()
        }
        _ => {
            return Err(Error::Invalid(
                "achievement JSON must contain an object or array".into(),
            ));
        }
    };
    normalize_entries(entries, source_id, game_id)
}

pub fn parse_ini(
    content: &str,
    source_id: &str,
    game_id: &str,
) -> Result<Vec<AchievementObservation>> {
    let sections = parse_ini_sections(content);
    if let (Some(states), Some(times)) = (
        sections.get("Achievements"),
        sections.get("AchievementsUnlockTimes"),
    ) {
        let entries = states
            .iter()
            .map(|(id, value)| {
                let mut object = Map::new();
                object.insert("Achieved".into(), Value::String(value.clone()));
                if let Some(time) = times.get(id) {
                    object.insert("UnlockTime".into(), Value::String(time.clone()));
                }
                (id.clone(), Value::Object(object))
            })
            .collect();
        return normalize_entries(entries, source_id, game_id);
    }
    if let (Some(states), Some(times)) = (sections.get("State"), sections.get("Time")) {
        let entries = states
            .iter()
            .map(|(id, value)| {
                let mut object = Map::new();
                object.insert(
                    "Achieved".into(),
                    Value::Bool(value.eq_ignore_ascii_case("0101")),
                );
                if let Some(time) = times.get(id).and_then(|value| decode_le_hex(value)) {
                    object.insert("UnlockTime".into(), Value::Number(time.into()));
                }
                (id.clone(), Value::Object(object))
            })
            .collect();
        return normalize_entries(entries, source_id, game_id);
    }
    if let Some(tenoke) = sections.get("ACHIEVEMENTS") {
        let entries = tenoke
            .iter()
            .map(|(id, raw)| {
                let unlocked = raw.to_ascii_lowercase().contains("unlocked=true");
                let time = raw
                    .split(',')
                    .find_map(|part| part.trim().strip_prefix("time="))
                    .map(|v| v.trim_matches('}'))
                    .unwrap_or("0");
                let mut object = Map::new();
                object.insert("Achieved".into(), Value::Bool(unlocked));
                object.insert("UnlockTime".into(), Value::String(time.into()));
                (id.trim_matches('"').to_string(), Value::Object(object))
            })
            .collect();
        return normalize_entries(entries, source_id, game_id);
    }
    let ignored = [
        "SteamAchievements",
        "Steam64",
        "Steam",
        "Settings",
        "GameSettings",
    ];
    let entries = sections
        .into_iter()
        .filter(|(name, _)| {
            !ignored
                .iter()
                .any(|ignored| name.eq_ignore_ascii_case(ignored))
        })
        .map(|(name, values)| {
            (
                name,
                Value::Object(
                    values
                        .into_iter()
                        .map(|(k, v)| (k, Value::String(v)))
                        .collect(),
                ),
            )
        })
        .collect();
    normalize_entries(entries, source_id, game_id)
}

fn parse_ini_sections(content: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut sections = BTreeMap::new();
    let mut current = String::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            sections
                .entry(current.clone())
                .or_insert_with(BTreeMap::new)
                .insert(
                    key.trim().trim_matches('"').to_string(),
                    value.trim().to_string(),
                );
        }
    }
    sections
}

fn normalize_entries(
    entries: Vec<(String, Value)>,
    source_id: &str,
    game_id: &str,
) -> Result<Vec<AchievementObservation>> {
    let mut result = Vec::new();
    for (fallback_id, value) in entries {
        let (id, achieved, hidden, current, max, unlock_time, display_name, description, icon) =
            match value {
                Value::Object(object) => {
                    let id = string_field(&object, &["id", "apiname", "name", "AchievementId"])
                        .unwrap_or(fallback_id);
                    let rld_state = little_endian_hex_field(&object, "State");
                    let current = little_endian_hex_field(&object, "CurProgress")
                        .or_else(|| integer_field(&object, &["CurProgress", "progress"]))
                        .unwrap_or(0);
                    let max = little_endian_hex_field(&object, "MaxProgress")
                        .or_else(|| integer_field(&object, &["MaxProgress", "max_progress"]))
                        .unwrap_or(0);
                    let mut unlock_time = little_endian_hex_field(&object, "Time")
                        .or_else(|| {
                            integer_field(
                                &object,
                                &[
                                    "UnlockTime",
                                    "unlocktime",
                                    "unlock_time",
                                    "HaveAchievedTime",
                                    "HaveHaveAchievedTime",
                                    "Time",
                                    "earned_time",
                                    "timestamp",
                                ],
                            )
                        })
                        .unwrap_or(0);
                    if lookup(&object, &["unlocktime"])
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| value.len() == 7)
                    {
                        unlock_time *= 1_000;
                    }
                    let achieved = bool_field(
                        &object,
                        &[
                            "Achieved",
                            "achieved",
                            "State",
                            "HaveAchieved",
                            "Unlocked",
                            "unlocked",
                            "earned",
                        ],
                    )
                    .unwrap_or(false)
                        || rld_state == Some(1)
                        || (max > 0 && current >= max)
                        || unlock_time > 0;
                    (
                        id,
                        achieved,
                        bool_field(&object, &["hidden", "Hidden"]).unwrap_or(false),
                        current,
                        max,
                        unlock_time,
                        string_field(&object, &["displayName", "display_name"]),
                        string_field(&object, &["description"]),
                        string_field(&object, &["icon"]),
                    )
                }
                Value::String(value) => (
                    fallback_id,
                    parse_bool(&value),
                    false,
                    0,
                    0,
                    0,
                    None,
                    None,
                    None,
                ),
                Value::Bool(value) => (fallback_id, value, false, 0, 0, 0, None, None, None),
                Value::Number(value) => (
                    fallback_id,
                    value.as_i64() == Some(1),
                    false,
                    0,
                    0,
                    0,
                    None,
                    None,
                    None,
                ),
                _ => continue,
            };
        result.push(AchievementObservation {
            source_id: source_id.into(),
            origin_source_id: None,
            game_id: game_id.into(),
            achievement_id: id,
            achieved,
            hidden,
            global_percent_hundredths: None,
            trophy_grade: None,
            current_progress: current,
            max_progress: max,
            unlock_time,
            display_name,
            description,
            icon,
        });
    }
    result.sort_by(|left, right| left.achievement_id.cmp(&right.achievement_id));
    Ok(result)
}

fn lookup<'a>(object: &'a Map<String, Value>, names: &[&str]) -> Option<&'a Value> {
    object
        .iter()
        .find(|(key, _)| names.iter().any(|name| key.eq_ignore_ascii_case(name)))
        .map(|(_, value)| value)
}

fn string_field(object: &Map<String, Value>, names: &[&str]) -> Option<String> {
    lookup(object, names).and_then(|value| match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    })
}

fn integer_field(object: &Map<String, Value>, names: &[&str]) -> Option<i64> {
    lookup(object, names).and_then(|value| match value {
        Value::Number(value) => value.as_i64(),
        Value::String(value) => value.parse().ok(),
        Value::Bool(value) => Some(*value as i64),
        _ => None,
    })
}

fn bool_field(object: &Map<String, Value>, names: &[&str]) -> Option<bool> {
    lookup(object, names).map(|value| match value {
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_i64() == Some(1),
        Value::String(value) => parse_bool(value),
        _ => false,
    })
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "0101"
    )
}

fn decode_le_hex(value: &str) -> Option<i64> {
    let value = value.trim().trim_matches('"');
    if value.len() < 8 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0_u8; 4];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(u32::from_le_bytes(bytes) as i64)
}

fn little_endian_hex_field(object: &Map<String, Value>, name: &str) -> Option<i64> {
    lookup(object, &[name])
        .and_then(Value::as_str)
        .and_then(decode_le_hex)
}

pub fn parse_sse(
    bytes: &[u8],
    source_id: &str,
    game_id: &str,
) -> Result<Vec<AchievementObservation>> {
    if bytes.len() < 4 {
        return Err(Error::Invalid("SSE file is shorter than its header".into()));
    }
    let expected = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let (records, remainder) = bytes[4..].as_chunks::<24>();
    if records.len() != expected || !remainder.is_empty() {
        return Err(Error::Invalid(
            "SSE record count does not match header".into(),
        ));
    }
    let mut result = Vec::new();
    for record in records {
        if record[20] != 1 {
            continue;
        }
        result.push(AchievementObservation {
            source_id: source_id.into(),
            origin_source_id: None,
            game_id: game_id.into(),
            achievement_id: record[0..4]
                .iter()
                .rev()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            achieved: true,
            hidden: false,
            global_percent_hundredths: None,
            trophy_grade: None,
            current_progress: 0,
            max_progress: 0,
            unlock_time: u32::from_le_bytes(record[8..12].try_into().unwrap()) as i64,
            display_name: None,
            description: None,
            icon: None,
        });
    }
    Ok(result)
}

pub fn parse_rpcs3_trophies(
    bytes: &[u8],
    source_id: &str,
    game_id: &str,
) -> Result<Vec<AchievementObservation>> {
    const HEADER: &[u8] = &[0x81, 0x8f, 0x54, 0xad];
    const DELIMITERS: [&[u8]; 2] = [&[0x04, 0, 0, 0, 0x50], &[0x06, 0, 0, 0, 0x60]];
    if !bytes.starts_with(HEADER) {
        return Err(Error::Invalid("unexpected RPCS3 trophy header".into()));
    }
    let positions: Vec<usize> = bytes
        .windows(DELIMITERS[0].len())
        .enumerate()
        .filter_map(|(index, window)| (window == DELIMITERS[0]).then_some(index))
        .collect();
    if positions.len() < 2 {
        return Err(Error::Invalid(
            "RPCS3 trophy table header is missing".into(),
        ));
    }
    let data = &bytes[positions[1] + DELIMITERS[0].len()..];
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < data.len() {
        if let Some(delimiter) = DELIMITERS
            .iter()
            .find(|delimiter| data[index..].starts_with(delimiter))
        {
            chunks.push(&data[start..index]);
            index += delimiter.len();
            start = index;
        } else {
            index += 1;
        }
    }
    chunks.push(&data[start..]);
    if chunks.len() % 2 != 0 || chunks.len() / 2 > 128 {
        return Err(Error::Invalid("unexpected RPCS3 trophy count".into()));
    }
    let half = chunks.len() / 2;
    let mut result = Vec::new();
    for index in 0..half {
        if chunks[index].len() < 20 || chunks[index + half].len() < 16 {
            continue;
        }
        let id = i32::from_be_bytes(chunks[index][0..4].try_into().unwrap());
        let timestamp = &chunks[index][16..20];
        result.push(AchievementObservation {
            source_id: source_id.into(),
            origin_source_id: None,
            game_id: game_id.into(),
            achievement_id: id.to_string(),
            achieved: i32::from_be_bytes(chunks[index + half][12..16].try_into().unwrap()) == 1,
            hidden: false,
            global_percent_hundredths: None,
            trophy_grade: None,
            current_progress: 0,
            max_progress: 0,
            unlock_time: if timestamp == [0xff; 4] {
                0
            } else {
                i32::from_be_bytes(timestamp.try_into().unwrap()) as i64
            },
            display_name: None,
            description: None,
            icon: None,
        });
    }
    Ok(result)
}

#[derive(Deserialize)]
struct TrophyConfig {
    #[serde(rename = "title-name")]
    title_name: String,
    #[serde(rename = "trophy", default)]
    trophies: Vec<TrophyDefinition>,
}

#[derive(Deserialize)]
struct TrophyDefinition {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "@hidden", default)]
    hidden: String,
    #[serde(rename = "@ttype", default)]
    grade: String,
    name: Option<String>,
    detail: Option<String>,
}

pub fn enrich_rpcs3_schema(
    trophy_data_path: &Path,
    observations: &mut [AchievementObservation],
) -> Result<Option<(String, Option<String>)>> {
    let Some(directory) = trophy_data_path.parent() else {
        return Ok(None);
    };
    let xml = match fs::read_to_string(directory.join("TROPCONF.SFM")) {
        Ok(xml) => xml,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let schema: TrophyConfig = quick_xml::de::from_str(&xml)
        .map_err(|error| Error::Invalid(format!("invalid RPCS3 trophy schema: {error}")))?;
    for trophy in schema.trophies {
        let normalized_id = trophy
            .id
            .parse::<i32>()
            .map(|id| id.to_string())
            .unwrap_or_else(|_| trophy.id.clone());
        let Some(observation) = observations
            .iter_mut()
            .find(|item| item.achievement_id == normalized_id)
        else {
            continue;
        };
        observation.display_name = trophy.name;
        observation.description = trophy.detail;
        observation.hidden = trophy.hidden.eq_ignore_ascii_case("yes");
        observation.trophy_grade = match trophy.grade.to_ascii_uppercase().as_str() {
            "P" => Some("platinum".into()),
            "G" => Some("gold".into()),
            "S" => Some("silver".into()),
            "B" => Some("bronze".into()),
            _ => None,
        };
        observation.icon = image_data_url(&directory.join(format!("TROP{}.PNG", trophy.id)));
    }
    Ok(Some((
        schema.title_name,
        image_data_url(&directory.join("ICON0.PNG")),
    )))
}

fn image_data_url(path: &Path) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|bytes| format!("data:image/png;base64,{}", STANDARD.encode(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_ini_variants() {
        let parsed = parse_ini(
            "[ACH_WIN]\nAchieved=1\nUnlockTime=123\nCurProgress=4\nMaxProgress=4",
            "steam",
            "400",
        )
        .unwrap();
        assert_eq!(parsed[0].achievement_id, "ACH_WIN");
        assert!(parsed[0].achieved);
        assert_eq!(parsed[0].unlock_time, 123);
    }

    #[test]
    fn parses_tenoke_values() {
        let parsed = parse_ini(
            "[ACHIEVEMENTS]\n\"WIN\"={unlocked=true, time=1712253396}",
            "steam",
            "1",
        )
        .unwrap();
        assert!(parsed[0].achieved);
        assert_eq!(parsed[0].unlock_time, 1712253396);
    }

    #[test]
    fn parses_3dm_state_and_little_endian_time_sections() {
        let parsed = parse_ini(
            "[State]\nWIN=0101\nLOCKED=0000\n[Time]\nWIN=7b000000",
            "steam",
            "1",
        )
        .unwrap();
        let unlocked = parsed
            .iter()
            .find(|item| item.achievement_id == "WIN")
            .unwrap();
        assert!(unlocked.achieved);
        assert_eq!(unlocked.unlock_time, 123);
        assert!(
            !parsed
                .iter()
                .find(|item| item.achievement_id == "LOCKED")
                .unwrap()
                .achieved
        );
    }

    #[test]
    fn parses_reloaded_hex_encoded_fields() {
        let parsed = parse_ini(
            "[WIN]\nState=01000000\nCurProgress=04000000\nMaxProgress=0a000000\nTime=7b000000",
            "steam",
            "1",
        )
        .unwrap();
        assert!(parsed[0].achieved);
        assert_eq!(parsed[0].current_progress, 4);
        assert_eq!(parsed[0].max_progress, 10);
        assert_eq!(parsed[0].unlock_time, 123);
    }

    #[test]
    fn repairs_short_cream_api_timestamps() {
        let parsed = parse_ini("[WIN]\nachieved=1\nunlocktime=1234567", "steam", "1").unwrap();
        assert_eq!(parsed[0].unlock_time, 1_234_567_000);
    }

    #[test]
    fn preserves_hidden_metadata() {
        let parsed = parse_json(
            r#"{"SECRET":{"achieved":false,"hidden":true}}"#,
            "steam",
            "1",
        )
        .unwrap();
        assert!(parsed[0].hidden);
    }

    #[test]
    fn rejects_partial_sse_files() {
        assert!(parse_sse(&[1, 0, 0, 0, 1, 2], "steam", "1").is_err());
    }

    #[test]
    fn enriches_rpcs3_trophies_from_local_schema() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("TROPUSR.DAT"), []).unwrap();
        std::fs::write(directory.path().join("TROP000.PNG"), [1, 2, 3]).unwrap();
        std::fs::write(
            directory.path().join("TROPCONF.SFM"),
            r#"<trophyconf><title-name>Example Game</title-name><trophy id="000" hidden="yes" ttype="G"><name>Secret Gold</name><detail>Found it</detail></trophy></trophyconf>"#,
        ).unwrap();
        let mut observations = vec![AchievementObservation {
            source_id: "rpcs3".into(),
            origin_source_id: None,
            game_id: "NPXX00000".into(),
            achievement_id: "0".into(),
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
        let game = enrich_rpcs3_schema(&directory.path().join("TROPUSR.DAT"), &mut observations)
            .unwrap()
            .unwrap();
        assert_eq!(game.0, "Example Game");
        assert_eq!(observations[0].display_name.as_deref(), Some("Secret Gold"));
        assert_eq!(observations[0].trophy_grade.as_deref(), Some("gold"));
        assert!(observations[0].hidden);
        assert!(
            observations[0]
                .icon
                .as_deref()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
    }
}
