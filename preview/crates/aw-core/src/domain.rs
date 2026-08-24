use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AchievementObservation {
    pub source_id: String,
    #[serde(default)]
    pub origin_source_id: Option<String>,
    pub game_id: String,
    pub achievement_id: String,
    pub achieved: bool,
    pub hidden: bool,
    pub global_percent_hundredths: Option<u32>,
    #[serde(default)]
    pub trophy_grade: Option<String>,
    pub current_progress: i64,
    pub max_progress: i64,
    pub unlock_time: i64,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
}

pub fn merge_observations(
    observations: Vec<AchievementObservation>,
    recent_first: bool,
    source_priorities: &BTreeMap<String, u8>,
) -> Vec<AchievementObservation> {
    let mut merged: BTreeMap<(String, String), AchievementObservation> = BTreeMap::new();
    for mut observation in observations {
        let key = (
            observation.game_id.clone(),
            observation.achievement_id.to_ascii_lowercase(),
        );
        if observation.origin_source_id.is_none() {
            observation.origin_source_id = Some(observation.source_id.clone());
        }
        observation.source_id = "merged".into();
        match merged.get_mut(&key) {
            None => {
                merged.insert(key, observation);
            }
            Some(existing) => {
                let replace_state = observation.achieved && !existing.achieved;
                let observation_priority = observation
                    .origin_source_id
                    .as_ref()
                    .and_then(|source| source_priorities.get(source))
                    .copied()
                    .unwrap_or(u8::MAX);
                let existing_priority = existing
                    .origin_source_id
                    .as_ref()
                    .and_then(|source| source_priorities.get(source))
                    .copied()
                    .unwrap_or(u8::MAX);
                let replace_priority = observation.achieved == existing.achieved
                    && observation_priority < existing_priority;
                let replace_time = observation.achieved == existing.achieved
                    && observation_priority == existing_priority
                    && observation.unlock_time > 0
                    && (existing.unlock_time == 0
                        || if recent_first {
                            observation.unlock_time > existing.unlock_time
                        } else {
                            observation.unlock_time < existing.unlock_time
                        });
                if replace_state || replace_priority || replace_time {
                    let hidden = existing.hidden || observation.hidden;
                    let trophy_grade = observation
                        .trophy_grade
                        .take()
                        .or_else(|| existing.trophy_grade.take());
                    let global_percent = observation
                        .global_percent_hundredths
                        .or(existing.global_percent_hundredths);
                    observation.display_name = observation
                        .display_name
                        .take()
                        .or_else(|| existing.display_name.take());
                    observation.description = observation
                        .description
                        .take()
                        .or_else(|| existing.description.take());
                    observation.icon = observation.icon.take().or_else(|| existing.icon.take());
                    *existing = observation;
                    existing.hidden = hidden;
                    existing.global_percent_hundredths = global_percent;
                    existing.trophy_grade = trophy_grade;
                } else {
                    existing.hidden |= observation.hidden;
                    existing.global_percent_hundredths = existing
                        .global_percent_hundredths
                        .or(observation.global_percent_hundredths);
                    existing.trophy_grade =
                        existing.trophy_grade.take().or(observation.trophy_grade);
                    existing.display_name =
                        existing.display_name.take().or(observation.display_name);
                    existing.description = existing.description.take().or(observation.description);
                    existing.icon = existing.icon.take().or(observation.icon);
                    existing.current_progress =
                        existing.current_progress.max(observation.current_progress);
                    existing.max_progress = existing.max_progress.max(observation.max_progress);
                }
            }
        }
    }
    merged.into_values().collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationEvent {
    pub id: i64,
    pub event_key: String,
    pub kind: NotificationKind,
    pub observation: AchievementObservation,
    pub attempts: u32,
    pub next_attempt_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    Unlock,
    Progress,
}

impl NotificationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unlock => "unlock",
            Self::Progress => "progress",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceLocation {
    pub id: String,
    pub kind: SourceKind,
    pub path: PathBuf,
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub notify: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameLaunchConfig {
    pub executable: PathBuf,
    pub arguments: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Steam,
    SteamEmulator,
    GreenLuma,
    Rpcs3,
    Epic,
    Gog,
    LumaPlay,
    WatchdogCache,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GameSummary {
    pub source_id: String,
    pub source_kind: Option<SourceKind>,
    pub game_id: String,
    pub name: String,
    pub unlocked: u32,
    pub total: u32,
    pub platinum: u32,
    pub gold: u32,
    pub silver: u32,
    pub bronze: u32,
    pub last_unlock_time: i64,
    pub playtime_seconds: i64,
    pub last_played: i64,
    pub icon: Option<String>,
    pub tracked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct AppSettings {
    pub language: String,
    pub username: String,
    pub profile_avatar_path: Option<PathBuf>,
    pub profile_avatar_squared: bool,
    pub thumbnail_portrait: bool,
    pub show_hidden: bool,
    pub merge_duplicate: bool,
    pub time_merge_recent_first: bool,
    pub hide_zero: bool,
    pub run_at_login: bool,
    pub start_minimized: bool,
    pub close_to_tray: bool,
    pub check_for_updates: bool,
    pub skipped_update_version: Option<String>,
    pub blacklisted_game_ids: Vec<String>,
    pub game_launch_configs: BTreeMap<String, GameLaunchConfig>,
    pub show_play_button: bool,
    pub notification_mode: NotificationMode,
    pub notification_enabled: bool,
    pub notify_on_progress: bool,
    pub notify_on_playtime: bool,
    pub notification_show_description: bool,
    pub notification_max_age_seconds: u32,
    pub notification_require_running_game: bool,
    pub notification_preset: String,
    pub notification_sound: String,
    pub notification_custom_sound_path: Option<PathBuf>,
    pub rumble_enabled: bool,
    pub rumble_strength_percent: u8,
    pub rumble_duration_ms: u64,
    pub screenshot_enabled: bool,
    pub screenshot_overwrite: bool,
    pub obs_replay_enabled: bool,
    pub obs_host: String,
    pub obs_port: u16,
    pub obs_password: String,
    pub obs_start_replay_buffer: bool,
    pub custom_action_enabled: bool,
    pub custom_action_executable: PathBuf,
    pub custom_action_arguments: String,
    pub custom_action_working_directory: Option<PathBuf>,
    pub custom_action_hide_window: bool,
    pub notification_duration_percent: u16,
    pub notification_scale_percent: u16,
    pub game_bar_enabled: bool,
    pub game_bar_fullscreen_only: bool,
    pub game_bar_token: String,
    pub achievement_overlay_enabled: bool,
    pub achievement_overlay_hotkey: String,
    pub achievement_overlay_scale_percent: u16,
    pub websocket_enabled: bool,
    pub websocket_host: String,
    pub websocket_port: u16,
    pub gntp_enabled: bool,
    pub gntp_host: String,
    pub gntp_port: u16,
    pub source_locations: Vec<SourceLocation>,
    pub sources_initialized: bool,
    pub show_cached_games: bool,
    pub notification_position: String,
    pub screenshot_directory: Option<PathBuf>,
    pub steam_enabled: bool,
    pub steam_library_mode: String,
    pub steam_emulator_enabled: bool,
    pub green_luma_enabled: bool,
    pub rpcs3_enabled: bool,
    pub epic_enabled: bool,
    pub gog_enabled: bool,
    pub luma_play_enabled: bool,
    pub watchdog_cache_enabled: bool,
    pub steam_public_fallback: bool,
    pub steam_account_id: Option<String>,
    pub steam_api_key: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "english".into(),
            username: std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_else(|_| "User".into()),
            profile_avatar_path: None,
            profile_avatar_squared: false,
            thumbnail_portrait: false,
            show_hidden: false,
            merge_duplicate: true,
            time_merge_recent_first: false,
            hide_zero: false,
            run_at_login: false,
            start_minimized: false,
            close_to_tray: true,
            check_for_updates: true,
            skipped_update_version: None,
            blacklisted_game_ids: Vec::new(),
            game_launch_configs: BTreeMap::new(),
            show_play_button: false,
            notification_mode: NotificationMode::OverlayWithNativeFallback,
            notification_enabled: true,
            notify_on_progress: true,
            notify_on_playtime: false,
            notification_show_description: true,
            notification_max_age_seconds: 10,
            notification_require_running_game: true,
            notification_preset: "steam".into(),
            notification_sound: "steam_deck".into(),
            notification_custom_sound_path: None,
            rumble_enabled: false,
            rumble_strength_percent: 65,
            rumble_duration_ms: 450,
            screenshot_enabled: false,
            screenshot_overwrite: false,
            obs_replay_enabled: false,
            obs_host: "127.0.0.1".into(),
            obs_port: 4455,
            obs_password: String::new(),
            obs_start_replay_buffer: false,
            custom_action_enabled: false,
            custom_action_executable: PathBuf::new(),
            custom_action_arguments: String::new(),
            custom_action_working_directory: None,
            custom_action_hide_window: true,
            notification_duration_percent: 100,
            notification_scale_percent: 100,
            game_bar_enabled: false,
            game_bar_fullscreen_only: true,
            game_bar_token: random_token(),
            achievement_overlay_enabled: false,
            achievement_overlay_hotkey: "Ctrl+Shift+O".into(),
            achievement_overlay_scale_percent: 100,
            websocket_enabled: false,
            websocket_host: "127.0.0.1".into(),
            websocket_port: 8082,
            gntp_enabled: false,
            gntp_host: "127.0.0.1".into(),
            gntp_port: 23053,
            source_locations: Vec::new(),
            sources_initialized: false,
            show_cached_games: true,
            notification_position: "bottom_right".into(),
            screenshot_directory: None,
            steam_enabled: true,
            steam_library_mode: "installed".into(),
            steam_emulator_enabled: true,
            green_luma_enabled: true,
            rpcs3_enabled: true,
            epic_enabled: true,
            gog_enabled: true,
            luma_play_enabled: false,
            watchdog_cache_enabled: true,
            steam_public_fallback: true,
            steam_account_id: None,
            steam_api_key: String::new(),
        }
    }
}

fn random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationMode {
    OverlayWithNativeFallback,
    OverlayOnly,
    NativeOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    pub imported_settings: bool,
    pub imported_sources: usize,
    pub imported_blacklist_entries: usize,
    pub imported_observations: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryReceipt {
    pub event_id: i64,
    pub transport: String,
    pub success: bool,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_settings_gain_a_secure_disabled_game_bar_configuration() {
        let settings: AppSettings = serde_json::from_str(
            r#"{"notificationMode":"native_only","screenshotEnabled":false,"notificationDurationMs":4000,"sourceLocations":[]}"#,
        )
        .unwrap();
        assert!(!settings.game_bar_enabled);
        assert_eq!(settings.game_bar_token.len(), 64);
        assert!(
            settings
                .game_bar_token
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
        assert!(!settings.show_play_button);
        assert!(!settings.achievement_overlay_enabled);
    }

    #[test]
    fn new_install_enables_only_the_core_background_features() {
        let settings = AppSettings::default();
        assert!(settings.notification_enabled);
        assert!(!settings.screenshot_enabled);
        assert!(!settings.achievement_overlay_enabled);
        assert!(!settings.obs_replay_enabled);
        assert!(!settings.game_bar_enabled);
        assert!(!settings.websocket_enabled);
        assert!(!settings.gntp_enabled);
    }

    fn observation(source: &str, achieved: bool, unlock_time: i64) -> AchievementObservation {
        AchievementObservation {
            source_id: source.into(),
            origin_source_id: None,
            game_id: "504230".into(),
            achievement_id: "FIRST".into(),
            achieved,
            hidden: false,
            global_percent_hundredths: None,
            trophy_grade: None,
            current_progress: if achieved { 1 } else { 0 },
            max_progress: 1,
            unlock_time,
            display_name: Some(format!("from {source}")),
            description: None,
            icon: None,
        }
    }

    #[test]
    fn duplicate_sources_are_unioned_without_double_counting() {
        let merged = merge_observations(
            vec![observation("steam", false, 0), observation("emu", true, 20)],
            false,
            &BTreeMap::new(),
        );
        assert_eq!(merged.len(), 1);
        assert!(merged[0].achieved);
        assert_eq!(merged[0].unlock_time, 20);
    }

    #[test]
    fn timestamp_merge_order_is_configurable() {
        let values = vec![observation("one", true, 20), observation("two", true, 10)];
        assert_eq!(
            merge_observations(values.clone(), false, &BTreeMap::new())[0].unlock_time,
            10
        );
        assert_eq!(
            merge_observations(values, true, &BTreeMap::new())[0].unlock_time,
            20
        );
    }

    #[test]
    fn equal_states_prefer_the_higher_priority_source() {
        let values = vec![
            observation("emulator", true, 10),
            observation("steam", true, 20),
        ];
        let priorities = BTreeMap::from([("steam".into(), 0), ("emulator".into(), 1)]);
        let merged = merge_observations(values, false, &priorities);
        assert_eq!(merged[0].origin_source_id.as_deref(), Some("steam"));
        assert_eq!(merged[0].unlock_time, 20);
    }
}
