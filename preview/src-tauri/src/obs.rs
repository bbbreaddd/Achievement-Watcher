use aw_core::AppSettings;
use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

pub async fn save_replay(settings: &AppSettings) -> Result<PathBuf, String> {
    if !settings.obs_replay_enabled {
        return Err("OBS replay souvenirs are disabled".into());
    }
    let password = (!settings.obs_password.is_empty()).then_some(settings.obs_password.as_str());
    let client = tokio::time::timeout(
        Duration::from_secs(3),
        obws::Client::connect(&settings.obs_host, settings.obs_port, password),
    )
    .await
    .map_err(|_| "OBS WebSocket connection timed out".to_string())?
    .map_err(|error| format!("Could not connect to OBS WebSocket: {error}"))?;
    let status = client
        .replay_buffer()
        .status()
        .await
        .map_err(|error| format!("Could not read the OBS replay buffer: {error}"))?;
    if !status {
        if !settings.obs_start_replay_buffer {
            return Err("OBS replay buffer is not running".into());
        }
        client
            .replay_buffer()
            .start()
            .await
            .map_err(|error| format!("Could not start the OBS replay buffer: {error}"))?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let previous = client
        .replay_buffer()
        .last_replay()
        .await
        .ok()
        .and_then(replay_fingerprint);
    client
        .replay_buffer()
        .save()
        .await
        .map_err(|error| format!("Could not save the OBS replay buffer: {error}"))?;

    let mut stable_candidate = None;
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let Ok(saved) = client.replay_buffer().last_replay().await else {
            continue;
        };
        let Some(fingerprint) = replay_fingerprint(saved) else {
            continue;
        };
        if Some(&fingerprint) == previous.as_ref() {
            continue;
        }
        if stable_candidate.as_ref() == Some(&fingerprint) {
            return Ok(fingerprint.0);
        }
        stable_candidate = Some(fingerprint);
    }
    Err("OBS did not report a completed replay within 15 seconds".into())
}

fn replay_fingerprint(path: String) -> Option<(PathBuf, u64, SystemTime)> {
    let path = PathBuf::from(path);
    let metadata = std::fs::metadata(&path).ok()?;
    let size = metadata.len();
    (size > 0).then_some((path, size, metadata.modified().ok()?))
}
