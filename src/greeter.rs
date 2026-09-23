//! Native greetd greeter and session login support.
use std::{
    fs,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::Duration,
};
use zeroize::Zeroizing;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserEntry {
    pub username: String,
    pub display_name: String,
    pub uid: u32,
    pub icon_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopSession {
    pub id: String,
    pub name: String,
    pub exec: Vec<String>,
}

/// Discover regular user accounts available on the system.
pub fn list_system_users() -> Vec<UserEntry> {
    parse_users_from_passwd("/etc/passwd")
}

pub fn parse_users_from_passwd<P: AsRef<Path>>(path: P) -> Vec<UserEntry> {
    let Ok(content) = fs::read_to_string(path) else {
        return vec![UserEntry {
            username: "user".into(),
            display_name: "User".into(),
            uid: 1000,
            icon_path: None,
        }];
    };

    let mut users = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 7 {
            continue;
        }
        let username = parts[0];
        let Ok(uid) = parts[2].parse::<u32>() else {
            continue;
        };
        let gecos = parts[4].split(',').next().unwrap_or("").trim();
        let home = Path::new(parts[5]);
        let shell = parts[6].trim();

        // Regular desktop users are typically in UID 1000..60000 with interactive shells.
        if (1000..60000).contains(&uid) && !shell.ends_with("nologin") && !shell.ends_with("false")
        {
            let display_name = if !gecos.is_empty() {
                gecos.to_string()
            } else {
                username.to_string()
            };

            let mut icon_path = None;
            for candidate in [
                home.join(".face"),
                home.join(".face.icon"),
                PathBuf::from(format!("/var/lib/AccountsService/icons/{username}")),
            ] {
                if candidate.is_file() {
                    icon_path = Some(candidate);
                    break;
                }
            }

            users.push(UserEntry {
                username: username.to_string(),
                display_name,
                uid,
                icon_path,
            });
        }
    }

    if users.is_empty() {
        users.push(UserEntry {
            username: "user".into(),
            display_name: "User".into(),
            uid: 1000,
            icon_path: None,
        });
    }

    users
}

/// Discover Wayland sessions launchable by greetd's bare-VT greeter.
pub fn list_desktop_sessions() -> Vec<DesktopSession> {
    let mut sessions = Vec::new();
    let data_dirs: Vec<PathBuf> = std::env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });

    for dir in data_dirs.iter().map(|root| root.join("wayland-sessions")) {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "desktop")
                && let Some(session) = parse_session_file(&path)
                && !sessions.iter().any(|s: &DesktopSession| s.id == session.id)
            {
                sessions.push(session);
            }
        }
    }

    sessions.sort_by(|a, b| a.id.cmp(&b.id));
    sessions
}

fn parse_session_file(path: &Path) -> Option<DesktopSession> {
    let content = fs::read_to_string(path).ok()?;
    let id = path.file_stem()?.to_string_lossy().to_string();
    let mut name = None;
    let mut exec = None;
    let mut main_group = false;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            main_group = line == "[Desktop Entry]";
            continue;
        }
        if !main_group {
            continue;
        }
        if let Some(val) = line.strip_prefix("Name=")
            && name.is_none()
        {
            name = Some(val.trim().to_string());
        } else if let Some(val) = line.strip_prefix("Exec=")
            && exec.is_none()
        {
            exec = Some(val.trim().to_string());
        }
    }

    let name = name.unwrap_or_else(|| id.clone());
    let exec_str = exec?;
    // Session entries normally use a simple Exec command. Parse quotes as
    // arguments and refuse field codes that require a desktop-file launcher.
    if exec_str.contains('%') {
        return None;
    }
    let exec: Vec<String> = glib::shell_parse_argv(&exec_str)
        .ok()?
        .into_iter()
        .map(|arg| arg.into_string().ok())
        .collect::<Option<_>>()?;
    if exec.is_empty() {
        return None;
    }

    Some(DesktopSession { id, name, exec })
}

/// Persistent state across greeter restarts (e.g. remember last user and session).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct GreeterState {
    pub last_user: Option<String>,
    pub last_session: Option<String>,
}

impl GreeterState {
    pub fn state_file_path() -> PathBuf {
        if let Some(override_path) = std::env::var_os("DECKLOCK_GREETER_STATE") {
            return PathBuf::from(override_path);
        }
        if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
            return PathBuf::from(state_home).join("decklock/greeter-state.toml");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".local/state/decklock/greeter-state.toml");
        }
        std::env::temp_dir().join("decklock-greeter-state.toml")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::state_file_path())
    }

    pub fn load_from(path: &Path) -> Self {
        if let Ok(content) = fs::read_to_string(path) {
            toml::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        self.save_to(&Self::state_file_path());
    }

    pub fn save_to(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(content) = toml::to_string_pretty(self) {
            let _ = fs::write(path, content);
        }
    }
}

pub fn save_last_selection(user: &str, session_id: &str) {
    let mut state = GreeterState::load();
    state.last_user = Some(user.to_string());
    state.last_session = Some(session_id.to_string());
    state.save();
}

/// Greetd IPC request/response types according to greetd-ipc specification.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GreetdResponse {
    Success,
    Error {
        error_type: String,
        description: String,
    },
    AuthMessage {
        auth_message_type: String,
        auth_message: String,
    },
}

pub struct GreetdClient {
    stream: UnixStream,
}

impl GreetdClient {
    pub fn connect() -> Result<Self, String> {
        let socket_path = std::env::var("GREETD_SOCK")
            .map_err(|_| "GREETD_SOCK environment variable not set".to_string())?;
        Self::connect_path(&socket_path)
    }

    pub fn connect_path<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let stream = UnixStream::connect(path)
            .map_err(|e| format!("Failed to connect to greetd socket: {e}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .map_err(|e| format!("Failed to set greetd read timeout: {e}"))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(20)))
            .map_err(|e| format!("Failed to set greetd write timeout: {e}"))?;
        Ok(Self { stream })
    }

    fn send_json(&mut self, json: &str) -> Result<(), String> {
        let bytes = json.as_bytes();
        let len = bytes.len() as u32;
        self.stream
            .write_all(&len.to_ne_bytes())
            .map_err(|e| format!("Failed to write length: {e}"))?;
        self.stream
            .write_all(bytes)
            .map_err(|e| format!("Failed to write payload: {e}"))?;
        self.stream
            .flush()
            .map_err(|e| format!("Failed to flush stream: {e}"))?;
        Ok(())
    }

    fn read_json(&mut self) -> Result<String, String> {
        let mut len_bytes = [0u8; 4];
        self.stream
            .read_exact(&mut len_bytes)
            .map_err(|e| format!("Failed to read message length: {e}"))?;
        let len = u32::from_ne_bytes(len_bytes) as usize;
        if len > 65536 {
            return Err(format!("Message too large: {len} bytes"));
        }
        let mut payload = vec![0u8; len];
        self.stream
            .read_exact(&mut payload)
            .map_err(|e| format!("Failed to read message payload: {e}"))?;
        String::from_utf8(payload).map_err(|e| format!("Invalid UTF-8 from greetd: {e}"))
    }

    pub fn send_request(&mut self, json: &str) -> Result<GreetdResponse, String> {
        self.send_json(json)?;
        let response_str = self.read_json()?;
        parse_greetd_response(&response_str)
    }

    pub fn create_session(&mut self, username: &str) -> Result<GreetdResponse, String> {
        self.send_request(
            &serde_json::json!({"type":"create_session","username":username}).to_string(),
        )
    }

    pub fn post_auth_response(&mut self, response: Option<&str>) -> Result<GreetdResponse, String> {
        self.send_request(
            &serde_json::json!({"type":"post_auth_message_response","response":response})
                .to_string(),
        )
    }

    pub fn start_session(
        &mut self,
        cmd: &[String],
        env: &[String],
    ) -> Result<GreetdResponse, String> {
        self.send_request(
            &serde_json::json!({"type":"start_session","cmd":cmd,"env":env}).to_string(),
        )
    }

    pub fn cancel_session(&mut self) -> Result<GreetdResponse, String> {
        self.send_request("{\"type\":\"cancel_session\"}")
    }
}

pub fn parse_greetd_response(json: &str) -> Result<GreetdResponse, String> {
    serde_json::from_str(json).map_err(|e| format!("Invalid greetd response: {e}"))
}

/// Complete one greetd conversation. The callback supplies later answers and
/// receives informative messages; it must never decide that authentication
/// succeeded. Only greetd's successful start_session response does that.
pub fn login<F>(
    client: &mut GreetdClient,
    username: &str,
    command: &[String],
    initial_password: Zeroizing<String>,
    mut prompt: F,
) -> Result<(), String>
where
    F: FnMut(&str, &str) -> Result<Option<Zeroizing<String>>, String>,
{
    if command.is_empty() {
        return Err("No desktop session was selected".into());
    }
    let mut initial = (!initial_password.is_empty()).then_some(initial_password);
    let mut response = client.create_session(username)?;
    for _ in 0..32 {
        response = match response {
            GreetdResponse::Success => {
                return match client.start_session(command, &[])? {
                    GreetdResponse::Success => Ok(()),
                    GreetdResponse::Error { description, .. } => Err(description),
                    GreetdResponse::AuthMessage { .. } => {
                        Err("Unexpected authentication prompt while starting session".into())
                    }
                };
            }
            GreetdResponse::Error { description, .. } => return Err(description),
            GreetdResponse::AuthMessage {
                auth_message_type,
                auth_message,
            } => {
                let answer = match auth_message_type.as_str() {
                    "secret" => match initial.take() {
                        Some(password) => Some(password),
                        None => match prompt(&auth_message_type, &auth_message) {
                            Ok(answer) => answer,
                            Err(error) => {
                                let _ = client.cancel_session();
                                return Err(error);
                            }
                        },
                    },
                    "visible" | "info" | "error" => {
                        let result = prompt(&auth_message_type, &auth_message);
                        match result {
                            Ok(answer) if auth_message_type == "visible" => answer,
                            Ok(_) => None,
                            Err(error) => {
                                let _ = client.cancel_session();
                                return Err(error);
                            }
                        }
                    }
                    _ => {
                        let _ = client.cancel_session();
                        return Err("Unknown greetd authentication message type".into());
                    }
                };
                if answer.is_none() && matches!(auth_message_type.as_str(), "secret" | "visible") {
                    let _ = client.cancel_session();
                    return Err("Authentication prompt was not answered".into());
                }
                client.post_auth_response(answer.as_deref().map(String::as_str))?
            }
        };
    }
    let _ = client.cancel_session();
    Err("Too many greetd authentication messages".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_passwd_users() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("passwd");
        fs::write(
            &path,
            "root:x:0:0:root:/root:/bin/bash\n\
             daemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin\n\
             yan:x:1000:1000:Yan Vidal,,,:/home/yan:/bin/bash\n\
             guest:x:1001:1001::/home/guest:/usr/bin/zsh\n\
             disabled:x:1002:1002::/home/disabled:/bin/false\n\
             nobody:x:65534:65534:nobody:/nonexistent:/usr/sbin/nologin\n",
        )
        .unwrap();

        let users = parse_users_from_passwd(&path);
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].username, "yan");
        assert_eq!(users[0].display_name, "Yan Vidal");
        assert_eq!(users[0].uid, 1000);
        assert_eq!(users[1].username, "guest");
        assert_eq!(users[1].display_name, "guest");
        assert_eq!(users[1].uid, 1001);
    }

    #[test]
    fn desktop_session_parses_quoted_arguments_only_in_main_group() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.desktop");
        fs::write(
            &path,
            "[Desktop Action Other]\nExec=/bin/false\n[Desktop Entry]\nName=Test Session\nExec=/usr/bin/env \"XDG_CURRENT_DESKTOP=Test Session\" /usr/bin/sway\nType=Application\n",
        )
        .unwrap();
        let session = parse_session_file(&path).unwrap();
        assert_eq!(session.name, "Test Session");
        assert_eq!(
            session.exec,
            [
                "/usr/bin/env",
                "XDG_CURRENT_DESKTOP=Test Session",
                "/usr/bin/sway"
            ]
        );
    }

    #[test]
    fn test_parse_greetd_responses() {
        let resp = parse_greetd_response("{\"type\":\"success\"}").unwrap();
        assert!(matches!(resp, GreetdResponse::Success));

        let resp = parse_greetd_response(
            "{\"type\":\"auth_message\",\"auth_message_type\":\"secret\",\"auth_message\":\"Password: \"}"
        ).unwrap();
        match resp {
            GreetdResponse::AuthMessage {
                auth_message_type,
                auth_message,
            } => {
                assert_eq!(auth_message_type, "secret");
                assert_eq!(auth_message, "Password: ");
            }
            _ => panic!("Expected AuthMessage"),
        }

        let resp = parse_greetd_response(
            "{\"type\":\"error\",\"error_type\":\"auth_error\",\"description\":\"Authentication failure\"}"
        ).unwrap();
        match resp {
            GreetdResponse::Error {
                error_type,
                description,
            } => {
                assert_eq!(error_type, "auth_error");
                assert_eq!(description, "Authentication failure");
            }
            _ => panic!("Expected Error"),
        }

        // Whitespace and escaped text are valid JSON on the real IPC boundary.
        let resp = parse_greetd_response(
            r#"{ "type" : "auth_message", "auth_message_type": "visible", "auth_message": "Code: \"OTP\"" }"#,
        )
        .unwrap();
        assert!(
            matches!(resp, GreetdResponse::AuthMessage { auth_message, .. } if auth_message == "Code: \"OTP\"")
        );
    }

    #[test]
    fn protocol_continues_multiple_prompts_and_rejects_failed_session_start() {
        use std::os::unix::net::UnixListener;
        use zeroize::Zeroizing;

        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("greetd.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            for (request, reply) in [
                (
                    "create_session",
                    r#"{"type":"auth_message","auth_message_type":"info","auth_message":"Policy notice"}"#,
                ),
                (
                    "post_auth_message_response",
                    r#"{"type":"auth_message","auth_message_type":"visible","auth_message":"One-time code"}"#,
                ),
                (
                    "post_auth_message_response",
                    r#"{"type":"auth_message","auth_message_type":"secret","auth_message":"Password"}"#,
                ),
                ("post_auth_message_response", r#"{"type":"success"}"#),
                (
                    "start_session",
                    r#"{"type":"error","error_type":"error","description":"Session refused"}"#,
                ),
            ] {
                let mut length = [0; 4];
                stream.read_exact(&mut length).unwrap();
                let mut body = vec![0; u32::from_ne_bytes(length) as usize];
                stream.read_exact(&mut body).unwrap();
                let body = String::from_utf8(body).unwrap();
                assert!(body.contains(request), "{body}");
                if reply.contains("One-time code") {
                    assert!(body.contains("\"response\":null"), "{body}");
                }
                stream
                    .write_all(&(reply.len() as u32).to_ne_bytes())
                    .unwrap();
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        let mut client = GreetdClient::connect_path(&socket).unwrap();
        let mut prompts = Vec::new();
        let result = login(
            &mut client,
            "locktest",
            &["/usr/bin/sway".into()],
            Zeroizing::new("DeckLock-test-42".into()),
            |kind, message| {
                prompts.push((kind.to_string(), message.to_string()));
                match kind {
                    "info" | "error" => Ok(Some(Zeroizing::new("must-not-send".into()))),
                    "visible" => Ok(Some(Zeroizing::new("123456".into()))),
                    "secret" => Ok(Some(Zeroizing::new("DeckLock-test-42".into()))),
                    _ => panic!("unexpected kind"),
                }
            },
        );
        assert_eq!(result.unwrap_err(), "Session refused");
        assert_eq!(prompts[0], ("info".into(), "Policy notice".into()));
        assert_eq!(prompts[1], ("visible".into(), "One-time code".into()));
        server.join().unwrap();
    }

    #[test]
    fn protocol_cancels_when_user_aborts_a_prompt() {
        use std::os::unix::net::UnixListener;

        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("greetd.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            for (expected, reply) in [
                (
                    "create_session",
                    r#"{"type":"auth_message","auth_message_type":"visible","auth_message":"Code"}"#,
                ),
                ("cancel_session", r#"{"type":"success"}"#),
            ] {
                let mut length = [0; 4];
                stream.read_exact(&mut length).unwrap();
                let mut body = vec![0; u32::from_ne_bytes(length) as usize];
                stream.read_exact(&mut body).unwrap();
                assert!(String::from_utf8(body).unwrap().contains(expected));
                stream
                    .write_all(&(reply.len() as u32).to_ne_bytes())
                    .unwrap();
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        let mut client = GreetdClient::connect_path(&socket).unwrap();
        let error = login(
            &mut client,
            "locktest",
            &["/usr/bin/sway".into()],
            Zeroizing::new(String::new()),
            |_, _| Err("Prompt canceled".into()),
        )
        .unwrap_err();
        assert_eq!(error, "Prompt canceled");
        server.join().unwrap();
    }

    #[test]
    fn test_greeter_state_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("greeter-state.toml");

        // Non-existent file yields default state
        let state = GreeterState::load_from(&state_path);
        assert_eq!(state, GreeterState::default());

        // Saving and reloading preserves chosen user and session
        let saved = GreeterState {
            last_user: Some("yan".into()),
            last_session: Some("hyprland".into()),
        };
        saved.save_to(&state_path);

        let loaded = GreeterState::load_from(&state_path);
        assert_eq!(loaded.last_user.as_deref(), Some("yan"));
        assert_eq!(loaded.last_session.as_deref(), Some("hyprland"));
    }
}
