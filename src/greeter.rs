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

/// Where the greeter account can remember its choice under greetd, whose `HOME` for
/// that account is `/` and so never writable. `setup greeter` creates it.
pub const SYSTEM_STATE_DIR: &str = "/var/lib/decklock-greeter";

type EnvValue = Option<std::ffi::OsString>;

fn resolve_state_path(
    override_path: EnvValue,
    state_home: EnvValue,
    under_greetd: bool,
    system_dir: &Path,
    home: EnvValue,
) -> PathBuf {
    if let Some(override_path) = override_path {
        return PathBuf::from(override_path);
    }
    if let Some(state_home) = state_home {
        return PathBuf::from(state_home).join("decklock/greeter-state.toml");
    }
    if under_greetd && system_dir.is_dir() {
        return system_dir.join("greeter-state.toml");
    }
    if let Some(home) = home {
        return PathBuf::from(home).join(".local/state/decklock/greeter-state.toml");
    }
    std::env::temp_dir().join("decklock-greeter-state.toml")
}

/// The session to preselect when no earlier choice is remembered.
///
/// A uwsm-managed session (`uwsm start ...`) wins over a bare one for the same
/// compositor: uwsm runs the compositor as a systemd user service, reaches
/// `graphical-session.target` and imports the session environment, which is what
/// user units tied to that target expect. Sessions are sorted by id, so the bare
/// `hyprland` used to come first and be picked. Without a usable uwsm session it is
/// still the first one.
pub fn default_session_index(sessions: &[DesktopSession]) -> usize {
    default_session_index_with(sessions, program_in_path)
}

fn default_session_index_with(
    sessions: &[DesktopSession],
    available: impl Fn(&str) -> bool,
) -> usize {
    sessions
        .iter()
        .position(|session| {
            session.exec.first().is_some_and(|program| {
                Path::new(program)
                    .file_name()
                    .is_some_and(|name| name == "uwsm")
                    && available(program)
            })
        })
        .unwrap_or(0)
}

/// Session entries carry `TryExec` for this, which the greeter does not read: a
/// session whose launcher is not installed must not become the default.
fn program_in_path(program: &str) -> bool {
    let path = Path::new(program);
    if path.is_absolute() {
        return path.is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

impl GreeterState {
    pub fn state_file_path() -> PathBuf {
        resolve_state_path(
            std::env::var_os("DECKLOCK_GREETER_STATE"),
            std::env::var_os("XDG_STATE_HOME"),
            std::env::var_os("GREETD_SOCK").is_some(),
            Path::new(SYSTEM_STATE_DIR),
            std::env::var_os("HOME"),
        )
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
                    GreetdResponse::Error { description, .. } => {
                        let _ = client.cancel_session();
                        Err(description)
                    }
                    GreetdResponse::AuthMessage { .. } => {
                        let _ = client.cancel_session();
                        Err("Unexpected authentication prompt while starting session".into())
                    }
                };
            }
            GreetdResponse::Error { description, .. } => {
                let _ = client.cancel_session();
                return Err(description);
            }
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

    fn session(id: &str, exec: &[&str]) -> DesktopSession {
        DesktopSession {
            id: id.into(),
            name: id.into(),
            exec: exec.iter().map(|part| part.to_string()).collect(),
        }
    }

    fn hyprland_pair() -> Vec<DesktopSession> {
        // Sorted by id, as `list_desktop_sessions` returns them: the bare session
        // comes first, which is what used to be preselected.
        vec![
            session("hyprland", &["/usr/bin/start-hyprland"]),
            session(
                "hyprland-uwsm",
                &["uwsm", "start", "-e", "-D", "Hyprland", "hyprland.desktop"],
            ),
        ]
    }

    #[test]
    fn a_uwsm_managed_session_is_preselected_over_the_bare_one() {
        assert_eq!(default_session_index_with(&hyprland_pair(), |_| true), 1);
    }

    #[test]
    fn uwsm_is_not_preferred_when_it_is_not_installed() {
        assert_eq!(default_session_index_with(&hyprland_pair(), |_| false), 0);
    }

    #[test]
    fn without_a_uwsm_session_the_first_one_is_preselected() {
        let sessions = vec![
            session("00-other", &["/usr/bin/sway"]),
            session("sway", &["/usr/bin/sway"]),
        ];
        assert_eq!(default_session_index_with(&sessions, |_| true), 0);
    }

    #[test]
    fn a_uwsm_path_in_the_exec_counts_as_uwsm() {
        let sessions = vec![
            session("hyprland", &["/usr/bin/start-hyprland"]),
            session(
                "hyprland-uwsm",
                &["/usr/bin/uwsm", "start", "hyprland.desktop"],
            ),
        ];
        assert_eq!(default_session_index_with(&sessions, |_| true), 1);
    }

    fn os(value: &str) -> EnvValue {
        Some(value.into())
    }

    #[test]
    fn under_greetd_the_state_goes_to_the_system_directory_when_it_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = resolve_state_path(None, None, true, dir.path(), os("/"));
        assert_eq!(path, dir.path().join("greeter-state.toml"));
    }

    #[test]
    fn under_greetd_without_the_system_directory_the_home_is_used_as_before() {
        let missing = tempfile::tempdir().unwrap().path().join("absent");
        let path = resolve_state_path(None, None, true, &missing, os("/var/lib/greeter"));
        assert_eq!(
            path,
            Path::new("/var/lib/greeter/.local/state/decklock/greeter-state.toml")
        );
    }

    #[test]
    fn outside_greetd_the_system_directory_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = resolve_state_path(None, None, false, dir.path(), os("/home/me"));
        assert_eq!(
            path,
            Path::new("/home/me/.local/state/decklock/greeter-state.toml")
        );
    }

    #[test]
    fn an_explicit_path_and_xdg_state_home_still_win_over_the_system_directory() {
        let dir = tempfile::tempdir().unwrap();
        let explicit = resolve_state_path(os("/x/state.toml"), os("/xdg"), true, dir.path(), None);
        assert_eq!(explicit, Path::new("/x/state.toml"));
        let xdg = resolve_state_path(None, os("/xdg"), true, dir.path(), None);
        assert_eq!(xdg, Path::new("/xdg/decklock/greeter-state.toml"));
    }

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
    fn protocol_cancels_denied_attempt_before_another_login() {
        use std::os::unix::net::UnixListener;

        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("greetd.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            for (expected, reply) in [
                (
                    "create_session",
                    r#"{"type":"auth_message","auth_message_type":"secret","auth_message":"Password"}"#,
                ),
                (
                    "post_auth_message_response",
                    r#"{"type":"error","error_type":"auth_error","description":"Denied"}"#,
                ),
                ("cancel_session", r#"{"type":"success"}"#),
                (
                    "create_session",
                    r#"{"type":"auth_message","auth_message_type":"secret","auth_message":"Password"}"#,
                ),
                ("post_auth_message_response", r#"{"type":"success"}"#),
                ("start_session", r#"{"type":"success"}"#),
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
        assert_eq!(
            login(
                &mut client,
                "locktest",
                &["/usr/bin/sway".into()],
                Zeroizing::new("wrong".into()),
                |_, _| unreachable!(),
            )
            .unwrap_err(),
            "Denied"
        );
        login(
            &mut client,
            "locktest",
            &["/usr/bin/sway".into()],
            Zeroizing::new("correct".into()),
            |_, _| unreachable!(),
        )
        .unwrap();
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
