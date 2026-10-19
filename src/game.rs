use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Game {
    pub id: &'static str,
    pub name: &'static str,
    pub host: &'static str,
    pub prefix: &'static str,
    pub suffix: &'static str,
    pub latin1: bool,
    pub dirs: &'static [&'static str],
}

macro_rules! wow {
    ($id:literal, $name:literal, $host:literal, $flavor:literal) => {
        Game {
            id: $id,
            name: $name,
            host: $host,
            prefix: "WoWCombatLog",
            suffix: ".txt",
            latin1: false,
            dirs: &[
                concat!("C:/Program Files (x86)/World of Warcraft/", $flavor, "/Logs"),
                concat!("C:/Program Files/World of Warcraft/", $flavor, "/Logs"),
                concat!("/Applications/World of Warcraft/", $flavor, "/Logs"),
                concat!(
                    "~/Games/battlenet/drive_c/Program Files (x86)/World of Warcraft/",
                    $flavor,
                    "/Logs"
                ),
                concat!(
                    "~/Games/world-of-warcraft/drive_c/Program Files (x86)/World of Warcraft/",
                    $flavor,
                    "/Logs"
                ),
                concat!(
                    "~/.wine/drive_c/Program Files (x86)/World of Warcraft/",
                    $flavor,
                    "/Logs"
                ),
                concat!(
                    "~/.local/share/Steam/steamapps/compatdata/*/pfx/drive_c/Program Files (x86)/World of Warcraft/",
                    $flavor,
                    "/Logs"
                ),
            ],
        }
    };
}

macro_rules! docs {
    ($sub:literal, $steam:literal) => {
        &[
            concat!("~/Documents/", $sub),
            concat!("~/OneDrive/Documents/", $sub),
            concat!(
                "~/.local/share/Steam/steamapps/compatdata/",
                $steam,
                "/pfx/drive_c/users/steamuser/Documents/",
                $sub
            ),
            concat!(
                "~/.steam/steam/steamapps/compatdata/",
                $steam,
                "/pfx/drive_c/users/steamuser/Documents/",
                $sub
            ),
        ]
    };
}

pub const GAMES: &[Game] = &[
    wow!("wow", "World of Warcraft", "www.warcraftlogs.com", "_retail_"),
    wow!("classic", "WoW Classic", "classic.warcraftlogs.com", "_classic_"),
    wow!("fresh", "WoW Anniversary", "fresh.warcraftlogs.com", "_anniversary_"),
    wow!(
        "sod",
        "WoW Season of Discovery",
        "sod.warcraftlogs.com",
        "_classic_era_"
    ),
    wow!(
        "vanilla",
        "WoW Classic Era",
        "vanilla.warcraftlogs.com",
        "_classic_era_"
    ),
    wow!(
        "titan",
        "WoW Titan Reforged",
        "titan.warcraftlogs.com",
        "_classic_titan_"
    ),
    Game {
        id: "ff",
        name: "Final Fantasy XIV",
        host: "www.fflogs.com",
        prefix: "Network_",
        suffix: ".log",
        latin1: false,
        dirs: &[
            "%APPDATA%/Advanced Combat Tracker/FFXIVLogs",
            "~/Documents/IINACT",
            "~/OneDrive/Documents/IINACT",
            "~/.xlcore/wineprefix/drive_c/users/*/Documents/IINACT",
            "~/.xlcore/wineprefix/drive_c/users/*/AppData/Roaming/Advanced Combat Tracker/FFXIVLogs",
        ],
    },
    Game {
        id: "eso",
        name: "Elder Scrolls Online",
        host: "www.esologs.com",
        prefix: "Encounter",
        suffix: ".log",
        latin1: false,
        dirs: docs!("Elder Scrolls Online/live/Logs", "306130"),
    },
    Game {
        id: "swtor",
        name: "Star Wars: The Old Republic",
        host: "www.swtorlogs.com",
        prefix: "combat_",
        suffix: ".txt",
        latin1: true,
        dirs: docs!("Star Wars - The Old Republic/CombatLogs", "1286830"),
    },
    Game {
        id: "fellowship",
        name: "Fellowship",
        host: "www.fellowshiplogs.com",
        prefix: "CombatLog",
        suffix: ".txt",
        latin1: false,
        dirs: &[
            "C:/Program Files (x86)/Steam/steamapps/common/Fellowship/fellowship/Saved/CombatLogs",
            "~/.local/share/Steam/steamapps/common/Fellowship/fellowship/Saved/CombatLogs",
            "~/.steam/steam/steamapps/common/Fellowship/fellowship/Saved/CombatLogs",
        ],
    },
];

pub struct LogFile {
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
}

impl Game {
    pub fn find(id: &str) -> Option<&'static Game> {
        GAMES.iter().find(|g| g.id.eq_ignore_ascii_case(id))
    }

    pub fn site(&self) -> &'static str {
        self.host.split_once('.').map_or(self.host, |(_, rest)| rest)
    }

    pub fn base(&self) -> String {
        format!("https://{}", self.host)
    }

    pub fn matches(&self, name: &str) -> bool {
        name.len() >= self.prefix.len() + self.suffix.len()
            && name.starts_with(self.prefix)
            && name.ends_with(self.suffix)
    }

    pub fn start_date(&self, path: &Path) -> Option<String> {
        if self.id != "swtor" {
            return None;
        }

        let name = path.file_stem()?.to_str()?;
        let date = name.strip_prefix(self.prefix)?.get(..10)?;
        let [y, m, d] = [&date[..4], date.get(5..7)?, date.get(8..10)?];
        Some(format!("{m}/{d}/{y}"))
    }

    pub fn detect(&self) -> Option<PathBuf> {
        self.dirs.iter().flat_map(|d| expand(d)).find(|p| p.is_dir())
    }

    pub fn logs(&self, dir: &Path) -> Vec<LogFile> {
        let mut out: Vec<LogFile> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| self.matches(&e.file_name().to_string_lossy()))
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                meta.is_file().then(|| LogFile {
                    path: e.path(),
                    size: meta.len(),
                    modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                })
            })
            .collect();

        out.sort_by_key(|l| std::cmp::Reverse(l.modified));
        out
    }
}

fn expand(template: &str) -> Vec<PathBuf> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);

    let root = match template.split('/').next() {
        Some("~") => home,
        Some("%APPDATA%") => appdata,
        Some(drive) if drive.ends_with(':') => cfg!(windows).then(|| PathBuf::from(format!("{drive}\\"))),
        Some("") => cfg!(unix).then(|| PathBuf::from("/")),
        _ => None,
    };

    let mut paths: Vec<PathBuf> = root.into_iter().collect();
    for part in template.split('/').skip(1).filter(|p| !p.is_empty()) {
        paths = paths
            .into_iter()
            .flat_map(|p| match part {
                "*" => std::fs::read_dir(&p)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .collect(),
                _ => vec![p.join(part)],
            })
            .collect();
    }

    paths
}
