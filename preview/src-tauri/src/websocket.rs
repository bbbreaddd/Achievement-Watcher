use aw_core::NotificationEvent;
use std::{
    io::ErrorKind,
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    time::Duration,
};
use tungstenite::{Message, WebSocket, accept};

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
    pub fn configure(&self, enabled: bool, host: &str, port: u16) -> Result<(), String> {
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

impl Drop for Bridge {
    fn drop(&mut self) {
        if let Ok(mut running) = self.running.lock()
            && let Some(running) = running.take()
        {
            running.stop.store(true, Ordering::Relaxed);
        }
    }
}
