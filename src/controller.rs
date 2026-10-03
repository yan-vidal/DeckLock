//! Optional sc-controller Unix protocol client. Never captures on connection.
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::time::Duration;

    #[test]
    fn fragmented_events_and_explicit_capture_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.socket");
        let listener = UnixListener::bind(&path).unwrap();
        let client = ControllerClient::connect(path).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        peer.write_all(b"Controller: test deck 0 None\nReady.\n")
            .unwrap();
        assert_eq!(
            client.events.recv_timeout(Duration::from_secs(2)).unwrap(),
            ControllerEvent::Connected
        );
        let mut buf = [0; 1024];
        assert!(
            peer.read(&mut buf).is_err(),
            "must not capture automatically"
        );
        client.set_active(true).unwrap();
        use std::io::{BufRead, BufReader};
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut reader = BufReader::new(peer.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line, "Controller: test\n");
        peer.write_all(b"OK.\n").unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert!(line.starts_with("Lock: "));
        peer.write_all(b"OK.\nEvent: test LP").unwrap();
        peer.write_all(b"AD 12 -34\nEvent: test RT 255 0\n")
            .unwrap();
        assert_eq!(
            client.events.recv_timeout(Duration::from_secs(2)).unwrap(),
            ControllerEvent::Pad {
                side: Side::Left,
                x: 12,
                y: -34
            }
        );
        assert_eq!(
            client.events.recv_timeout(Duration::from_secs(2)).unwrap(),
            ControllerEvent::Trigger {
                side: Side::Right,
                value: 255
            }
        );
        drop(client);
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line, "Unlock.\n");
    }

    #[test]
    fn parser_rejects_bad_input_and_preserves_touch_release() {
        assert_eq!(
            parse_event("Event: c LPADTOUCH 0", "c"),
            Some(ControllerEvent::Button {
                name: "LPADTOUCH".into(),
                pressed: false
            })
        );
        assert!(parse_event("Event: other LPAD 1 2", "c").is_none());
        assert!(parse_event("Event: c LPAD nan 2", "c").is_none());
        assert!(parse_event("Event: c LPAD 9999999 2", "c").is_none());
    }

    #[test]
    fn daemon_restart_recaptures_only_while_keyboard_remains_active() {
        use std::io::{BufRead, BufReader};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.socket");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let accept = || {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            loop {
                match listener.accept() {
                    Ok((peer, _)) => {
                        peer.set_read_timeout(Some(Duration::from_millis(250)))
                            .unwrap();
                        return peer;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "client did not reconnect"
                        );
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("{error}"),
                }
            }
        };
        let client = ControllerClient::connect(path).unwrap();
        for id in ["before", "after"] {
            let mut peer = accept();
            peer.write_all(format!("Controller: {id} deck 0 None\nReady.\n").as_bytes())
                .unwrap();
            assert_eq!(
                client.events.recv_timeout(Duration::from_secs(2)).unwrap(),
                ControllerEvent::Connected
            );
            if id == "before" {
                client.set_active(true).unwrap();
            }
            let mut reader = BufReader::new(peer.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert_eq!(line, format!("Controller: {id}\n"));
            peer.write_all(b"OK.\n").unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            assert!(line.starts_with("Lock: "));
            peer.write_all(
                format!("OK.\nEvent: {id} LPADTOUCH 1\nEvent: {id} RPADTOUCH 1\n").as_bytes(),
            )
            .unwrap();
            for name in ["LPADTOUCH", "RPADTOUCH"] {
                assert_eq!(
                    client.events.recv_timeout(Duration::from_secs(2)).unwrap(),
                    ControllerEvent::Button {
                        name: name.into(),
                        pressed: true
                    }
                );
            }
            if id == "after" {
                client.set_active(false).unwrap();
                line.clear();
                reader.read_line(&mut line).unwrap();
                assert_eq!(line, "Unlock.\n");
            }
            drop(reader);
            drop(peer);
            assert_eq!(
                client.events.recv_timeout(Duration::from_secs(2)).unwrap(),
                ControllerEvent::Disconnected
            );
        }
        let mut peer = accept();
        peer.write_all(b"Controller: hidden deck 0 None\nReady.\n")
            .unwrap();
        assert_eq!(
            client.events.recv_timeout(Duration::from_secs(2)).unwrap(),
            ControllerEvent::Connected
        );
        assert!(
            peer.read(&mut [0]).is_err(),
            "hidden keyboard must not recapture"
        );
    }

    #[test]
    fn oversized_daemon_line_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.socket");
        let listener = UnixListener::bind(&path).unwrap();
        let client = ControllerClient::connect(path).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        peer.write_all(&vec![b'x'; MAX_LINE + 1]).unwrap();
        assert!(matches!(
            client.events.recv_timeout(Duration::from_secs(2)).unwrap(),
            ControllerEvent::Error(_)
        ));
    }
}

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const MAX_LINE: usize = 8192;
const SOURCES: &str = "LPAD RPAD LPADTOUCH RPADTOUCH LPADPRESS RPADPRESS LT RT A B X Y C LB RB LGRIP RGRIP LGRIP2 RGRIP2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControllerEvent {
    Connected,
    Disconnected,
    Pad { side: Side, x: i32, y: i32 },
    Button { name: String, pressed: bool },
    Trigger { side: Side, value: i32 },
    Error(String),
}

enum Command {
    Active(bool),
    Stop,
}

pub struct ControllerClient {
    pub events: Receiver<ControllerEvent>,
    commands: Sender<Command>,
    worker: Option<JoinHandle<()>>,
}

impl ControllerClient {
    pub fn connect(path: PathBuf) -> Result<Self, String> {
        let address = socket2::SockAddr::unix(path).map_err(|e| e.to_string())?;
        let (commands, rx) = mpsc::channel();
        let (tx, events) = mpsc::sync_channel(256);
        let worker = thread::spawn(move || {
            let mut active = false;
            let mut unavailable = false;
            loop {
                while let Ok(command) = rx.try_recv() {
                    match command {
                        Command::Active(value) => active = value,
                        Command::Stop => return,
                    }
                }
                let mut connected = false;
                let result = (|| {
                    let socket =
                        socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None)?;
                    socket.connect_timeout(&address, Duration::from_secs(1))?;
                    let fd: std::os::fd::OwnedFd = socket.into();
                    let stream = UnixStream::from(fd);
                    stream.set_read_timeout(Some(Duration::from_millis(50)))?;
                    stream.set_write_timeout(Some(Duration::from_millis(100)))?;
                    connected = true;
                    run(stream, &rx, &tx, &mut active)
                })();
                if matches!(result, Ok(true)) {
                    return;
                }
                let fatal = result
                    .as_ref()
                    .is_err_and(|e| e.kind() == io::ErrorKind::InvalidData);
                if connected || !unavailable {
                    if let Err(error) = result {
                        let _ = tx.try_send(ControllerEvent::Error(error.to_string()));
                    }
                    let _ = tx.try_send(ControllerEvent::Disconnected);
                }
                if fatal {
                    return;
                }
                unavailable = true;
                // Retry transport loss after daemon restart, but never override a
                // protocol refusal or keep re-capturing after an overflowing UI queue.
                match rx.recv_timeout(Duration::from_millis(250)) {
                    Ok(Command::Active(value)) => active = value,
                    Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        });
        Ok(Self {
            events,
            commands,
            worker: Some(worker),
        })
    }

    pub fn set_active(&self, active: bool) -> Result<(), String> {
        self.commands
            .send(Command::Active(active))
            .map_err(|e| e.to_string())
    }
}

impl Drop for ControllerClient {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            // Never wait for network I/O on the GTK thread. The bounded worker
            // observes Stop, releases the capture and exits independently.
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}

fn send(stream: &mut UnixStream, message: &str) -> io::Result<()> {
    stream.write_all(message.as_bytes())
}

fn run(
    mut stream: UnixStream,
    commands: &Receiver<Command>,
    events: &SyncSender<ControllerEvent>,
    active: &mut bool,
) -> io::Result<bool> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 2048];
    let mut controller: Option<String> = None;
    let mut ready = false;
    let mut captured = false;
    let mut selecting = false;
    let mut capture_pending = false;
    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Active(value) => *active = value,
                Command::Stop => {
                    if captured {
                        send(&mut stream, "Unlock.\n")?;
                    }
                    return Ok(true);
                }
            }
        }
        if captured && !*active {
            send(&mut stream, "Unlock.\n")?;
            captured = false;
        }
        if ready
            && *active
            && !captured
            && !selecting
            && !capture_pending
            && let Some(id) = &controller
        {
            send(&mut stream, &format!("Controller: {id}\n"))?;
            selecting = true;
        }
        let size = match stream.read(&mut chunk) {
            Ok(0) => return Ok(false),
            Ok(n) => n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e),
        };
        buffer.extend_from_slice(&chunk[..size]);
        while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
            if end > MAX_LINE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "controller protocol line too long",
                ));
            }
            let line = std::str::from_utf8(&buffer[..end])
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid controller UTF-8")
                })?
                .to_owned();
            buffer.drain(..=end);
            if let Some(body) = line.strip_prefix("Controller:") {
                let id = body.split_whitespace().next().unwrap_or_default();
                if controller.is_none() && !id.is_empty() {
                    controller = Some(id.to_owned());
                }
            } else if line == "OK." && selecting {
                selecting = false;
                if *active {
                    send(&mut stream, &format!("Lock: {SOURCES}\n"))?;
                    capture_pending = true;
                }
            } else if line == "OK." && capture_pending {
                capture_pending = false;
                captured = true;
            } else if line == "Ready." {
                ready = true;
                let _ = events.try_send(ControllerEvent::Connected);
            } else if line == "Controller Count: 0" {
                captured = false;
                controller = None;
                let _ = events.try_send(ControllerEvent::Disconnected);
            } else if line.starts_with("Fail:") || line.starts_with("Error:") {
                // Refusal must not cause a retry loop or displace another client.
                if captured {
                    send(&mut stream, "Unlock.\n")?;
                }
                captured = false;
                *active = false;
                selecting = false;
                capture_pending = false;
                let _ = events.try_send(ControllerEvent::Error(line));
            } else if captured
                && let Some(id) = &controller
                && let Some(event) = parse_event(&line, id)
            {
                // A stalled UI cannot let input accumulate without bounds. Terminating
                // the connection releases capture, avoiding lost-release stuck keys.
                events.try_send(event).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "controller event queue overflow",
                    )
                })?;
            }
        }
        if buffer.len() > MAX_LINE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "controller protocol line too long",
            ));
        }
    }
}

fn parse_event(line: &str, id: &str) -> Option<ControllerEvent> {
    let mut fields = line.strip_prefix("Event:")?.split_whitespace();
    if fields.next()? != id {
        return None;
    }
    let name = fields.next()?;
    let number = |s: &str| {
        let value = s.parse::<f64>().ok()?;
        (value.is_finite() && (-32768.0..=32767.0).contains(&value)).then_some(value as i32)
    };
    let value = number(fields.next()?)?;
    match name {
        "LPAD" | "RPAD" => Some(ControllerEvent::Pad {
            side: if name == "LPAD" {
                Side::Left
            } else {
                Side::Right
            },
            x: value,
            y: number(fields.next()?)?,
        }),
        "LT" | "RT" if (0..=255).contains(&value) => Some(ControllerEvent::Trigger {
            side: if name == "LT" {
                Side::Left
            } else {
                Side::Right
            },
            value,
        }),
        name if SOURCES.split_whitespace().any(|source| source == name)
            && (value == 0 || value == 1) =>
        {
            Some(ControllerEvent::Button {
                name: name.into(),
                pressed: value == 1,
            })
        }
        _ => None,
    }
}
