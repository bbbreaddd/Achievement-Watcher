use crate::{AchievementObservation, Result};
use chrono::DateTime;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::{Path, PathBuf};

pub struct GalaxySnapshot {
    pub game_id: String,
    pub name: String,
    pub achievements: Vec<AchievementObservation>,
    pub artwork: Vec<GalaxyAchievementArtwork>,
}

pub struct GalaxyAchievementArtwork {
    pub achievement_id: String,
    pub locked: String,
    pub unlocked: String,
}

pub fn gameplay_databases(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .max_depth(5)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("gameplay.db")
        })
        .map(|entry| entry.into_path())
        .collect()
}

pub fn client_id_from_path(path: &Path) -> Option<String> {
    let components: Vec<_> = path
        .ancestors()
        .filter_map(|directory| directory.file_name()?.to_str())
        .collect();
    let gameplay = components
        .iter()
        .position(|component| component.eq_ignore_ascii_case("Gameplay"))?;
    components
        .get(gameplay + 1)
        .filter(|client_id| {
            client_id
                .chars()
                .all(|character| character.is_ascii_digit())
        })
        .map(|client_id| (*client_id).to_owned())
}

pub fn game_id_from_path(path: &Path) -> Option<String> {
    client_id_from_path(path).map(|client_id| format!("gog-galaxy-{client_id}"))
}

pub fn read_gameplay(
    path: &Path,
    source_id: &str,
    catalog_database: Option<&Path>,
) -> Result<GalaxySnapshot> {
    let client_id = client_id_from_path(path)
        .ok_or_else(|| crate::Error::Invalid("GOG Galaxy client ID is missing".into()))?;
    let game_id = format!("gog-galaxy-{client_id}");
    let name = catalog_database
        .and_then(|catalog| galaxy_title(catalog, &client_id).ok().flatten())
        .unwrap_or_else(|| format!("GOG game {client_id}"));
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let retrieved = connection
        .query_row(
            "SELECT value FROM database_info WHERE key='achievements_retrieved'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some_and(|value| value != "0");
    if !retrieved {
        return Err(crate::Error::Invalid(
            "GOG Galaxy has not finished retrieving achievements".into(),
        ));
    }
    let mut statement = connection.prepare(
        "SELECT key,name,description,visible_while_locked,unlock_time,
                image_url_locked,image_url_unlocked,rarity
         FROM achievement ORDER BY id",
    )?;
    let rows = statement
        .query_map([], |row| {
            let unlock_time = row.get::<_, Option<String>>(4)?.unwrap_or_default();
            let achieved = !unlock_time.trim().is_empty();
            let unlock_timestamp = DateTime::parse_from_rfc3339(&unlock_time)
                .map(|value| value.timestamp())
                .unwrap_or_default();
            let locked_icon = row.get::<_, String>(5)?;
            let unlocked_icon = row.get::<_, String>(6)?;
            let rarity = row.get::<_, f64>(7)?;
            let achievement_id: String = row.get(0)?;
            let observation = AchievementObservation {
                source_id: source_id.into(),
                origin_source_id: None,
                game_id: game_id.clone(),
                achievement_id: achievement_id.clone(),
                achieved,
                hidden: row.get::<_, i64>(3)? == 0,
                global_percent_hundredths: rarity
                    .is_finite()
                    .then_some((rarity.clamp(0.0, 100.0) * 100.0).round() as u32),
                trophy_grade: None,
                current_progress: i64::from(achieved),
                max_progress: 1,
                unlock_time: unlock_timestamp,
                display_name: Some(row.get(1)?),
                description: Some(row.get(2)?),
                icon: Some(if achieved {
                    unlocked_icon.clone()
                } else {
                    locked_icon.clone()
                }),
            };
            Ok((
                observation,
                GalaxyAchievementArtwork {
                    achievement_id,
                    locked: locked_icon,
                    unlocked: unlocked_icon,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (achievements, artwork) = rows.into_iter().unzip();
    Ok(GalaxySnapshot {
        game_id,
        name,
        achievements,
        artwork,
    })
}

fn galaxy_title(path: &Path, client_id: &str) -> Result<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    Ok(connection
        .query_row(
            "SELECT title FROM LimitedDetails
             WHERE productId=(SELECT productId FROM ProductAuthorizations
                              WHERE CAST(clientId AS TEXT)=?1 LIMIT 1)
               AND title IS NOT NULL AND TRIM(title)<>''
             ORDER BY (languageId=16) DESC, stored_at DESC LIMIT 1",
            [client_id],
            |row| row.get(0),
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_galaxy_schema_progress_artwork_and_rarity() {
        let directory = tempfile::tempdir().unwrap();
        let gameplay = directory
            .path()
            .join("Applications/12345/Gameplay/67890/gameplay.db");
        std::fs::create_dir_all(gameplay.parent().unwrap()).unwrap();
        let connection = Connection::open(&gameplay).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE database_info(key TEXT PRIMARY KEY,value TEXT NOT NULL);
                 INSERT INTO database_info VALUES('achievements_retrieved','1');
                 CREATE TABLE achievement(id INTEGER PRIMARY KEY,key TEXT,name TEXT,description TEXT,
                   visible_while_locked INTEGER,unlock_time TEXT,image_url_locked TEXT,
                   image_url_unlocked TEXT,rarity REAL);
                 INSERT INTO achievement VALUES
                   (1,'FIRST','First','Description',1,'2024-04-06T10:00:00Z','locked','unlocked',12.34),
                   (2,'SECRET','Secret','Hidden',0,NULL,'secret-locked','secret-unlocked',0.1);",
            )
            .unwrap();
        drop(connection);

        let catalog = directory.path().join("galaxy-2.0.db");
        let connection = Connection::open(&catalog).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE ProductAuthorizations(productId INTEGER,clientId INTEGER);
                 CREATE TABLE LimitedDetails(productId INTEGER,title TEXT,languageId INTEGER,stored_at INTEGER);
                 INSERT INTO ProductAuthorizations VALUES(42,12345);
                 INSERT INTO LimitedDetails VALUES(42,'Test game',16,1);",
            )
            .unwrap();
        drop(connection);

        let snapshot = read_gameplay(&gameplay, "galaxy", Some(&catalog)).unwrap();
        assert_eq!(snapshot.game_id, "gog-galaxy-12345");
        assert_eq!(snapshot.name, "Test game");
        assert_eq!(snapshot.achievements.len(), 2);
        assert!(snapshot.achievements[0].achieved);
        assert_eq!(snapshot.achievements[0].icon.as_deref(), Some("unlocked"));
        assert_eq!(snapshot.artwork[0].locked, "locked");
        assert_eq!(snapshot.artwork[0].unlocked, "unlocked");
        assert_eq!(
            snapshot.achievements[0].global_percent_hundredths,
            Some(1234)
        );
        assert!(snapshot.achievements[1].hidden);
        assert_eq!(
            snapshot.achievements[1].icon.as_deref(),
            Some("secret-locked")
        );
    }
}
