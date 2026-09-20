//! Native greetd greeter and session login support.
use std::{
    fs,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
};

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

/// Discover available Wayland and X11 sessions installed on the system.
pub fn list_desktop_sessions() -> Vec<DesktopSession> {
    let mut sessions = Vec::new();

    for dir in ["/usr/share/wayland-sessions", "/usr/share/xsessions"] {
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

    if sessions.is_empty() {
        sessions.push(DesktopSession {
            id: "default".into(),
            name: "Default Session".into(),
            exec: vec!["sh".into(), "-l".into()],
        });
    }

    sessions
}

fn parse_session_file(path: &Path) -> Option<DesktopSession> {
    let content = fs::read_to_string(path).ok()?;
    let id = path.file_stem()?.to_string_lossy().to_string();
    let mut name = None;
    let mut exec = None;

    for line in content.lines() {
        let line = line.trim();
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
    let exec: Vec<String> = exec_str.split_whitespace().map(|s| s.to_string()).collect();
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
        let system_path = Path::new("/var/lib/decklock/greeter-state.toml");
        if system_path.parent().is_some_and(|p| p.is_dir()) {
            return system_path.to_path_buf();
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
#[derive(Debug)]
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
        let req = format!(
            "{{\"type\":\"create_session\",\"username\":\"{}\"}}",
            escape_json(username)
        );
        self.send_request(&req)
    }

    pub fn post_auth_response(&mut self, response: &str) -> Result<GreetdResponse, String> {
        let req = format!(
            "{{\"type\":\"post_auth_message_response\",\"response\":\"{}\"}}",
            escape_json(response)
        );
        self.send_request(&req)
    }

    pub fn start_session(
        &mut self,
        cmd: &[String],
        env: &[String],
    ) -> Result<GreetdResponse, String> {
        let cmd_json: Vec<String> = cmd
            .iter()
            .map(|c| format!("\"{}\"", escape_json(c)))
            .collect();
        let env_json: Vec<String> = env
            .iter()
            .map(|e| format!("\"{}\"", escape_json(e)))
            .collect();
        let req = format!(
            "{{\"type\":\"start_session\",\"cmd\":[{}],\"env\":[{}]}}",
            cmd_json.join(","),
            env_json.join(",")
        );
        self.send_request(&req)
    }

    pub fn cancel_session(&mut self) -> Result<GreetdResponse, String> {
        self.send_request("{\"type\":\"cancel_session\"}")
    }
}

fn escape_json(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c => escaped.push(c),
        }
    }
    escaped
}

fn extract_json_field(json: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{}\":", field);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();
    if let Some(inner) = rest.strip_prefix('"') {
        let mut end = 0;
        let mut escaped = false;
        for (i, c) in inner.char_indices() {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                end = i;
                break;
            }
        }
        Some(inner[..end].to_string())
    } else {
        None
    }
}

pub fn parse_greetd_response(json: &str) -> Result<GreetdResponse, String> {
    let msg_type = extract_json_field(json, "type")
        .ok_or_else(|| format!("Missing type field in greetd response: {json}"))?;

    match msg_type.as_str() {
        "success" => Ok(GreetdResponse::Success),
        "error" => {
            let error_type =
                extract_json_field(json, "error_type").unwrap_or_else(|| "error".into());
            let description = extract_json_field(json, "description")
                .unwrap_or_else(|| "Authentication error".into());
            Ok(GreetdResponse::Error {
                error_type,
                description,
            })
        }
        "auth_message" => {
            let auth_message_type =
                extract_json_field(json, "auth_message_type").unwrap_or_else(|| "secret".into());
            let auth_message = extract_json_field(json, "auth_message").unwrap_or_default();
            Ok(GreetdResponse::AuthMessage {
                auth_message_type,
                auth_message,
            })
        }
        other => Err(format!("Unknown greetd response type: {other}")),
    }
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
