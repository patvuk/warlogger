use crate::upload::Event;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const PLUGIN_ID: &str = "patvuk.warlogger";
const MANIFEST: &str = include_str!("../omarchy/manifest.json");
const WIDGET: &str = include_str!("../omarchy/BarWidget.qml");

pub struct Status {
    path: PathBuf,
    state: &'static str,
    text: String,
    progress: f32,
    report: String,
}

fn runtime() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("warlogger")
}

fn quote(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c < ' ' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl Status {
    pub fn new() -> Self {
        let live = std::env::args().nth(1).is_some_and(|a| a == "live");
        let status = Self {
            path: runtime().join("status.json"),
            state: if live { "live" } else { "working" },
            text: String::new(),
            progress: 0.0,
            report: String::new(),
        };

        let _ = std::fs::create_dir_all(runtime());
        status.write();
        status
    }

    fn write(&self) {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let mut out = format!(
            r#"{{"ts":{ts},"pid":{},"state":"{}","progress":{:.3},"text":"#,
            std::process::id(),
            self.state,
            self.progress
        );
        quote(&mut out, &self.text);
        out.push_str(r#","report":"#);
        quote(&mut out, &self.report);
        out.push_str("}\n");

        let staged = self.path.with_extension("new");
        let _ = std::fs::write(&staged, out).and_then(|()| std::fs::rename(&staged, &self.path));
    }

    pub fn event(&mut self, event: &Event) {
        match event {
            Event::Step(text) => self.text.clone_from(text),
            Event::Progress(p) => {
                if (p - self.progress).abs() < 0.01 && *p < 1.0 {
                    return;
                }
                self.progress = *p;
            }
            Event::Report(url) => self.report.clone_from(url),
        }
        self.write();
    }

    pub fn fail(&mut self, error: &str) {
        self.state = "error";
        error.clone_into(&mut self.text);
        self.write();
    }
}

impl Drop for Status {
    fn drop(&mut self) {
        if self.state != "error" {
            self.state = "done";
            self.write();
        }
    }
}

fn plugin_dir() -> Result<PathBuf, String> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or("HOME is not set")?;
    Ok(config.join("omarchy").join("plugins").join(PLUGIN_ID))
}

fn quiet(program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("{program}: {e}"))?;
    match status.success() {
        true => Ok(()),
        false => Err(format!("{program} {} failed", args.join(" "))),
    }
}

fn write_plugin(dir: &Path) -> Result<(), String> {
    let manifest = MANIFEST.replace("@VERSION@", env!("CARGO_PKG_VERSION"));
    std::fs::create_dir_all(dir)
        .and_then(|()| std::fs::write(dir.join("manifest.json"), manifest))
        .and_then(|()| std::fs::write(dir.join("BarWidget.qml"), WIDGET))
        .map_err(|e| format!("{}: {e}", dir.display()))
}

fn install() -> Result<(), String> {
    let dir = plugin_dir()?;
    write_plugin(&dir)?;

    let _ = quiet("omarchy-shell", &["shell", "rescanPlugins"]);
    quiet("omarchy", &["plugin", "enable", PLUGIN_ID, "--section", "right"])
        .map_err(|e| format!("{} written, but enabling it failed: {e}", dir.display()))?;

    println!("added {PLUGIN_ID} to the bar");
    Ok(())
}

fn uninstall() -> Result<(), String> {
    let dir = plugin_dir()?;
    let _ = quiet("omarchy", &["plugin", "disable", PLUGIN_ID]);

    match std::fs::remove_dir_all(&dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("{}: {e}", dir.display())),
        _ => {
            println!("removed {PLUGIN_ID}");
            Ok(())
        }
    }
}

pub fn run(action: Option<&str>) -> Result<(), String> {
    if unsafe { libc::geteuid() } == 0 {
        return Err("run `warlogger omarchy ...` as your desktop user, without sudo".into());
    }

    match action {
        Some("install") => install(),
        Some("uninstall") => uninstall(),
        _ => Err("usage: warlogger omarchy <install|uninstall>".into()),
    }
}
