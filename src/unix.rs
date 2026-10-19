use crate::CANCEL;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

pub struct Pid(PathBuf);

pub struct Hidden(Option<libc::termios>);

fn pid_path() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("warlogger").join("live.pid")
}

fn running() -> Option<i32> {
    let pid: i32 = std::fs::read_to_string(pid_path()).ok()?.trim().parse().ok()?;
    (unsafe { libc::kill(pid, 0) } == 0).then_some(pid)
}

impl Pid {
    pub fn write() -> Result<Self, String> {
        if let Some(pid) = running() {
            return Err(format!(
                "live upload already running (pid {pid}); stop it with `warlogger stop`"
            ));
        }

        let path = pid_path();
        std::fs::create_dir_all(path.parent().unwrap_or(&path))
            .and_then(|()| std::fs::write(&path, std::process::id().to_string()))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self(path))
    }
}

impl Drop for Pid {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub fn stop() -> Result<(), String> {
    let pid = running().ok_or("no live upload is running")?;
    unsafe { libc::kill(pid, libc::SIGTERM) };
    println!("stopping live upload (pid {pid}); it uploads the last fight first");
    Ok(())
}

pub fn on_signals() {
    extern "C" fn handle(_: libc::c_int) {
        if CANCEL.swap(true, Ordering::Relaxed) {
            std::process::exit(130);
        }
    }

    unsafe {
        libc::signal(libc::SIGINT, handle as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, handle as *const () as libc::sighandler_t);
    }
}

impl Hidden {
    pub fn new(hidden: bool) -> Self {
        Self(hidden.then(|| unsafe {
            let mut term = std::mem::zeroed::<libc::termios>();
            libc::tcgetattr(0, &mut term);
            let saved = term;
            term.c_lflag &= !libc::ECHO;
            libc::tcsetattr(0, libc::TCSANOW, &term);
            saved
        }))
    }
}

impl Drop for Hidden {
    fn drop(&mut self) {
        if let Some(saved) = self.0 {
            unsafe { libc::tcsetattr(0, libc::TCSANOW, &saved) };
            eprintln!();
        }
    }
}
