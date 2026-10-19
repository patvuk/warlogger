use std::path::PathBuf;

const SERVICE: &str = "warlogger";

#[derive(Default)]
pub struct Config {
    entries: Vec<(String, String)>,
}

pub fn dir() -> PathBuf {
    let base = match cfg!(windows) {
        true => std::env::var_os("APPDATA").map(PathBuf::from),
        false => std::env::var_os("XDG_CONFIG_HOME")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))),
    };
    base.unwrap_or_else(std::env::temp_dir).join(SERVICE)
}

impl Config {
    pub fn path() -> PathBuf {
        dir().join("config")
    }

    pub fn load() -> Self {
        let text = std::fs::read_to_string(Self::path()).unwrap_or_default();
        let entries = text
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
            .collect();
        Self { entries }
    }

    pub fn save(&self) -> Result<(), String> {
        let text: String = self.entries.iter().map(|(k, v)| format!("{k} = {v}\n")).collect();
        std::fs::create_dir_all(dir())
            .and_then(|()| std::fs::write(Self::path(), text))
            .map_err(|e| format!("{}: {e}", Self::path().display()))
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.is_empty())
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.entries.retain(|(k, _)| k != key);
        if !value.is_empty() {
            self.entries.push((key.to_owned(), value.to_owned()));
        }
    }
}

#[cfg(windows)]
pub use crate::win::cred::{load_secret, save_secret};

#[cfg(target_os = "macos")]
pub fn save_secret(account: &str, secret: &str) -> Result<(), String> {
    let args = ["add-generic-password", "-U", "-a", account, "-s", SERVICE, "-w", secret];
    run("security", &args, None).map(drop)
}

#[cfg(target_os = "macos")]
pub fn load_secret(account: &str) -> Option<String> {
    run(
        "security",
        &["find-generic-password", "-a", account, "-s", SERVICE, "-w"],
        None,
    )
    .ok()
    .filter(|s| !s.is_empty())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn save_secret(account: &str, secret: &str) -> Result<(), String> {
    let args = ["store", "--label", SERVICE, "service", SERVICE, "account", account];
    match run("secret-tool", &args, Some(secret)) {
        Ok(_) => {
            let _ = std::fs::remove_file(dir().join("secret"));
            Ok(())
        }
        Err(_) => write_private(&dir().join("secret"), &format!("{account}\n{secret}")),
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn load_secret(account: &str) -> Option<String> {
    run("secret-tool", &["lookup", "service", SERVICE, "account", account], None)
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let text = std::fs::read_to_string(dir().join("secret")).ok()?;
            let (who, secret) = text.split_once('\n')?;
            (who == account).then(|| secret.to_owned())
        })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn write_private(path: &std::path::Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    std::fs::create_dir_all(dir())
        .and_then(|()| {
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(path)
        })
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(unix)]
fn run(program: &str, args: &[&str], input: Option<&str>) -> Result<String, String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;

    if let Some(input) = input {
        let _ = child.stdin.take().map(|mut s| s.write_all(input.as_bytes()));
    }
    drop(child.stdin.take());

    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_owned();
    match out.status.success() {
        true => Ok(text),
        false => Err(format!("{program} failed")),
    }
}
