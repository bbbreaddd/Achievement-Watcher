use crate::{
    AchievementObservation, AppSettings, GameSummary, MigrationReport, NotificationEvent,
    NotificationKind, Result,
};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

type StoredAchievementMetadata = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    Option<u32>,
);

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_memory() -> Result<Self> {
        let store = Self {
            connection: Connection::open_in_memory()?,
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.connection.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS settings (
               id INTEGER PRIMARY KEY CHECK (id = 1), json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS observations (
               source_id TEXT NOT NULL, game_id TEXT NOT NULL, achievement_id TEXT NOT NULL,
               achieved INTEGER NOT NULL, current_progress INTEGER NOT NULL,
               max_progress INTEGER NOT NULL, unlock_time INTEGER NOT NULL,
               display_name TEXT, description TEXT, icon TEXT, hidden INTEGER NOT NULL DEFAULT 0,
               global_percent_hundredths INTEGER,
               trophy_grade TEXT,
               PRIMARY KEY(source_id, game_id, achievement_id)
             );
             CREATE TABLE IF NOT EXISTS games (
               game_id TEXT PRIMARY KEY, name TEXT NOT NULL, icon TEXT
             );
             CREATE TABLE IF NOT EXISTS game_aliases (
               alias_game_id TEXT PRIMARY KEY, canonical_game_id TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS achievement_metadata (
               game_id TEXT NOT NULL, achievement_id TEXT NOT NULL,
               display_name TEXT, description TEXT, icon TEXT, locked_icon TEXT,
               hidden INTEGER NOT NULL DEFAULT 0,
               PRIMARY KEY(game_id,achievement_id)
             );
             CREATE TABLE IF NOT EXISTS game_activity (
               game_id TEXT PRIMARY KEY, playtime_seconds INTEGER NOT NULL DEFAULT 0,
               last_played INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS notification_events (
               id INTEGER PRIMARY KEY AUTOINCREMENT, event_key TEXT NOT NULL UNIQUE,
               kind TEXT NOT NULL, payload TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending',
               attempts INTEGER NOT NULL DEFAULT 0, next_attempt_at INTEGER NOT NULL DEFAULT 0,
               created_at INTEGER NOT NULL, delivered_at INTEGER
             );
             CREATE TABLE IF NOT EXISTS delivery_attempts (
               id INTEGER PRIMARY KEY AUTOINCREMENT, event_id INTEGER NOT NULL,
               transport TEXT NOT NULL, success INTEGER NOT NULL, error TEXT,
               attempted_at INTEGER NOT NULL,
               FOREIGN KEY(event_id) REFERENCES notification_events(id) ON DELETE CASCADE
             );
             CREATE TABLE IF NOT EXISTS migration_runs (
               source_path TEXT PRIMARY KEY, imported_at INTEGER NOT NULL, report TEXT NOT NULL
             );
             COMMIT;",
        )?;
        let _ = self.connection.execute(
            "ALTER TABLE observations ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = self.connection.execute(
            "ALTER TABLE achievement_metadata ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = self.connection.execute(
            "ALTER TABLE achievement_metadata ADD COLUMN global_percent_hundredths INTEGER",
            [],
        );
        let _ = self.connection.execute(
            "ALTER TABLE achievement_metadata ADD COLUMN locked_icon TEXT",
            [],
        );
        let _ = self
            .connection
            .execute("ALTER TABLE observations ADD COLUMN trophy_grade TEXT", []);
        Ok(())
    }

    pub fn load_settings(&self) -> Result<AppSettings> {
        let json: Option<String> = self
            .connection
            .query_row("SELECT json FROM settings WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()?;
        let mut settings = match json {
            Some(value) => serde_json::from_str(&value)?,
            None => AppSettings::default(),
        };
        settings.steam_api_key = crate::secure::unprotect(&settings.steam_api_key)?;
        settings.obs_password = crate::secure::unprotect(&settings.obs_password)?;
        Ok(settings)
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        let mut persisted = settings.clone();
        persisted.steam_api_key = crate::secure::protect(&persisted.steam_api_key)?;
        persisted.obs_password = crate::secure::protect(&persisted.obs_password)?;
        self.connection.execute(
            "INSERT INTO settings(id, json) VALUES(1, ?1)
             ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            [serde_json::to_string(&persisted)?],
        )?;
        Ok(())
    }

    pub fn record_observations(
        &mut self,
        observations: &[AchievementObservation],
        establish_baseline: bool,
    ) -> Result<Vec<NotificationEvent>> {
        let tx = self.connection.transaction()?;
        let mut created = Vec::new();

        for observation in observations {
            let previous: Option<(bool, i64)> = tx
                .query_row(
                    "SELECT achieved, current_progress FROM observations
                 WHERE source_id=?1 AND game_id=?2 AND achievement_id=?3",
                    params![
                        observation.source_id,
                        observation.game_id,
                        observation.achievement_id
                    ],
                    |row| Ok((row.get::<_, i64>(0)? != 0, row.get(1)?)),
                )
                .optional()?;

            let kind = match previous {
                None if !establish_baseline && observation.achieved => {
                    Some(NotificationKind::Unlock)
                }
                None => None,
                Some(_) if establish_baseline => None,
                Some((false, _)) if observation.achieved => Some(NotificationKind::Unlock),
                Some((_, previous_progress))
                    if !observation.achieved
                        && observation.max_progress > 0
                        && observation.current_progress > previous_progress =>
                {
                    Some(NotificationKind::Progress)
                }
                Some(_) => None,
            };

            tx.execute(
                "INSERT INTO observations(source_id, game_id, achievement_id, achieved,
                 current_progress, max_progress, unlock_time, display_name, description, icon, hidden,trophy_grade)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
                 ON CONFLICT(source_id,game_id,achievement_id) DO UPDATE SET
                 achieved=excluded.achieved,current_progress=excluded.current_progress,
                 max_progress=excluded.max_progress,unlock_time=excluded.unlock_time,
                 display_name=excluded.display_name,description=excluded.description,icon=excluded.icon,
                 hidden=excluded.hidden,trophy_grade=COALESCE(excluded.trophy_grade,observations.trophy_grade)",
                params![
                    observation.source_id, observation.game_id, observation.achievement_id,
                    observation.achieved as i64, observation.current_progress,
                    observation.max_progress, observation.unlock_time,
                    observation.display_name, observation.description, observation.icon,
                    observation.hidden as i64,
                    observation.trophy_grade,
                ],
            )?;

            if let Some(kind) = kind {
                let transition_value = if kind == NotificationKind::Unlock {
                    observation.unlock_time.max(1)
                } else {
                    observation.current_progress
                };
                let event_key = format!(
                    "{}:{}:{}:{}:{}",
                    observation.source_id,
                    observation.game_id,
                    observation.achievement_id,
                    kind.as_str(),
                    transition_value
                );
                tx.execute(
                    "INSERT OR IGNORE INTO notification_events(event_key,kind,payload,created_at)
                     VALUES(?1,?2,?3,?4)",
                    params![
                        event_key,
                        kind.as_str(),
                        serde_json::to_string(observation)?,
                        Utc::now().timestamp()
                    ],
                )?;
                if tx.changes() > 0 {
                    let id = tx.last_insert_rowid();
                    created.push(NotificationEvent {
                        id,
                        event_key,
                        kind,
                        observation: observation.clone(),
                        attempts: 0,
                        next_attempt_at: 0,
                    });
                }
            }
        }
        tx.commit()?;
        Ok(created)
    }

    pub fn pending_events(&self, now: i64, limit: usize) -> Result<Vec<NotificationEvent>> {
        let mut statement = self.connection.prepare(
            "SELECT id,event_key,kind,payload,attempts,next_attempt_at FROM notification_events
             WHERE status='pending' AND next_attempt_at<=?1 ORDER BY id LIMIT ?2",
        )?;
        let rows = statement.query_map(params![now, limit as i64], |row| {
            let kind: String = row.get(2)?;
            let payload: String = row.get(3)?;
            Ok((
                row.get(0)?,
                row.get(1)?,
                kind,
                payload,
                row.get::<_, i64>(4)? as u32,
                row.get(5)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, event_key, kind, payload, attempts, next_attempt_at) = row?;
            result.push(NotificationEvent {
                id,
                event_key,
                kind: if kind == "unlock" {
                    NotificationKind::Unlock
                } else {
                    NotificationKind::Progress
                },
                observation: serde_json::from_str(&payload)?,
                attempts,
                next_attempt_at,
            });
        }
        Ok(result)
    }

    pub fn record_delivery(
        &self,
        event_id: i64,
        transport: &str,
        result: std::result::Result<(), &str>,
    ) -> Result<()> {
        let now = Utc::now().timestamp();
        let (success, error) = match result {
            Ok(()) => (1, None),
            Err(error) => (0, Some(error)),
        };
        self.connection.execute(
            "INSERT INTO delivery_attempts(event_id,transport,success,error,attempted_at)
             VALUES(?1,?2,?3,?4,?5)",
            params![event_id, transport, success, error, now],
        )?;
        if success == 1 {
            self.connection.execute(
                "UPDATE notification_events SET status='delivered', delivered_at=?2 WHERE id=?1",
                params![event_id, now],
            )?;
        } else {
            let attempts: i64 = self.connection.query_row(
                "SELECT attempts FROM notification_events WHERE id=?1",
                [event_id],
                |row| row.get(0),
            )?;
            let next_attempts = attempts + 1;
            let delay = (1_i64 << next_attempts.min(8)) * 5;
            let status = if next_attempts >= 8 {
                "failed"
            } else {
                "pending"
            };
            self.connection.execute(
                "UPDATE notification_events SET attempts=?2,next_attempt_at=?3,status=?4 WHERE id=?1",
                params![event_id, next_attempts, now + delay, status],
            )?;
        }
        Ok(())
    }

    pub fn notification_queue_counts(&self) -> Result<(u32, u32)> {
        let pending = self.connection.query_row(
            "SELECT COUNT(*) FROM notification_events WHERE status='pending'",
            [],
            |row| row.get::<_, u32>(0),
        )?;
        let failed = self.connection.query_row(
            "SELECT COUNT(*) FROM notification_events WHERE status='failed'",
            [],
            |row| row.get::<_, u32>(0),
        )?;
        Ok((pending, failed))
    }

    pub fn retry_failed_notifications(&self) -> Result<usize> {
        Ok(self.connection.execute(
            "UPDATE notification_events SET status='pending',attempts=0,next_attempt_at=?1
             WHERE status='failed'",
            [Utc::now().timestamp()],
        )?)
    }

    pub fn dismiss_failed_notifications(&self) -> Result<usize> {
        Ok(self.connection.execute(
            "UPDATE notification_events SET status='dismissed' WHERE status='failed'",
            [],
        )?)
    }

    pub fn recent_delivery_errors(&self, limit: usize) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "SELECT transport || ': ' || error FROM delivery_attempts
             WHERE success=0 AND error IS NOT NULL ORDER BY attempted_at DESC,id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit as i64], |row| row.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn observations(&self) -> Result<Vec<AchievementObservation>> {
        let mut statement = self.connection.prepare(
            "SELECT source_id,game_id,achievement_id,achieved,current_progress,max_progress,
             unlock_time,display_name,description,icon,hidden,trophy_grade FROM observations ORDER BY game_id,achievement_id"
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AchievementObservation {
                source_id: row.get(0)?,
                origin_source_id: None,
                game_id: row.get(1)?,
                achievement_id: row.get(2)?,
                achieved: row.get::<_, i64>(3)? != 0,
                current_progress: row.get(4)?,
                max_progress: row.get(5)?,
                unlock_time: row.get(6)?,
                display_name: row.get(7)?,
                description: row.get(8)?,
                icon: row.get(9)?,
                hidden: row.get::<_, i64>(10)? != 0,
                global_percent_hundredths: None,
                trophy_grade: row.get(11)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn save_game_metadata(&self, game_id: &str, name: &str, icon: Option<&str>) -> Result<()> {
        self.connection.execute(
            "INSERT INTO games(game_id,name,icon) VALUES(?1,?2,?3)
             ON CONFLICT(game_id) DO UPDATE SET name=excluded.name,
             icon=COALESCE(excluded.icon,games.icon)",
            params![game_id, name, icon],
        )?;
        Ok(())
    }

    pub fn save_game_metadata_if_achievements(
        &self,
        game_id: &str,
        name: &str,
        icon: Option<&str>,
    ) -> Result<bool> {
        let has_achievements: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM observations WHERE game_id=?1)
             OR EXISTS(SELECT 1 FROM achievement_metadata WHERE game_id=?1)",
            [game_id],
            |row| row.get(0),
        )?;
        if has_achievements {
            self.save_game_metadata(game_id, name, icon)?;
        } else {
            self.connection
                .execute("DELETE FROM games WHERE game_id=?1", [game_id])?;
        }
        Ok(has_achievements)
    }

    pub fn canonical_game_id(&self, game_id: &str) -> Result<String> {
        Ok(self
            .connection
            .query_row(
                "SELECT canonical_game_id FROM game_aliases WHERE alias_game_id=?1",
                [game_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_else(|| game_id.to_string()))
    }

    pub fn save_game_alias(&self, alias: &str, canonical: &str) -> Result<()> {
        if alias == canonical {
            return Ok(());
        }
        self.connection.execute(
            "INSERT INTO game_aliases(alias_game_id,canonical_game_id) VALUES(?1,?2)
             ON CONFLICT(alias_game_id) DO UPDATE SET canonical_game_id=excluded.canonical_game_id",
            params![alias, canonical],
        )?;
        self.connection.execute(
            "UPDATE OR IGNORE observations SET game_id=?1 WHERE game_id=?2",
            params![canonical, alias],
        )?;
        self.connection
            .execute("DELETE FROM observations WHERE game_id=?1", [alias])?;
        Ok(())
    }

    pub fn game_metadata(&self, game_id: &str) -> Result<Option<(String, Option<String>)>> {
        Ok(self
            .connection
            .query_row(
                "SELECT name,icon FROM games WHERE game_id=?1",
                [game_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?)
    }

    pub fn clear_game_metadata(&self, game_id: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM achievement_metadata WHERE game_id=?1",
            [game_id],
        )?;
        self.connection
            .execute("DELETE FROM games WHERE game_id=?1", [game_id])?;
        Ok(())
    }

    pub fn game_activity(&self, game_id: &str) -> Result<(i64, i64)> {
        Ok(self
            .connection
            .query_row(
                "SELECT playtime_seconds,last_played FROM game_activity WHERE game_id=?1",
                [game_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .unwrap_or_default())
    }

    pub fn record_play_session(&self, game_id: &str, seconds: i64, last_played: i64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO game_activity(game_id,playtime_seconds,last_played) VALUES(?1,?2,?3)
             ON CONFLICT(game_id) DO UPDATE SET playtime_seconds=game_activity.playtime_seconds+excluded.playtime_seconds,
             last_played=MAX(game_activity.last_played,excluded.last_played)",
            params![game_id, seconds.max(0), last_played],
        )?;
        Ok(())
    }

    pub fn reset_game_activity(&self, game_id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM game_activity WHERE game_id=?1", [game_id])?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn save_achievement_metadata(
        &self,
        game_id: &str,
        achievement_id: &str,
        display_name: Option<&str>,
        description: Option<&str>,
        icon: Option<&str>,
        locked_icon: Option<&str>,
        hidden: bool,
    ) -> Result<()> {
        let updated = self.connection.execute(
            "UPDATE achievement_metadata SET
             display_name=COALESCE(?3,display_name),description=COALESCE(?4,description),
             icon=COALESCE(?5,icon),locked_icon=COALESCE(?6,locked_icon),hidden=?7
             WHERE game_id=?1 AND achievement_id=?2 COLLATE NOCASE",
            params![
                game_id,
                achievement_id,
                display_name,
                description,
                icon,
                locked_icon,
                hidden as i64
            ],
        )?;
        if updated == 0 {
            self.connection.execute(
            "INSERT INTO achievement_metadata(game_id,achievement_id,display_name,description,icon,locked_icon,hidden)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![game_id, achievement_id, display_name, description, icon, locked_icon, hidden as i64],
            )?;
        }
        Ok(())
    }

    pub fn save_global_percent(
        &self,
        game_id: &str,
        achievement_id: &str,
        hundredths: u32,
    ) -> Result<()> {
        let updated = self.connection.execute(
            "UPDATE achievement_metadata SET global_percent_hundredths=?3
             WHERE game_id=?1 AND achievement_id=?2 COLLATE NOCASE",
            params![game_id, achievement_id, hundredths],
        )?;
        if updated == 0 {
            self.connection.execute(
                "INSERT INTO achievement_metadata(game_id,achievement_id,global_percent_hundredths)
                 VALUES(?1,?2,?3)",
                params![game_id, achievement_id, hundredths],
            )?;
        }
        Ok(())
    }

    pub fn enrich_observations(&self, observations: &mut [AchievementObservation]) -> Result<()> {
        let mut statement = self.connection.prepare(
            "SELECT MAX(display_name),MAX(description),MAX(icon),MAX(locked_icon),COALESCE(MAX(hidden),0),MAX(global_percent_hundredths) FROM achievement_metadata
             WHERE game_id=?1 AND achievement_id=?2 COLLATE NOCASE",
        )?;
        for observation in observations {
            let metadata: Option<StoredAchievementMetadata> = statement
                .query_row(
                    params![observation.game_id, observation.achievement_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )
                .optional()?;
            if let Some((name, description, icon, locked_icon, hidden, global_percent)) = metadata {
                observation.display_name = observation.display_name.take().or(name);
                observation.description = observation.description.take().or(description);
                let state_icon = if observation.achieved {
                    icon.or(locked_icon)
                } else {
                    locked_icon.or(icon)
                };
                observation.icon = observation.icon.take().or(state_icon);
                observation.hidden |= hidden != 0;
                observation.global_percent_hundredths =
                    observation.global_percent_hundredths.or(global_percent);
            }
        }
        Ok(())
    }

    pub fn has_achievement_metadata(&self, game_id: &str) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM achievement_metadata WHERE game_id=?1)",
            [game_id],
            |row| row.get(0),
        )?)
    }

    pub fn has_global_percentages(&self, game_id: &str) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM achievement_metadata WHERE game_id=?1 AND global_percent_hundredths IS NOT NULL)",
            [game_id], |row| row.get(0),
        )?)
    }

    pub fn catalog_games(&self) -> Result<Vec<GameSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT games.game_id,games.name,games.icon,COUNT(DISTINCT achievement_metadata.achievement_id COLLATE NOCASE)
             FROM games LEFT JOIN achievement_metadata USING(game_id)
             GROUP BY games.game_id,games.name,games.icon ORDER BY games.name COLLATE NOCASE",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(GameSummary {
                source_id: "catalog".into(),
                source_kind: None,
                game_id: row.get(0)?,
                name: row.get(1)?,
                icon: row.get(2)?,
                unlocked: 0,
                total: row.get::<_, i64>(3)? as u32,
                platinum: 0,
                gold: 0,
                silver: 0,
                bronze: 0,
                last_unlock_time: 0,
                playtime_seconds: 0,
                last_played: 0,
                tracked: false,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn catalog_achievements(&self, game_id: &str) -> Result<Vec<AchievementObservation>> {
        let mut statement = self.connection.prepare(
            "SELECT MIN(achievement_id),MAX(display_name),MAX(description),COALESCE(MAX(locked_icon),MAX(icon)),COALESCE(MAX(hidden),0),MAX(global_percent_hundredths)
             FROM achievement_metadata WHERE game_id=?1 GROUP BY achievement_id COLLATE NOCASE
             ORDER BY MAX(display_name) COLLATE NOCASE,MIN(achievement_id)",
        )?;
        let rows = statement.query_map([game_id], |row| {
            Ok(AchievementObservation {
                source_id: "catalog".into(),
                origin_source_id: None,
                game_id: game_id.into(),
                achievement_id: row.get(0)?,
                achieved: false,
                hidden: row.get::<_, i64>(4)? != 0,
                global_percent_hundredths: row.get(5)?,
                trophy_grade: None,
                current_progress: 0,
                max_progress: 0,
                unlock_time: 0,
                display_name: row.get(1)?,
                description: row.get(2)?,
                icon: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn migration_report(&self, source_path: &str) -> Result<Option<MigrationReport>> {
        let report: Option<String> = self
            .connection
            .query_row(
                "SELECT report FROM migration_runs WHERE source_path=?1",
                [source_path],
                |row| row.get(0),
            )
            .optional()?;
        report
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }

    pub fn save_migration_report(&self, source_path: &str, report: &MigrationReport) -> Result<()> {
        self.connection.execute(
            "INSERT INTO migration_runs(source_path,imported_at,report) VALUES(?1,?2,?3)
             ON CONFLICT(source_path) DO UPDATE SET imported_at=excluded.imported_at,report=excluded.report",
            params![source_path, Utc::now().timestamp(), serde_json::to_string(report)?],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(achieved: bool, progress: i64) -> AchievementObservation {
        AchievementObservation {
            source_id: "steam".into(),
            origin_source_id: None,
            game_id: "400".into(),
            achievement_id: "WIN".into(),
            achieved,
            hidden: false,
            global_percent_hundredths: None,
            trophy_grade: None,
            current_progress: progress,
            max_progress: 10,
            unlock_time: if achieved { 100 } else { 0 },
            display_name: Some("Winner".into()),
            description: None,
            icon: None,
        }
    }

    #[test]
    fn baseline_is_silent_and_transition_is_durable_and_deduplicated() {
        let mut store = Store::open_memory().unwrap();
        assert!(
            store
                .record_observations(&[observation(false, 0)], true)
                .unwrap()
                .is_empty()
        );
        let events = store
            .record_observations(&[observation(true, 0)], false)
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(store.pending_events(i64::MAX, 10).unwrap().len(), 1);
        assert!(
            store
                .record_observations(&[observation(true, 0)], false)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn first_seen_live_unlock_is_not_lost() {
        let mut store = Store::open_memory().unwrap();
        let events = store
            .record_observations(&[observation(true, 0)], false)
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, NotificationKind::Unlock);

        let mut baseline_store = Store::open_memory().unwrap();
        assert!(
            baseline_store
                .record_observations(&[observation(true, 0)], true)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn failed_delivery_retries_and_success_completes_event() {
        let mut store = Store::open_memory().unwrap();
        store
            .record_observations(&[observation(false, 0)], true)
            .unwrap();
        let event = store
            .record_observations(&[observation(true, 0)], false)
            .unwrap()
            .remove(0);
        store
            .record_delivery(event.id, "overlay", Err("timeout"))
            .unwrap();
        assert!(store.pending_events(i64::MAX, 10).unwrap()[0].attempts > 0);
        store.record_delivery(event.id, "native", Ok(())).unwrap();
        assert!(store.pending_events(i64::MAX, 10).unwrap().is_empty());
    }

    #[test]
    fn failed_notifications_can_be_retried_or_dismissed() {
        let mut store = Store::open_memory().unwrap();
        store
            .record_observations(&[observation(false, 0)], true)
            .unwrap();
        let event = store
            .record_observations(&[observation(true, 0)], false)
            .unwrap()
            .remove(0);
        for _ in 0..8 {
            store
                .record_delivery(event.id, "overlay", Err("timeout"))
                .unwrap();
        }
        assert_eq!(store.notification_queue_counts().unwrap(), (0, 1));
        assert_eq!(store.retry_failed_notifications().unwrap(), 1);
        assert_eq!(store.notification_queue_counts().unwrap(), (1, 0));
        for _ in 0..8 {
            store
                .record_delivery(event.id, "overlay", Err("timeout"))
                .unwrap();
        }
        assert_eq!(store.dismiss_failed_notifications().unwrap(), 1);
        assert_eq!(store.notification_queue_counts().unwrap(), (0, 0));
    }

    #[test]
    fn play_sessions_accumulate_and_keep_latest_timestamp() {
        let store = Store::open_memory().unwrap();
        store.record_play_session("400", 40, 100).unwrap();
        store.record_play_session("400", 20, 90).unwrap();
        assert_eq!(store.game_activity("400").unwrap(), (60, 100));
        store.reset_game_activity("400").unwrap();
        assert_eq!(store.game_activity("400").unwrap(), (0, 0));
    }

    #[test]
    fn catalog_keeps_games_without_achievements() {
        let store = Store::open_memory().unwrap();
        store
            .save_game_metadata("400", "A Game Without Achievements", None)
            .unwrap();

        let games = store.catalog_games().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].game_id, "400");
        assert_eq!(games[0].total, 0);
        assert!(!games[0].tracked);
    }

    #[test]
    fn shortcut_metadata_requires_achievement_data() {
        let mut store = Store::open_memory().unwrap();
        store
            .save_game_metadata("900", "Empty Shortcut", None)
            .unwrap();
        assert!(
            !store
                .save_game_metadata_if_achievements("900", "Empty Shortcut", None)
                .unwrap()
        );
        assert!(store.catalog_games().unwrap().is_empty());

        let mut achievement = observation(false, 0);
        achievement.game_id = "901".into();
        store.record_observations(&[achievement], true).unwrap();
        assert!(
            store
                .save_game_metadata_if_achievements("901", "Useful Shortcut", None)
                .unwrap()
        );
        assert_eq!(store.catalog_games().unwrap()[0].name, "Useful Shortcut");
    }

    #[test]
    fn enrichment_uses_state_correct_achievement_artwork() {
        let store = Store::open_memory().unwrap();
        store
            .save_achievement_metadata(
                "400",
                "WIN",
                Some("Winner"),
                None,
                Some("unlocked.png"),
                Some("locked.png"),
                false,
            )
            .unwrap();
        let mut locked = vec![observation(false, 0)];
        let mut unlocked = vec![observation(true, 0)];
        store.enrich_observations(&mut locked).unwrap();
        store.enrich_observations(&mut unlocked).unwrap();
        assert_eq!(locked[0].icon.as_deref(), Some("locked.png"));
        assert_eq!(unlocked[0].icon.as_deref(), Some("unlocked.png"));
    }

    #[test]
    fn achievement_metadata_ids_are_case_insensitive() {
        let store = Store::open_memory().unwrap();
        store
            .save_achievement_metadata(
                "883710",
                "new_achievement_1_1",
                Some("Welcome to the City of the Dead"),
                None,
                Some("re2.png"),
                None,
                false,
            )
            .unwrap();
        store
            .save_global_percent("883710", "NEW_ACHIEVEMENT_1_1", 8_421)
            .unwrap();

        let mut observations = vec![observation(false, 0)];
        observations[0].game_id = "883710".into();
        observations[0].achievement_id = "NEW_ACHIEVEMENT_1_1".into();
        observations[0].display_name = None;
        observations[0].icon = None;
        store.enrich_observations(&mut observations).unwrap();

        assert_eq!(
            observations[0].display_name.as_deref(),
            Some("Welcome to the City of the Dead")
        );
        assert_eq!(observations[0].icon.as_deref(), Some("re2.png"));
        assert_eq!(observations[0].global_percent_hundredths, Some(8_421));
        assert_eq!(store.catalog_achievements("883710").unwrap().len(), 1);
    }

    #[test]
    fn game_aliases_remap_existing_and_future_source_observations() {
        let mut store = Store::open_memory().unwrap();
        let mut gog = observation(true, 0);
        gog.source_id = "gog".into();
        gog.game_id = "gog-release-42".into();
        store.record_observations(&[gog], true).unwrap();
        store.save_game_alias("gog-release-42", "400").unwrap();

        assert_eq!(store.canonical_game_id("gog-release-42").unwrap(), "400");
        let observations = store.observations().unwrap();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].game_id, "400");
    }
}
