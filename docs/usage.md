# Warlogger usage

## Apps

The Windows app and the macOS app work the same way.

1. Select the game.
2. Sign in.
3. Select the guild, the region and the visibility.
4. Select one or more logs.
5. Click **Upload selected**.

**Start live** uploads fights while you play. **Stop live** uploads the last fight and closes the report.

| Control | Function |
|---|---|
| **Merge selected** | Uploads all selected logs into one report. |
| **Skip trash** | Does not upload trash fights. |
| **Skip wipes** | Does not upload boss wipes. |
| **Skip failed keys** | Does not upload Mythic+ keys that are not timed or that are abandoned. |
| **Skip short** | Does not upload fights of less than 10 seconds. |
| **Fights** | Shows the fights in the selected log. Skipped fights are not selected. Select the fights, then click **Upload selected**. Click **Logs** to go back to the list of logs. |
| **Split** | Divides each selected log into smaller logs in the same directory. When **Skip short** is on, Split deletes parts of less than 10 seconds. Split does not change the original log. |

Split uses the same rules as the official uploader:

| Game | New part starts at |
|---|---|
| World of Warcraft | A new boss, a new Mythic+ key, or a break of 4 hours or more |
| FF XIV, Fellowship | A new zone, or a break of 4 hours or more |
| ESO | A new session |
| SWTOR | Not possible |

## Command line

```sh
warlogger login -g wow
warlogger logs
warlogger upload 1 3
warlogger upload latest -v unlisted -G "My Guild"
warlogger upload 1 2 3 --merge --no-trash --no-wipes
warlogger fights latest
warlogger upload latest --fights 2,5-7
warlogger split latest --no-short
warlogger live
warlogger stop
```

| Command | Function |
|---|---|
| `games` | Shows the sites and their ids. |
| `login` | Signs in and keeps the account for the site. |
| `logs` | Shows the log files of the game, newest first, with their numbers. |
| `upload [latest\|N\|path]...` | Uploads logs by number, by path, or the newest log. |
| `fights [latest\|N\|path]` | Shows the fights in a log with their numbers. A `-` marks a skipped fight. |
| `split [latest\|N\|path]...` | Divides logs into one file for each boss, key, zone or session. |
| `live [--all]` | Uploads fights while they occur, until you stop it. `--all` also uploads the fights that are already in the file. |
| `stop` | Stops `live` (Linux). |
| `update` | Downloads the newest release from GitHub and replaces the program. |
| `uninstall [--quiet]` | Removes the installed app, the Start menu shortcut, the settings and the saved passwords (Windows setup only). `--quiet` does not ask. |
| `set <key> <value>` | Keeps a default value for `game`, `guild`, `region`, `visibility`, `description` or `dir.<game>`. |

| Option | Function |
|---|---|
| `-g, --game <id>` | Site for the upload. |
| `-d, --dir <path>` | Log directory, if Warlogger cannot find it. |
| `-e, --email <email>` | Account email. |
| `-G, --guild <name or id>` | Guild. If you do not set it, the logs are personal. |
| `-r, --region <name or id>` | Region or server. |
| `-v, --visibility <name or id>` | Public, private or unlisted. |
| `-m, --desc <text>` | Report description. |
| `-M, --merge` | Uploads all given logs into one report. |
| `-f, --fights <list>` | Uploads only these fights. Use the numbers from `fights`, for example `1,3-5`. |
| `--no-trash` | Does not upload trash fights. |
| `--no-wipes` | Does not upload boss wipes. |
| `--no-failed` | Does not upload Mythic+ keys that are not timed or that are abandoned. |
| `--no-short` | Does not upload fights of less than 10 seconds. Split deletes parts of less than 10 seconds. |

`WARLOGGER_PASSWORD` replaces the saved password. The configuration file is `~/.config/warlogger/config` on Linux and `%APPDATA%\warlogger\config` on Windows. The macOS app keeps its settings in its user defaults.

## Updates

The Windows app and the macOS app look for a newer release on GitHub at start. To look again, click **Updates** (Windows) or **Check for updates** (macOS). The apps do not look for releases at other times.

- **Windows**: The app asks before it updates. Then it restarts. The setup version gets the setup file. The portable version gets the portable file.
- **macOS**: The app shows **Update and restart**. Click it to update.
- **Command line**: Run `warlogger update`.

## Uninstall

- **Windows setup**: Open **Settings → Apps → Installed apps**. Select **Warlogger** and click **Uninstall**. Or, run `warlogger uninstall`.
- **Windows portable**: Delete the file. To remove the settings, delete `%APPDATA%\warlogger`.

## Omarchy

`warlogger omarchy install` adds a widget to the bar. The widget shows the upload progress or **LIVE**. Its panel has:

- A game selector
- The newest logs. Click a log to upload it.
- The buttons Upload latest, Live / Stop live, Report, Login, Folder and About

`warlogger omarchy uninstall` removes the widget. During an upload, the CLI writes its status to `$XDG_RUNTIME_DIR/warlogger/status.json`. The widget reads this file.

## How it works

Warlogger uses the same `/desktop-client` endpoints as the official uploader.

1. Warlogger signs in with your email and password.
2. It downloads the parser of the site from `/desktop-client/parser`. It runs the parser in an embedded JavaScript engine: QuickJS on Windows and Linux, JavaScriptCore on macOS. `host/host.js` supplies the browser objects that the parser needs. Thus the parser is always current, and Warlogger does not need its own log format code.
3. It reads the log in parts of 4 MB and sends the lines to the parser. To skip or select fights, it first runs the parser in scan mode to get the list of fights. Then it sends the time ranges of the selected fights to the parser during the upload.
4. For each group of completed fights, it compresses the master table and the events. It uploads them as a report segment. Then it releases the memory.
5. At the end, it closes the report.

Only one part of the log and one segment are in memory at a time. In a test, a 40 MB log uploaded in 16 seconds with a maximum of 40 MB of memory.

Related projects: [combatlog](https://github.com/thevinter/combatlog), [ACTLogsUploader](https://github.com/zhui-zi/ACTLogsUploader) and [FFLogsUploader](https://github.com/Robert5204/FFLogsUploader).

## Build

```sh
cargo build --release
cargo build --release --features omarchy
macos/bundle.sh 0.4.1
```

## Disclaimer

Warlogger is not an official program. It uses a protocol that has no documentation. This protocol can change at any time. Use Warlogger at your own risk.
