//! Linux-PAM runs in a disposable child, never on the GTK event thread.
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

pub const MAX_PASSWORD_BYTES: usize = 511;
/// Upper bound for PAM text forwarded to the screen. Modules send short lines.
const MAX_NOTICE_BYTES: usize = 512;
const AUTH_TIMEOUT: Duration = Duration::from_secs(30);

fn valid_service(service: &str) -> bool {
    !service.is_empty()
        && service.len() <= 128
        && service
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}

/// A finished authentication attempt, plus any text PAM wanted the user to see.
#[derive(Debug, PartialEq)]
pub struct Outcome {
    pub accepted: bool,
    pub notice: Option<String>,
}

/// Flatten module text into one displayable line: control characters become
/// spaces, runs of whitespace collapse, and the result is bounded. Callers
/// render it as plain text, never as markup.
fn sanitize_notice(raw: &[u8]) -> Option<String> {
    let mut text = String::new();
    let mut pending = false;
    for character in String::from_utf8_lossy(raw).chars() {
        if character.is_control() || character.is_whitespace() {
            pending = !text.is_empty();
            continue;
        }
        let needed = character.len_utf8() + usize::from(pending);
        if text.len() + needed > MAX_NOTICE_BYTES {
            break;
        }
        if std::mem::take(&mut pending) {
            text.push(' ');
        }
        text.push(character);
    }
    (!text.is_empty()).then_some(text)
}

fn valid_password(password: &[u8]) -> bool {
    !password.is_empty() && password.len() <= MAX_PASSWORD_BYTES && !password.contains(&0)
}

/// Call from a worker thread. Passwords travel only through the child's stdin.
pub fn authenticate(password: Zeroizing<String>, service: &str) -> Result<Outcome, String> {
    if !valid_service(service) {
        return Err("Invalid PAM service name".into());
    }
    if !valid_password(password.as_bytes()) {
        return Ok(Outcome {
            accepted: false,
            notice: None,
        });
    }
    let executable = std::env::current_exe().map_err(|_| "Cannot locate authentication helper")?;
    let mut command = Command::new(executable);
    command.args(["--auth-helper", service]);
    // Pin the module locale so its wording can be recognized and re-rendered in
    // the interface language instead of being matched against translations.
    command.env("LC_ALL", "C");
    run_helper(&mut command, password.as_bytes(), AUTH_TIMEOUT)
}

fn run_helper(
    command: &mut Command,
    password: &[u8],
    timeout: Duration,
) -> Result<Outcome, String> {
    if !valid_password(password) {
        return Ok(Outcome {
            accepted: false,
            notice: None,
        });
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Cannot start authentication helper")?;
    let start = Instant::now();
    let write_result = child
        .stdin
        .take()
        .ok_or(std::io::ErrorKind::BrokenPipe.into())
        .and_then(|mut stdin| stdin.write_all(password));
    // At most 511 bytes are written to a new, empty pipe (below Linux PIPE_BUF).
    // Closing stdin frames the request and avoids line-ending transformations.
    if write_result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Cannot send authentication request".into());
    }
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let accepted = match status.code() {
                    Some(0) => true,
                    Some(1) => false,
                    _ => return Err("Authentication helper failed".into()),
                };
                // The helper never writes more than MAX_NOTICE_BYTES, so it
                // cannot block on this pipe while we wait for it to exit.
                let mut raw = Vec::new();
                if let Some(stdout) = child.stdout.take() {
                    let _ = stdout.take(MAX_NOTICE_BYTES as u64).read_to_end(&mut raw);
                }
                return Ok(Outcome {
                    accepted,
                    notice: sanitize_notice(&raw),
                });
            }
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Authentication timed out".into());
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Cannot collect authentication result".into());
            }
        }
    }
}

/// Resolve the actual process uid through NSS, never a caller-controlled USER.
pub fn current_username() -> Result<String, String> {
    let mut buffer = vec![0_u8; 16384];
    loop {
        let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        // SAFETY: entry, result and buffer are valid writable storage for this call.
        let status = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                entry.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status == libc::ERANGE && buffer.len() < 1024 * 1024 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        if status != 0 || result.is_null() {
            return Err("Cannot resolve session user".into());
        }
        // SAFETY: successful getpwuid_r initialized entry; name lives in buffer.
        let name = unsafe {
            let entry = entry.assume_init();
            if entry.pw_name.is_null() {
                return Err("Session user has no name".into());
            }
            CStr::from_ptr(entry.pw_name)
        };
        return name
            .to_str()
            .map(str::to_owned)
            .map_err(|_| "Invalid session username".into());
    }
}

#[repr(C)]
struct PamMessage {
    style: c_int,
    text: *const c_char,
}
#[repr(C)]
struct PamResponse {
    response: *mut c_char,
    code: c_int,
}
#[repr(C)]
struct PamConversation {
    callback: unsafe extern "C" fn(
        c_int,
        *const *const PamMessage,
        *mut *mut PamResponse,
        *mut c_void,
    ) -> c_int,
    data: *mut c_void,
}
// Linux-PAM ABI checked against security/pam_appl.h and security/_pam_types.h.
#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(
        service: *const c_char,
        user: *const c_char,
        conv: *const PamConversation,
        handle: *mut *mut c_void,
    ) -> c_int;
    fn pam_authenticate(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_acct_mgmt(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_end(handle: *mut c_void, status: c_int) -> c_int;
}
struct ConversationData {
    notices: String,
    password: Zeroizing<Vec<u8>>,
    username: CString,
}

unsafe extern "C" fn conversation(
    count: c_int,
    messages: *const *const PamMessage,
    output: *mut *mut PamResponse,
    data: *mut c_void,
) -> c_int {
    const CONV_ERR: c_int = 19;
    if !(1..=32).contains(&count) || messages.is_null() || output.is_null() || data.is_null() {
        return CONV_ERR;
    }
    // SAFETY: PAM invokes us with its message array and the context installed by
    // helper_main. calloc/free match PAM's ownership contract for responses.
    unsafe {
        *output = std::ptr::null_mut();
        let data = &mut *data.cast::<ConversationData>();
        let responses =
            libc::calloc(count as usize, std::mem::size_of::<PamResponse>()).cast::<PamResponse>();
        if responses.is_null() {
            return CONV_ERR;
        }
        for index in 0..count as usize {
            let message = *messages.add(index);
            let bytes = if message.is_null() {
                None
            } else {
                match (*message).style {
                    1 => Some(data.password.as_slice()),
                    2 => Some(data.username.as_bytes()),
                    // Informational text only: PAM_ERROR_MSG and PAM_TEXT_INFO
                    // are shown to the user, and expect an empty response.
                    3 | 4 => {
                        if !(*message).text.is_null() && data.notices.len() < MAX_NOTICE_BYTES {
                            if !data.notices.is_empty() {
                                data.notices.push(' ');
                            }
                            data.notices
                                .push_str(&CStr::from_ptr((*message).text).to_string_lossy());
                        }
                        continue;
                    }
                    _ => None,
                }
            };
            if let Some(bytes) = bytes {
                let response = libc::malloc(bytes.len() + 1).cast::<u8>();
                if !response.is_null() {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), response, bytes.len());
                    *response.add(bytes.len()) = 0;
                    (*responses.add(index)).response = response.cast();
                    continue;
                }
            }
            for previous in 0..index {
                let response = (*responses.add(previous)).response;
                if !response.is_null() {
                    libc::explicit_bzero(response.cast(), libc::strlen(response));
                    libc::free(response.cast());
                }
            }
            libc::free(responses.cast());
            return CONV_ERR;
        }
        // PAM now owns responses, including password copies, and must free them.
        *output = responses;
    }
    0
}

/// Internal process entry, handled before parsing normal options or starting GTK.
/// 0 = accepted, 1 = denied, 2 = helper error. Standard output carries only
/// PAM_ERROR_MSG/PAM_TEXT_INFO text, flattened and bounded: never the password,
/// never a prompt we answered.
pub fn helper_main(service: &str) -> i32 {
    if !valid_service(service) {
        return 2;
    }
    // SAFETY: Linux prctl and setrlimit receive documented values and live storage.
    // Disable core dumps before reading secrets; no elevated executable needed.
    unsafe {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::setrlimit(libc::RLIMIT_CORE, &limit) != 0
            || libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) != 0
        {
            return 2;
        }
    }
    let mut password = Zeroizing::new(Vec::with_capacity(MAX_PASSWORD_BYTES + 1));
    if std::io::stdin()
        .take((MAX_PASSWORD_BYTES + 1) as u64)
        .read_to_end(&mut password)
        .is_err()
    {
        return 2;
    }
    if !valid_password(&password) {
        return 1;
    }
    let Ok(username) = current_username()
        .and_then(|name| CString::new(name).map_err(|_| "Invalid username".into()))
    else {
        return 2;
    };
    let Ok(service) = CString::new(service) else {
        return 2;
    };
    let mut data = ConversationData {
        notices: String::new(),
        password,
        username,
    };
    let conv = PamConversation {
        callback: conversation,
        data: (&mut data as *mut ConversationData).cast(),
    };
    let mut handle = std::ptr::null_mut();
    // SAFETY: all C strings and conversation storage outlive pam_end. The opaque
    // handle is used only after pam_start succeeds and finalized exactly once.
    unsafe {
        let start = pam_start(service.as_ptr(), data.username.as_ptr(), &conv, &mut handle);
        if start != 0 || handle.is_null() {
            return 2;
        }
        let mut result = pam_authenticate(handle, 1); // PAM_DISALLOW_NULL_AUTHTOK
        if result == 0 {
            result = pam_acct_mgmt(handle, 0);
        }
        let end = pam_end(handle, result);
        // Only module text collected above leaves this process; never a prompt,
        // the password, or PAM's own status codes.
        if let Some(notice) = sanitize_notice(data.notices.as_bytes()) {
            let _ = std::io::stdout().write_all(notice.as_bytes());
        }
        if result == 0 && end == 0 { 0 } else { 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_requests_before_spawning() {
        assert!(!valid_service("../login"));
        assert!(!valid_service(""));
        assert!(valid_service("decklock-test"));
        for password in [vec![], vec![0], vec![b'x'; MAX_PASSWORD_BYTES + 1]] {
            assert_eq!(
                run_helper(
                    &mut Command::new("/does/not/exist"),
                    &password,
                    AUTH_TIMEOUT
                ),
                Ok(Outcome {
                    accepted: false,
                    notice: None
                })
            );
        }
    }

    #[test]
    fn pam_text_reaches_the_caller_with_the_denial() {
        let result = run_helper(
            Command::new("/bin/sh").args([
                "-c",
                "cat >/dev/null; printf 'The account is locked due to 3 failed logins.'; exit 1",
            ]),
            b"test-fixture",
            Duration::from_secs(2),
        );
        assert_eq!(
            result,
            Ok(Outcome {
                accepted: false,
                notice: Some("The account is locked due to 3 failed logins.".into()),
            })
        );
    }

    #[test]
    fn helper_text_is_flattened_and_bounded_before_display() {
        let result = run_helper(
            Command::new("/bin/sh").args([
                "-c",
                r"cat >/dev/null; printf 'first\nsecond\ttab\r\033[31m  spaced  '; \
                  head -c 4000 /dev/zero | tr '\0' 'x'; exit 1",
            ]),
            b"test-fixture",
            Duration::from_secs(2),
        );
        let notice = result.unwrap().notice.expect("sanitized text");
        assert!(
            notice.starts_with("first second tab [31m spaced x"),
            "{notice:?}"
        );
        assert!(!notice.contains('\n') && !notice.contains('\t') && !notice.contains('\r'));
        assert!(notice.len() <= MAX_NOTICE_BYTES, "{} bytes", notice.len());
    }

    #[test]
    fn a_silent_helper_reports_no_text() {
        let result = run_helper(
            Command::new("/bin/sh").args(["-c", "cat >/dev/null; printf '   '; exit 0"]),
            b"test-fixture",
            Duration::from_secs(2),
        );
        assert_eq!(
            result,
            Ok(Outcome {
                accepted: true,
                notice: None
            })
        );
    }

    #[test]
    fn fake_helper_acceptance_denial_and_crash_are_distinct() {
        for (script, expected) in [
            ("cat >/dev/null; exit 0", Some(true)),
            ("cat >/dev/null; exit 1", Some(false)),
            ("cat >/dev/null; kill -TERM $$", None),
        ] {
            let result = run_helper(
                Command::new("/bin/sh").args(["-c", script]),
                b"test-fixture",
                Duration::from_secs(2),
            );
            assert_eq!(result.map(|outcome| outcome.accepted).ok(), expected);
        }
    }

    #[test]
    fn stuck_helper_is_killed_and_reaped() {
        let start = Instant::now();
        let result = run_helper(
            Command::new("/bin/sh").args(["-c", "exec sleep 10"]),
            b"test-fixture",
            Duration::from_millis(30),
        );
        assert_eq!(result, Err("Authentication timed out".into()));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn actual_uid_resolves_without_environment_username() {
        assert!(!current_username().unwrap().is_empty());
    }

    #[test]
    fn conversation_answers_password_and_username_and_rejects_unknown_prompts() {
        let mut data = ConversationData {
            password: Zeroizing::new(b"fixture-password".to_vec()),
            username: CString::new("fixture-user").unwrap(),
            notices: String::new(),
        };
        for style in [1, 2, 3, 4, 7] {
            let text = CString::new(format!("message {style}")).unwrap();
            let message = PamMessage {
                style,
                text: text.as_ptr(),
            };
            let messages = [&message as *const PamMessage];
            let mut output = std::ptr::null_mut();
            // SAFETY: this fake PAM call supplies the same valid allocations as
            // the real ABI. Responses are inspected and freed exactly once.
            unsafe {
                let result = conversation(
                    1,
                    messages.as_ptr(),
                    &mut output,
                    (&mut data as *mut ConversationData).cast(),
                );
                if style == 7 {
                    assert_eq!(result, 19);
                    assert!(output.is_null());
                    continue;
                }
                assert_eq!(result, 0);
                assert!(!output.is_null());
                let response = (*output).response;
                if style <= 2 {
                    let expected: &[u8] = if style == 1 {
                        b"fixture-password"
                    } else {
                        b"fixture-user"
                    };
                    assert_eq!(CStr::from_ptr(response).to_bytes(), expected);
                    libc::explicit_bzero(response.cast(), libc::strlen(response));
                    libc::free(response.cast());
                } else {
                    assert!(response.is_null());
                }
                libc::free(output.cast());
            }
        }
        // Only informational styles are forwarded; prompts we answer are never shown.
        assert_eq!(data.notices, "message 3 message 4");
    }
}
