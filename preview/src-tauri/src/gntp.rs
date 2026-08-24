use aw_core::NotificationEvent;
use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

pub fn send(host: &str, port: u16, event: &NotificationEvent) -> Result<(), String> {
    let address = (host, port)
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .next()
        .ok_or("GNTP host did not resolve")?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(400))
        .map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_millis(400)))
        .map_err(|error| error.to_string())?;
    let register = concat!(
        "GNTP/1.0 REGISTER NONE\r\n",
        "Application-Name: Achievement Watcher\r\n",
        "Notifications-Count: 1\r\n\r\n",
        "Notification-Name: Achievement\r\n",
        "Notification-Display-Name: Achievement\r\n",
        "Notification-Enabled: True\r\n\r\n"
    );
    stream
        .write_all(register.as_bytes())
        .map_err(|error| error.to_string())?;
    let mut response = [0_u8; 512];
    let count = stream
        .read(&mut response)
        .map_err(|error| error.to_string())?;
    if !String::from_utf8_lossy(&response[..count]).contains("-OK") {
        return Err("Growl rejected GNTP registration".into());
    }
    drop(stream);

    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(400))
        .map_err(|error| error.to_string())?;
    let title = clean(
        event
            .observation
            .display_name
            .as_deref()
            .unwrap_or(&event.observation.achievement_id),
    );
    let description = clean(
        event
            .observation
            .description
            .as_deref()
            .unwrap_or("Achievement unlocked"),
    );
    let notify = format!(
        "GNTP/1.0 NOTIFY NONE\r\nApplication-Name: Achievement Watcher\r\nNotification-Name: Achievement\r\nNotification-Title: {title}\r\nNotification-Text: {description}\r\n\r\n"
    );
    stream
        .write_all(notify.as_bytes())
        .map_err(|error| error.to_string())
}

fn clean(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}
