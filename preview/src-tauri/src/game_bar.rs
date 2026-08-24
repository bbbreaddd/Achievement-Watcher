use aw_core::NotificationEvent;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[cfg(windows)]
const PIPE_NAME: &str = r"\\.\pipe\LOCAL\AchievementWatcher.GameBar.v1";

#[derive(Debug)]
#[cfg_attr(not(windows), allow(dead_code))]
struct Request {
    token: String,
    event: NotificationEvent,
    expires_at: Instant,
    receipt: mpsc::SyncSender<Result<(), String>>,
}

#[derive(Clone)]
pub struct GameBarBridge {
    sender: mpsc::SyncSender<Request>,
    connected: Arc<AtomicBool>,
}

impl GameBarBridge {
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::sync_channel(8);
        let connected = Arc::new(AtomicBool::new(false));
        let server_connected = Arc::clone(&connected);
        std::thread::Builder::new()
            .name("game-bar-bridge".into())
            .spawn(move || server(receiver, server_connected))
            .expect("failed to start Game Bar bridge");
        Self { sender, connected }
    }

    pub fn deliver(&self, token: &str, event: &NotificationEvent) -> Result<(), String> {
        if !self.connected.load(Ordering::Acquire) {
            return Err("Game Bar companion is unavailable".into());
        }
        let (receipt, response) = mpsc::sync_channel(1);
        self.sender
            .try_send(Request {
                token: token.into(),
                event: event.clone(),
                expires_at: Instant::now() + Duration::from_millis(700),
                receipt,
            })
            .map_err(|_| "Game Bar companion is unavailable".to_string())?;
        response
            .recv_timeout(Duration::from_millis(750))
            .map_err(|_| "Game Bar companion did not acknowledge delivery".to_string())?
    }
}

#[cfg(not(windows))]
fn server(receiver: mpsc::Receiver<Request>, _connected: Arc<AtomicBool>) {
    for request in receiver {
        let _ = request
            .receipt
            .send(Err("Game Bar is available only on Windows".into()));
    }
}

#[cfg(windows)]
fn server(receiver: mpsc::Receiver<Request>, connected_state: Arc<AtomicBool>) {
    use std::{ffi::c_void, ptr};
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
            LocalFree,
        },
        Security::{
            Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::{PIPE_ACCESS_DUPLEX, ReadFile, WriteFile},
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
            PIPE_TYPE_MESSAGE, PIPE_WAIT,
        },
    };

    let pipe_name = wide(PIPE_NAME);
    let sddl = wide("D:(A;;GRGW;;;WD)");
    loop {
        let mut descriptor: *mut c_void = ptr::null_mut();
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            )
        };
        if converted == 0 {
            fail_pending(
                &receiver,
                "Could not create Game Bar pipe security descriptor",
            );
            return;
        }
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let pipe = unsafe {
            CreateNamedPipeW(
                pipe_name.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                1,
                65_536,
                65_536,
                0,
                &mut attributes,
            )
        };
        unsafe { LocalFree(descriptor.cast()) };
        if pipe == INVALID_HANDLE_VALUE {
            fail_pending(&receiver, "Could not create Game Bar named pipe");
            return;
        }
        let connected = unsafe { ConnectNamedPipe(pipe, ptr::null_mut()) } != 0
            || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if connected {
            connected_state.store(true, Ordering::Release);
            serve_client(pipe, &receiver);
            connected_state.store(false, Ordering::Release);
        }
        unsafe {
            DisconnectNamedPipe(pipe);
            CloseHandle(pipe);
        }
    }

    fn serve_client(pipe: HANDLE, receiver: &mpsc::Receiver<Request>) {
        let Some(hello) = read_message(pipe) else {
            return;
        };
        let token = serde_json::from_slice::<serde_json::Value>(&hello)
            .ok()
            .and_then(|value| value.get("token")?.as_str().map(str::to_owned));
        for request in receiver {
            if Instant::now() >= request.expires_at {
                let _ = request.receipt.send(Err("Game Bar request expired".into()));
                continue;
            }
            if token.as_deref() != Some(&request.token) {
                let _ = request
                    .receipt
                    .send(Err("Game Bar pairing token rejected".into()));
                return;
            }
            let payload = match serde_json::to_vec(&request.event) {
                Ok(payload) => payload,
                Err(error) => {
                    let _ = request.receipt.send(Err(error.to_string()));
                    continue;
                }
            };
            if !write_message(pipe, &payload) {
                let _ = request
                    .receipt
                    .send(Err("Game Bar pipe disconnected".into()));
                return;
            }
            let success = read_message(pipe)
                .and_then(|reply| serde_json::from_slice::<serde_json::Value>(&reply).ok())
                .and_then(|reply| reply.get("success")?.as_bool())
                .unwrap_or(false);
            let _ = request.receipt.send(if success {
                Ok(())
            } else {
                Err("Game Bar rejected the notification".into())
            });
        }
    }

    fn read_message(pipe: HANDLE) -> Option<Vec<u8>> {
        let mut buffer = vec![0_u8; 65_536];
        let mut read = 0;
        let ok = unsafe {
            ReadFile(
                pipe,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        } != 0;
        buffer.truncate(read as usize);
        ok.then_some(buffer)
    }

    fn write_message(pipe: HANDLE, payload: &[u8]) -> bool {
        let mut written = 0;
        (unsafe {
            WriteFile(
                pipe,
                payload.as_ptr(),
                payload.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        }) != 0
            && written as usize == payload.len()
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn fail_pending(receiver: &mpsc::Receiver<Request>, message: &str) {
        while let Ok(request) = receiver.recv() {
            let _ = request.receipt.send(Err(message.into()));
        }
    }
}
