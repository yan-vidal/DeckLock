//! Own only the opt-in greeter daemon, never the desktop user's sc-controller.
use std::{
    fs,
    os::unix::{fs::FileTypeExt, process::CommandExt},
    path::PathBuf,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

pub struct GreeterController {
    child: Child,
    _directory: tempfile::TempDir,
    pub socket: PathBuf,
}

impl GreeterController {
    pub fn start() -> Result<Self, String> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or("A private XDG_RUNTIME_DIR is required for greeter controller input")?;
        let directory = tempfile::Builder::new()
            .prefix("decklock-pad-")
            .tempdir_in(runtime)
            .map_err(|e| format!("Cannot prepare greeter controller storage: {e}"))?;
        let root = directory.path();
        fs::create_dir(root.join("scc")).map_err(|e| e.to_string())?;
        let profile = root.join("Greeter.sccprofile");
        fs::write(
            &profile,
            include_str!("../assets/controller/Greeter.sccprofile"),
        )
        .map_err(|e| e.to_string())?;
        let socket = root.join("scc/daemon.socket");
        let mut command = Command::new("scc-daemon");
        command
            .arg(profile)
            .args(["start", "--foreground", "--alone"])
            .env("HOME", root)
            .env("XDG_CONFIG_HOME", root)
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env_remove("GREETD_SOCK");
        // Spawn from the main thread. Linux preserves this across ordinary exec;
        // --foreground/--alone prevent daemonization and separate OSD/auto-switch
        // processes. SIGKILL also covers a hung daemon and a killed/crashed greeter.
        let parent = unsafe { libc::getpid() };
        // SAFETY: this post-fork closure only calls async-signal-safe operations,
        // allocates nothing and closes the race where the parent already died.
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    libc::_exit(1);
                }
                Ok(())
            });
        }
        let child = command
            .spawn()
            .map_err(|e| format!("Cannot start scc-daemon: {e}"))?;
        let mut daemon = Self {
            child,
            _directory: directory,
            socket,
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = daemon.child.try_wait().map_err(|e| e.to_string())? {
                return Err(format!("scc-daemon exited before readiness: {status}"));
            }
            if fs::symlink_metadata(&daemon.socket)
                .is_ok_and(|metadata| metadata.file_type().is_socket())
            {
                return Ok(daemon);
            }
            if Instant::now() >= deadline {
                return Err("scc-daemon did not create its private socket within 2 seconds".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for GreeterController {
    fn drop(&mut self) {
        // Let sc-controller release its devices normally. A stalled daemon is
        // killed within the bound; only this unreaped child can receive the signal.
        if self.child.try_wait().is_ok_and(|status| status.is_none()) {
            // SAFETY: an unreaped child cannot have its PID reused.
            unsafe { libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM) };
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        let force_after = Instant::now() + Duration::from_secs(2);
        while self.child.try_wait().is_ok_and(|status| status.is_none()) {
            if Instant::now() >= force_after {
                let _ = self.child.kill();
            }
            if Instant::now() >= deadline {
                eprintln!("Greeter controller process did not exit within 3 seconds");
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
