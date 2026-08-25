use aw_core::{AchievementObservation, NotificationEvent, Store, merge_observations};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    io::ErrorKind,
    net::{IpAddr, TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    time::Duration,
};
use tungstenite::{Error as WebSocketError, Message, WebSocket, accept};

const PROTOCOL_VERSION: u8 = 1;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    protocol_version: u8,
    r#type: String,
    request_id: String,
    app_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GameSnapshot {
    app_id: String,
    game_id: String,
    name: String,
    unlocked: usize,
    total: usize,
    achievements: Vec<AchievementObservation>,
}

struct Running {
    sender: SyncSender<String>,
    stop: Arc<AtomicBool>,
    address: String,
}

#[derive(Default)]
pub struct Bridge {
    running: Mutex<Option<Running>>,
}

impl Bridge {
    pub fn configure(
        &self,
        enabled: bool,
        host: &str,
        port: u16,
        data_dir: &Path,
    ) -> Result<(), String> {
        if enabled {
            validate_address(host, port)?;
        }
        let mut running = self.running.lock().map_err(|error| error.to_string())?;
        let address = format!("{host}:{port}");
        if enabled
            && running
                .as_ref()
                .is_some_and(|current| current.address == address)
        {
            return Ok(());
        }
        if let Some(previous) = running.take() {
            previous.stop.store(true, Ordering::Relaxed);
        }
        if !enabled {
            return Ok(());
        }
        let listener = TcpListener::bind((host, port)).map_err(|error| error.to_string())?;
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let (sender, receiver) = mpsc::sync_channel::<String>(64);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let data_dir = data_dir.to_owned();
        std::thread::spawn(move || {
            let mut clients: Vec<WebSocket<TcpStream>> = Vec::new();
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                        if let Ok(socket) = accept(stream) {
                            let _ = socket.get_ref().set_nonblocking(true);
                            clients.push(socket);
                        }
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                    Err(_) => break,
                }
                while let Ok(payload) = receiver.try_recv() {
                    clients.retain_mut(|client| {
                        client.send(Message::Text(payload.clone().into())).is_ok()
                    });
                }
                clients.retain_mut(|client| read_requests(client, &data_dir));
                std::thread::sleep(Duration::from_millis(50));
            }
            for mut client in clients {
                let _ = client.close(None);
            }
        });
        *running = Some(Running {
            sender,
            stop,
            address,
        });
        Ok(())
    }

    pub fn broadcast(
        &self,
        event: &NotificationEvent,
        game_name: Option<&str>,
    ) -> Result<(), String> {
        let running = self.running.lock().map_err(|error| error.to_string())?;
        let Some(running) = running.as_ref() else {
            return Ok(());
        };
        let payload = serde_json::json!({
            "appID": event.observation.game_id,
            "game": game_name.unwrap_or(&event.observation.game_id),
            "achievement": event.observation.achievement_id,
            "displayName": event.observation.display_name,
            "description": event.observation.description,
            "icon": event.observation.icon,
            "time": event.observation.unlock_time.saturating_mul(1000),
            "source": event.observation.source_id,
            "kind": event.kind.as_str(),
        });
        running
            .sender
            .try_send(payload.to_string())
            .map_err(|error| error.to_string())
    }
}

fn read_requests(client: &mut WebSocket<TcpStream>, data_dir: &Path) -> bool {
    loop {
        match client.read() {
            Ok(Message::Text(payload)) => {
                if let Some(response) = request_response(&payload, data_dir)
                    && client.send(Message::Text(response.into())).is_err()
                {
                    return false;
                }
            }
            Ok(Message::Ping(payload)) => {
                if client.send(Message::Pong(payload)).is_err() {
                    return false;
                }
            }
            Ok(Message::Close(_)) => return false,
            Ok(_) => {}
            Err(WebSocketError::Io(error)) if error.kind() == ErrorKind::WouldBlock => return true,
            Err(WebSocketError::ConnectionClosed | WebSocketError::AlreadyClosed) => return false,
            Err(_) => return false,
        }
    }
}

fn request_response(payload: &str, data_dir: &Path) -> Option<String> {
    let request: Request = serde_json::from_str(payload).ok()?;
    if request.protocol_version != PROTOCOL_VERSION || request.r#type != "getGame" {
        return None;
    }
    let (game, error) = match game_snapshot(data_dir, &request.app_id) {
        Ok(game) => (game, None),
        Err(error) => (None, Some(error)),
    };
    Some(
        serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "type": "game",
            "requestId": request.request_id,
            "game": game,
            "error": error,
        })
        .to_string(),
    )
}

fn game_snapshot(data_dir: &Path, app_id: &str) -> Result<Option<GameSnapshot>, String> {
    let store = Store::open(data_dir.join("achievement-watcher.sqlite3"))
        .map_err(|error| error.to_string())?;
    let game_id = store
        .canonical_game_id(app_id)
        .map_err(|error| error.to_string())?;
    let mut achievements = merge_observations(
        store
            .observations()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|item| item.game_id == game_id)
            .collect(),
        false,
        &BTreeMap::new(),
    );
    let existing: HashSet<_> = achievements
        .iter()
        .map(|item| item.achievement_id.to_ascii_lowercase())
        .collect();
    if store
        .has_achievement_metadata(&game_id)
        .map_err(|error| error.to_string())?
    {
        achievements.extend(
            store
                .catalog_achievements(&game_id)
                .map_err(|error| error.to_string())?
                .into_iter()
                .filter(|item| !existing.contains(&item.achievement_id.to_ascii_lowercase())),
        );
    }
    if achievements.is_empty() {
        return Ok(None);
    }
    store
        .enrich_observations(&mut achievements)
        .map_err(|error| error.to_string())?;
    achievements.sort_by(|left, right| {
        right.achieved.cmp(&left.achieved).then_with(|| {
            left.display_name
                .as_ref()
                .unwrap_or(&left.achievement_id)
                .cmp(right.display_name.as_ref().unwrap_or(&right.achievement_id))
        })
    });
    let name = store
        .game_metadata(&game_id)
        .map_err(|error| error.to_string())?
        .map(|(name, _)| name)
        .unwrap_or_else(|| game_id.clone());
    Ok(Some(GameSnapshot {
        app_id: app_id.to_owned(),
        game_id,
        name,
        unlocked: achievements.iter().filter(|item| item.achieved).count(),
        total: achievements.len(),
        achievements,
    }))
}

pub fn validate_address(host: &str, port: u16) -> Result<(), String> {
    if !is_loopback_host(host) {
        return Err("The WebSocket listener must use localhost or a loopback address".into());
    }
    if port == 0 {
        return Err("The WebSocket listener port must be between 1 and 65535".into());
    }
    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

impl Drop for Bridge {
    fn drop(&mut self) {
        if let Ok(mut running) = self.running.lock()
            && let Some(running) = running.take()
        {
            running.stop.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{game_snapshot, is_loopback_host, request_response};
    use aw_core::AchievementObservation;
    use std::fs;

    #[test]
    fn accepts_only_local_websocket_hosts() {
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("[::1]"));
        assert!(!is_loopback_host("0.0.0.0"));
        assert!(!is_loopback_host("192.168.1.10"));
    }

    #[test]
    fn returns_a_game_snapshot_for_decky() {
        let directory = std::env::temp_dir().join(format!(
            "achievement-watcher-decky-bridge-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let mut store =
            aw_core::Store::open(directory.join("achievement-watcher.sqlite3")).unwrap();
        store.save_game_metadata("42", "Test Game", None).unwrap();
        store
            .record_observations(
                &[AchievementObservation {
                    source_id: "test".into(),
                    origin_source_id: None,
                    game_id: "42".into(),
                    achievement_id: "FIRST".into(),
                    achieved: true,
                    hidden: false,
                    global_percent_hundredths: Some(5_000),
                    trophy_grade: None,
                    current_progress: 0,
                    max_progress: 0,
                    unlock_time: 1,
                    display_name: Some("First".into()),
                    description: None,
                    icon: None,
                }],
                true,
            )
            .unwrap();
        drop(store);
        assert!(game_snapshot(&directory, "42").unwrap().is_some());

        let response = request_response(
            r#"{"protocolVersion":1,"type":"getGame","requestId":"one","appId":"42"}"#,
            &directory,
        )
        .unwrap();
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(response["game"]["name"], "Test Game");
        assert_eq!(response["game"]["unlocked"], 1);
        assert_eq!(response["game"]["total"], 1);

        fs::remove_dir_all(directory).unwrap();
    }
}
