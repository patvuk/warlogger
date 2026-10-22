<p align="center"><img src="assets/logo.svg" width="96" height="96" alt="Warlogger logo"></p>

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
| `forever` | WoW Forever | forever.warcraftlogs.com | `WoWCombatLog*.txt` |
| `titan` | WoW Titan Reforged | titan.warcraftlogs.com | `WoWCombatLog*.txt` |
| `ff` | Final Fantasy XIV | fflogs.com | `Network_*.log` from ACT or IINACT |
| `eso` | Elder Scrolls Online | esologs.com | `Encounter.log` |
| `swtor` | Star Wars: The Old Republic | swtorlogs.com | `combat_*.txt` |
| `fellowship` | Fellowship | fellowshiplogs.com | `CombatLog*.txt` |

Each site has a different account. Warlogger keeps one email for each site. It keeps the password in the system keychain.

## Platforms

| | Built with | App |
|---|---|---|
| Windows | Rust, Win32, WinHTTP, Credential Manager | Window without arguments, CLI with arguments |
| Linux | Rust, rustls, Secret Service | CLI and an Omarchy bar widget |
| macOS | Swift, SwiftUI, URLSession, JavaScriptCore, Keychain | Menu bar app |

## Install

Download the file for your platform from [Releases](../../releases).

- **Windows setup**: Start `warlogger-*-setup.exe`. Select the install location and click **Install**. Setup adds a Start menu shortcut and an uninstaller in **Installed apps**.
- **Windows portable**: Start `warlogger-*-portable.exe`. It does not install.
- **macOS**: Unzip the file. Move `Warlogger.app` to Applications. The app is not notarized, thus macOS stops it the first time. Open **System Settings → Privacy & Security** and click **Open Anyway**. Or, run `xattr -dr com.apple.quarantine /Applications/Warlogger.app`. The app shows in the menu bar, not in the Dock.
- **Linux**: Put `warlogger` in a directory on your `PATH`.
- **Omarchy**: Use the `-omarchy` file. Put `warlogger` in a directory on your `PATH`. Then run `warlogger omarchy install`.

## Use

Refer to [docs/usage.md](docs/usage.md).
