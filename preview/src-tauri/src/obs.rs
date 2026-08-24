use aw_core::AppSettings;
use std::time::Duration;

pub async fn save_replay(settings: &AppSettings) -> Result<(), String> {
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
    client
        .replay_buffer()
        .save()
        .await
        .map_err(|error| format!("Could not save the OBS replay buffer: {error}"))
}
