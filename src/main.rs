#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod engine;
mod game;
mod net;
#[cfg(all(feature = "omarchy", target_os = "linux"))]
mod omarchy;
mod site;
mod store;
#[cfg(unix)]
mod unix;
mod upload;
#[cfg(windows)]
mod win;
mod zip;

use game::{GAMES, Game};
use serde_json::Value;
use site::{Account, Choice};
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use store::Config;
use upload::{Event, Session, Target};

#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
use win as platform;

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => { eprintln!($($t)*) };
}

const USAGE: &str = "warlogger <command> [options]

commands:
  games                       list supported sites
  login                       sign in and remember the account
  logs                        list log files, newest first
  upload [latest|N|path]...   upload logs by index from `logs`, path, or the newest
  live [--all]                upload fights as they happen until stopped
  stop                        stop a running `live`
  set <key> <value>           set a default (game, region, guild, visibility, description, dir.<game>)
  omarchy <install|uninstall> add or remove the Omarchy bar widget

options:
  -g, --game <id>             wow, classic, fresh, sod, vanilla, titan, ff, eso, swtor, fellowship
  -d, --dir <path>            log folder
  -e, --email <email>
  -r, --region <name|id>
  -G, --guild <name|id>       guild to upload to, personal logs if unset
  -v, --visibility <name|id>  public, private, unlisted
  -m, --desc <text>           report description";

pub static CANCEL: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
pub struct Args {
    command: String,
    rest: Vec<String>,
    game: Option<String>,
    dir: Option<PathBuf>,
    email: Option<String>,
    region: Option<String>,
    guild: Option<String>,
    visibility: Option<String>,
    desc: Option<String>,
    all: bool,
}

fn parse() -> Result<Args, String> {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);

    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "-g" | "--game" => args.game = Some(value()?),
            "-d" | "--dir" => args.dir = Some(PathBuf::from(value()?)),
            "-e" | "--email" => args.email = Some(value()?),
            "-r" | "--region" => args.region = Some(value()?),
            "-G" | "--guild" => args.guild = Some(value()?),
            "-v" | "--visibility" => args.visibility = Some(value()?),
            "-m" | "--desc" => args.desc = Some(value()?),
            "-a" | "--all" => args.all = true,
            "-h" | "--help" => args.command = "help".into(),
            "-V" | "--version" => args.command = "version".into(),
            _ if args.command.is_empty() => args.command = arg,
            _ => args.rest.push(arg),
        }
    }

    Ok(args)
}

fn game(cfg: &Config, args: &Args) -> Result<&'static Game, String> {
    let id = args.game.as_deref().or(cfg.get("game")).unwrap_or("wow");
    Game::find(id).ok_or_else(|| format!("unknown game {id:?}; see `warlogger games`"))
}

fn log_dir(cfg: &Config, args: &Args, game: &Game) -> Result<PathBuf, String> {
    args.dir
        .clone()
        .or_else(|| cfg.get(&format!("dir.{}", game.id)).map(PathBuf::from))
        .or_else(|| game.detect())
        .ok_or_else(|| {
            format!(
                "no {} log folder found; pass --dir or run `warlogger set dir.{} <path>`",
                game.name, game.id
            )
        })
}

fn pick(choices: &[Choice], wanted: Option<&str>, what: &str) -> Result<Option<Value>, String> {
    let Some(wanted) = wanted else { return Ok(None) };
    choices
        .iter()
        .find(|c| c.label.eq_ignore_ascii_case(wanted) || c.value.to_string().trim_matches('"') == wanted)
        .map(|c| Some(c.value.clone()))
        .ok_or_else(|| {
            let names: Vec<_> = choices.iter().map(|c| format!("{} ({})", c.label, c.value)).collect();
            format!("unknown {what} {wanted:?}; choose from: {}", names.join(", "))
        })
}

pub fn target(cfg: &Config, args: &Args, account: &Account) -> Result<Target, String> {
    let first = |list: &[Choice]| list.first().map(|c| c.value.clone());
    let region = pick(&account.regions, args.region.as_deref().or(cfg.get("region")), "region")?;
    let visibility = pick(
        &account.visibilities,
        args.visibility.as_deref().or(cfg.get("visibility")),
        "visibility",
    )?;
    let guild = pick(&account.guilds, args.guild.as_deref().or(cfg.get("guild")), "guild")?;

    Ok(Target {
        region: region.or_else(|| first(&account.regions)).unwrap_or(Value::from(0)),
        guild: guild.filter(|g| g.as_i64() != Some(-1)),
        visibility: visibility.or_else(|| first(&account.visibilities)),
        description: args
            .desc
            .clone()
            .or(cfg.get("description").map(str::to_owned))
            .unwrap_or_default(),
    })
}

fn prompt(label: &str, hidden: bool) -> Result<String, String> {
    eprint!("{label}: ");
    let _ = std::io::stderr().flush();

    let mut line = String::new();
    let read = {
        let _hidden = platform::Hidden::new(hidden);
        std::io::stdin().read_line(&mut line)
    };

    read.map_err(|e| e.to_string())?;
    Ok(line.trim().to_owned())
}

fn email(cfg: &Config, args: &Args, game: &Game) -> Option<String> {
    let saved = cfg.get(&format!("email.{}", game.site())).or(cfg.get("email"));
    args.email.clone().or(saved.map(str::to_owned))
}

fn credentials(cfg: &Config, args: &Args, game: &Game) -> Result<(String, String), String> {
    let email = email(cfg, args, game).ok_or(format!(
        "not logged in to {}; run `warlogger login -g {}`",
        game.site(),
        game.id
    ))?;
    let password = std::env::var("WARLOGGER_PASSWORD")
        .ok()
        .or_else(|| store::load_secret(&email))
        .ok_or("no saved password; run `warlogger login`")?;
    Ok((email, password))
}

struct Printer {
    #[cfg(all(feature = "omarchy", target_os = "linux"))]
    status: Option<omarchy::Status>,
}

impl Printer {
    fn new() -> Self {
        Self {
            #[cfg(all(feature = "omarchy", target_os = "linux"))]
            status: Some(omarchy::Status::new()),
        }
    }

    fn event(&mut self, event: Event) {
        #[cfg(all(feature = "omarchy", target_os = "linux"))]
        if let Some(status) = self.status.as_mut() {
            status.event(&event);
        }

        match event {
            Event::Step(text) => eprintln!("{text}"),
            Event::Progress(p) => eprint!("\r{:>5.1}%{}", p * 100.0, if p >= 1.0 { "\n" } else { "" }),
            Event::Report(url) => eprintln!("report {url}"),
        }
    }

    fn finish(&mut self, result: Result<(), String>) -> Result<(), String> {
        #[cfg(all(feature = "omarchy", target_os = "linux"))]
        if let (Some(status), Err(e)) = (self.status.as_mut(), &result) {
            status.fail(e);
        }
        result
    }
}

fn big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(32 << 20)
        .spawn(f)
        .expect("spawn worker")
        .join()
        .unwrap_or_else(|_| std::process::abort())
}

fn login(mut cfg: Config, args: &Args) -> Result<(), String> {
    let game = game(&cfg, args)?;
    let email = match email(&cfg, args, game) {
        Some(e) => e,
        None => prompt("email", false)?,
    };
    let password = match std::env::var("WARLOGGER_PASSWORD") {
        Ok(p) => p,
        Err(_) => prompt("password", true)?,
    };

    let account = site::Site::new(game).login(&email, &password)?;
    store::save_secret(&email, &password)?;
    cfg.set(&format!("email.{}", game.site()), &email);
    cfg.set("game", game.id);
    cfg.save()?;

    println!("signed in to {} as {}", game.host, account.name);
    for (what, list) in [
        ("guild", &account.guilds),
        ("region", &account.regions),
        ("visibility", &account.visibilities),
    ] {
        let names: Vec<_> = list.iter().map(|c| format!("{} ({})", c.label, c.value)).collect();
        println!("{what:<11} {}", names.join(", "));
    }
    Ok(())
}

fn logs(cfg: &Config, args: &Args) -> Result<(), String> {
    let game = game(cfg, args)?;
    let dir = log_dir(cfg, args, game)?;
    println!("{}", dir.display());

    for (i, log) in game.logs(&dir).iter().enumerate() {
        let age = log.modified.elapsed().map_or(0, |d| d.as_secs() / 60);
        let name = log.path.file_name().unwrap_or_default().to_string_lossy();
        println!(
            "{:>3}  {:>9.1} MB  {:>6} min ago  {name}",
            i + 1,
            log.size as f64 / 1e6,
            age
        );
    }
    Ok(())
}

fn selected(cfg: &Config, args: &Args, game: &Game) -> Result<Vec<PathBuf>, String> {
    let wanted = if args.rest.is_empty() {
        vec!["latest".to_owned()]
    } else {
        args.rest.clone()
    };
    let mut out = Vec::new();
    let mut listing = None;

    for item in wanted {
        let path = PathBuf::from(&item);
        if path.is_file() {
            out.push(path);
            continue;
        }

        let index = match item.as_str() {
            "latest" => 1,
            n => n
                .parse::<usize>()
                .map_err(|_| format!("{item}: not a file or log number"))?,
        };
        if listing.is_none() {
            listing = Some(game.logs(&log_dir(cfg, args, game)?));
        }
        let log = listing
            .as_ref()
            .and_then(|l| l.get(index.wrapping_sub(1)))
            .ok_or(format!("no log number {index}"))?;
        out.push(log.path.clone());
    }

    Ok(out)
}

fn upload(cfg: Config, args: Args) -> Result<(), String> {
    let game = game(&cfg, &args)?;
    let files = selected(&cfg, &args, game)?;
    let (email, password) = credentials(&cfg, &args, game)?;

    big_stack(move || {
        let mut printer = Printer::new();
        let mut notify = |e| printer.event(e);
        let result = (|| {
            let mut session = Session::open(game, &email, &password, &mut notify)?;
            let target = target(&cfg, &args, &session.account)?;

            for file in files {
                let url = session.upload(&file, &target, &CANCEL, &mut notify)?;
                println!("{url}");
            }
            Ok(())
        })();
        printer.finish(result)
    })
}

fn live(cfg: Config, args: Args) -> Result<(), String> {
    let game = game(&cfg, &args)?;
    let dir = log_dir(&cfg, &args, game)?;
    let (email, password) = credentials(&cfg, &args, game)?;
    #[cfg(unix)]
    let _pid = unix::Pid::write()?;

    big_stack(move || {
        let mut printer = Printer::new();
        let mut notify = |e| printer.event(e);
        let result = (|| {
            let mut session = Session::open(game, &email, &password, &mut notify)?;
            let target = target(&cfg, &args, &session.account)?;

            let url = session.live(&dir, &target, args.all, &CANCEL, &mut notify)?;
            println!("{url}");
            Ok(())
        })();
        printer.finish(result)
    })
}

fn set(mut cfg: Config, args: &Args) -> Result<(), String> {
    let [key, value @ ..] = args.rest.as_slice() else {
        return Err("usage: warlogger set <key> <value>".into());
    };
    if key == "game" && Game::find(&value.join(" ")).is_none() {
        return Err(format!("unknown game {:?}", value.join(" ")));
    }

    cfg.set(key, &value.join(" "));
    cfg.save()
}

fn say(text: impl std::fmt::Display) -> Result<(), String> {
    println!("{text}");
    Ok(())
}

fn games() -> Result<(), String> {
    let rows: Vec<_> = GAMES
        .iter()
        .map(|g| format!("{:<11} {:<28} {}", g.id, g.name, g.host))
        .collect();
    say(rows.join("\n"))
}

fn main() -> ExitCode {
    #[cfg(windows)]
    if std::env::args().len() == 1 {
        return win::gui::run();
    }
    #[cfg(windows)]
    win::attach_console();

    #[cfg(unix)]
    unix::on_signals();
    #[cfg(windows)]
    win::on_ctrl_c();

    let cfg = Config::load();
    let result = parse().and_then(|args| match args.command.as_str() {
        "games" => games(),
        "login" => login(cfg, &args),
        "logs" => logs(&cfg, &args),
        "upload" => upload(cfg, args),
        "live" => live(cfg, args),
        "set" => set(cfg, &args),
        #[cfg(unix)]
        "stop" => unix::stop(),
        #[cfg(all(feature = "omarchy", target_os = "linux"))]
        "omarchy" => omarchy::run(args.rest.first().map(String::as_str)),
        "version" => say(concat!("warlogger ", env!("CARGO_PKG_VERSION"))),
        "" | "help" => say(USAGE),
        other => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    });

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
