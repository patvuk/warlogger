<h1 align="center">Warlogger</h1>

<p align="center">A small native uploader for Warcraft Logs, FF Logs and the other RPGLogs sites</p>

<p align="center">
  <a href="https://github.com/patvuk/warlogger/actions/workflows/ci.yml"><img src="https://github.com/patvuk/warlogger/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <a href="https://github.com/patvuk/warlogger/releases/latest"><img src="https://img.shields.io/github/v/release/patvuk/warlogger" alt="Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/patvuk/warlogger" alt="License"></a>
</p>

## Sites

| Id | Game | Site | Log files |
|---|---|---|---|
| `wow` | World of Warcraft | warcraftlogs.com | `WoWCombatLog*.txt` |
| `classic` | WoW Classic | classic.warcraftlogs.com | `WoWCombatLog*.txt` |
| `fresh` | WoW Anniversary | fresh.warcraftlogs.com | `WoWCombatLog*.txt` |
| `sod` | WoW Season of Discovery | sod.warcraftlogs.com | `WoWCombatLog*.txt` |
| `vanilla` | WoW Classic Era | vanilla.warcraftlogs.com | `WoWCombatLog*.txt` |
| `titan` | WoW Titan Reforged | titan.warcraftlogs.com | `WoWCombatLog*.txt` |
| `ff` | Final Fantasy XIV | fflogs.com | `Network_*.log` from ACT or IINACT |
| `eso` | Elder Scrolls Online | esologs.com | `Encounter.log` |
| `swtor` | Star Wars: The Old Republic | swtorlogs.com | `combat_*.txt` |
| `fellowship` | Fellowship | fellowshiplogs.com | `CombatLog*.txt` |

Each site has its own account. Warlogger remembers one email per site and keeps the password in the system keychain.

## Platforms

| | Built with | App |
|---|---|---|
| Windows | Rust, Win32, WinHTTP, Credential Manager | window when started without arguments, CLI with them |
| Linux | Rust, rustls, Secret Service | CLI, plus an Omarchy bar widget |
| macOS | Swift, SwiftUI, URLSession, JavaScriptCore, Keychain | menu bar app |

## Install

Download your platform's archive from [Releases](../../releases).

- **Windows**: run `warlogger.exe`.
- **macOS**: unzip and move `Warlogger.app` to Applications. The app isn't notarized, so macOS blocks it the first time. Either open **System Settings → Privacy & Security** and click **Open Anyway**, or run `xattr -dr com.apple.quarantine /Applications/Warlogger.app`. It runs in the menu bar, with no Dock icon.
- **Linux**: put `warlogger` on your `PATH`.
- **Omarchy**: use the `-omarchy` archive, put `warlogger` on your `PATH`, then run `warlogger omarchy install`.

## Use

The Windows and macOS apps work the same way: pick the game, sign in, choose the guild, region and visibility, select one or more logs, then press **Upload selected**. **Start live** uploads fights while you play, and **Stop live** uploads the last fight and closes the report.

On the command line:

```sh
warlogger login -g wow
warlogger logs
warlogger upload 1 3
warlogger upload latest -v unlisted -G "My Guild"
warlogger live
warlogger stop
```

| Command | What it does |
|---|---|
| `games` | lists the sites and their ids |
| `login` | signs in and saves the account for that site |
| `logs` | lists the game's log files, newest first, with the number `upload` takes |
| `upload [latest\|N\|path]...` | uploads logs by number, by path, or the newest one |
| `live [--all]` | uploads fights as they happen until stopped; `--all` also includes fights already in the file |
| `stop` | stops `live` (Linux) |
| `set <key> <value>` | saves a default: `game`, `guild`, `region`, `visibility`, `description`, `dir.<game>` |

| Option | |
|---|---|
| `-g, --game <id>` | site to upload to |
| `-d, --dir <path>` | log folder, when it isn't found on its own |
| `-e, --email <email>` | account email |
| `-G, --guild <name or id>` | guild, personal logs when unset |
| `-r, --region <name or id>` | region or server |
| `-v, --visibility <name or id>` | public, private or unlisted |
| `-m, --desc <text>` | report description |

`WARLOGGER_PASSWORD` overrides the saved password. The config lives in `~/.config/warlogger/config` on Linux and `%APPDATA%\warlogger\config` on Windows. The macOS app keeps its settings in its user defaults.

## Omarchy

`warlogger omarchy install` adds a bar widget. It shows the upload progress or **LIVE** in the bar, and its panel has:

- a game switcher
- the newest logs (click one to upload it)
- Upload latest, Live / Stop live, Report, Login, Folder and About buttons

`warlogger omarchy uninstall` removes it. During uploads, the CLI writes its state to `$XDG_RUNTIME_DIR/warlogger/status.json` for the widget to read.

## How it works

Warlogger uses the same `/desktop-client` endpoints as the official uploader:

1. It signs in with your email and password.
2. It downloads the site's own parser from `/desktop-client/parser` and runs it in an embedded JavaScript engine (QuickJS on Windows and Linux, JavaScriptCore on macOS). `host/host.js` provides the few browser objects the parser expects. The parser is always current, and nothing about the log format is reimplemented.
3. It reads the log in 4 MB chunks and feeds the lines to the parser.
4. For each batch of finished fights, it zips the master table and the events, uploads them as a report segment, and frees them.
5. It closes the report when done.

Only one chunk and one segment are in memory at a time. In testing, a 40 MB log uploaded in 16 seconds and peaked at 40 MB of resident memory.

Prior art: [combatlog](https://github.com/thevinter/combatlog), [ACTLogsUploader](https://github.com/zhui-zi/ACTLogsUploader) and [FFLogsUploader](https://github.com/Robert5204/FFLogsUploader).

## Build

```sh
cargo build --release
cargo build --release --features omarchy
macos/bundle.sh 0.1.0
```

## Disclaimer

Warlogger is unofficial. It uses an undocumented protocol, which can change at any time, so use it at your own risk.
