use super::wide;
use crate::game::{GAMES, Game};
use crate::site::{Account, Choice, Site};
use crate::store::{self, Config};
use crate::upload::{Event, Fight, Filter, Session};
use crate::{Args, CANCEL};
use std::path::PathBuf;
use std::process::ExitCode;
use std::ptr::{null, null_mut};
use std::sync::atomic::Ordering;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    COLOR_WINDOW, CreateFontIndirectW, DeleteObject, GetSysColorBrush, HBRUSH, HDC, HFONT, SetBkMode, TRANSPARENT,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    EM_SETCUEBANNER, ICC_PROGRESS_CLASS, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx, PBM_SETPOS,
    PBM_SETRANGE32, PROGRESS_CLASSW,
};
use windows_sys::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
    SetProcessDpiAwarenessContext, SystemParametersInfoForDpi,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::Shell::{
    BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW, SHBrowseForFolderW, SHGetPathFromIDListW, ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const GAME: i32 = 10;
const EMAIL: i32 = 11;
const PASSWORD: i32 = 12;
const SIGN_IN: i32 = 13;
const GUILD: i32 = 14;
const REGION: i32 = 15;
const VISIBILITY: i32 = 16;
const FOLDER: i32 = 17;
const BROWSE: i32 = 18;
const LOGS: i32 = 19;
const REFRESH: i32 = 20;
const UPLOAD: i32 = 21;
const LIVE: i32 = 22;
const REPORT: i32 = 23;
const PROGRESS: i32 = 24;
const STATUS: i32 = 25;
const FIGHTS: i32 = 26;
const SPLIT: i32 = 27;
const SKIP_TRASH: i32 = 28;
const SKIP_WIPES: i32 = 29;
const SKIP_FAILED: i32 = 30;
const SKIP_SHORT: i32 = 31;
const MERGE: i32 = 32;

const CHECKS: [(i32, &str); 5] = [
    (SKIP_TRASH, "skip.trash"),
    (SKIP_WIPES, "skip.wipes"),
    (SKIP_FAILED, "skip.failed"),
    (SKIP_SHORT, "skip.short"),
    (MERGE, "merge"),
];

const WIDTH: i32 = 560;
const HEIGHT: i32 = 560;

const LAYOUT: &[(i32, &str, &str, u32, [i32; 4])] = &[
    (0, "STATIC", "Game", 0, [16, 20, 70, 20]),
    (
        GAME,
        "COMBOBOX",
        "",
        CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
        [90, 16, 454, 320],
    ),
    (0, "STATIC", "Account", 0, [16, 52, 70, 20]),
    (EMAIL, "EDIT", "", ES_AUTOHSCROLL as u32 | WS_BORDER, [90, 48, 170, 24]),
    (
        PASSWORD,
        "EDIT",
        "",
        ES_AUTOHSCROLL as u32 | ES_PASSWORD as u32 | WS_BORDER,
        [268, 48, 170, 24],
    ),
    (SIGN_IN, "BUTTON", "Sign in", BS_PUSHBUTTON as u32, [446, 47, 98, 26]),
    (0, "STATIC", "Report", 0, [16, 86, 70, 20]),
    (
        GUILD,
        "COMBOBOX",
        "",
        CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
        [90, 82, 170, 320],
    ),
    (
        REGION,
        "COMBOBOX",
        "",
        CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
        [268, 82, 80, 320],
    ),
    (
        VISIBILITY,
        "COMBOBOX",
        "",
        CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
        [356, 82, 188, 320],
    ),
    (0, "STATIC", "Folder", 0, [16, 120, 70, 20]),
    (
        FOLDER,
        "EDIT",
        "",
        ES_AUTOHSCROLL as u32 | ES_READONLY as u32 | WS_BORDER,
        [90, 116, 348, 24],
    ),
    (BROWSE, "BUTTON", "Browse…", BS_PUSHBUTTON as u32, [446, 115, 98, 26]),
    (
        LOGS,
        "LISTBOX",
        "",
        LBS_EXTENDEDSEL as u32 | LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32 | WS_VSCROLL | WS_BORDER,
        [16, 152, 528, 250],
    ),
    (
        SKIP_TRASH,
        "BUTTON",
        "Skip trash",
        BS_AUTOCHECKBOX as u32,
        [16, 410, 96, 22],
    ),
    (
        SKIP_WIPES,
        "BUTTON",
        "Skip wipes",
        BS_AUTOCHECKBOX as u32,
        [116, 410, 96, 22],
    ),
    (
        SKIP_FAILED,
        "BUTTON",
        "Skip failed keys",
        BS_AUTOCHECKBOX as u32,
        [216, 410, 124, 22],
    ),
    (
        SKIP_SHORT,
        "BUTTON",
        "Skip short",
        BS_AUTOCHECKBOX as u32,
        [344, 410, 90, 22],
    ),
    (
        MERGE,
        "BUTTON",
        "Merge selected",
        BS_AUTOCHECKBOX as u32,
        [438, 410, 106, 22],
    ),
    (REFRESH, "BUTTON", "Refresh", BS_PUSHBUTTON as u32, [16, 440, 72, 28]),
    (FIGHTS, "BUTTON", "Fights", BS_PUSHBUTTON as u32, [96, 440, 72, 28]),
    (SPLIT, "BUTTON", "Split", BS_PUSHBUTTON as u32, [176, 440, 64, 28]),
    (
        UPLOAD,
        "BUTTON",
        "Upload selected",
        BS_DEFPUSHBUTTON as u32,
        [248, 440, 112, 28],
    ),
    (LIVE, "BUTTON", "Start live", BS_PUSHBUTTON as u32, [368, 440, 84, 28]),
    (
        REPORT,
        "BUTTON",
        "Open report",
        BS_PUSHBUTTON as u32,
        [460, 440, 84, 28],
    ),
    (PROGRESS, "", "", 0, [16, 480, 528, 14]),
    (STATUS, "STATIC", "", 0, [16, 502, 528, 52]),
];

enum Msg {
    Account(Account),
    Event(Event),
    Fights(PathBuf, Vec<Fight>),
    Split(Result<String, String>),
    Done(Result<String, String>),
}

#[derive(PartialEq, Eq)]
enum Job {
    Idle,
    Busy,
    Live,
}

struct App {
    hwnd: HWND,
    font: HFONT,
    cfg: Config,
    account: Option<Account>,
    logs: Vec<PathBuf>,
    fights: Option<(PathBuf, Vec<Fight>)>,
    report: String,
    job: Job,
}

fn item(hwnd: HWND, id: i32) -> HWND {
    unsafe { GetDlgItem(hwnd, id) }
}

fn send(hwnd: HWND, msg: u32, w: usize, l: isize) -> isize {
    unsafe { SendMessageW(hwnd, msg, w, l) }
}

fn get_text(hwnd: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    let mut buf = vec![0u16; len as usize + 1];
    let read = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..read as usize])
}

fn set_text(hwnd: HWND, text: &str) {
    unsafe { SetWindowTextW(hwnd, wide(text).as_ptr()) };
}

fn fill(combo: HWND, items: impl IntoIterator<Item = String>, selected: Option<usize>) {
    send(combo, CB_RESETCONTENT, 0, 0);
    for text in items {
        send(combo, CB_ADDSTRING, 0, wide(&text).as_ptr() as isize);
    }
    send(combo, CB_SETCURSEL, selected.unwrap_or(usize::MAX), 0);
}

fn selection(combo: HWND) -> Option<usize> {
    usize::try_from(send(combo, CB_GETCURSEL, 0, 0)).ok()
}

fn post(hwnd: usize, msg: Msg) {
    let raw = Box::into_raw(Box::new(msg));
    if unsafe { PostMessageW(hwnd as HWND, WM_APP, 0, raw as isize) } == 0 {
        drop(unsafe { Box::from_raw(raw) });
    }
}

fn ago(log: &crate::game::LogFile) -> String {
    let minutes = log.modified.elapsed().map_or(0, |d| d.as_secs() / 60);
    match minutes {
        0..60 => format!("{minutes} min ago"),
        60..1440 => format!("{} h ago", minutes / 60),
        _ => format!("{} d ago", minutes / 1440),
    }
}

impl App {
    fn game(&self) -> &'static Game {
        self.cfg.get("game").and_then(Game::find).unwrap_or(&GAMES[0])
    }

    fn email_key(&self) -> String {
        format!("email.{}", self.game().site())
    }

    fn dir(&self) -> Option<PathBuf> {
        let game = self.game();
        self.cfg
            .get(&format!("dir.{}", game.id))
            .map(PathBuf::from)
            .or_else(|| game.detect())
    }

    fn checked(&self, id: i32) -> bool {
        send(item(self.hwnd, id), BM_GETCHECK, 0, 0) == 1
    }

    fn filter(&self) -> Filter {
        Filter {
            trash: self.checked(SKIP_TRASH),
            wipes: self.checked(SKIP_WIPES),
            failed: self.checked(SKIP_FAILED),
            short: self.checked(SKIP_SHORT),
            fights: None,
        }
    }

    fn picked(&self) -> Vec<usize> {
        let list = item(self.hwnd, LOGS);
        let count = send(list, LB_GETSELCOUNT, 0, 0).max(0) as usize;
        let mut picked = vec![0i32; count];
        send(list, LB_GETSELITEMS, count, picked.as_mut_ptr() as isize);
        picked.into_iter().map(|i| i as usize).collect()
    }

    fn status(&self, text: &str) {
        set_text(item(self.hwnd, STATUS), text);
    }

    fn save(&self) {
        if let Err(e) = self.cfg.save() {
            self.status(&e);
        }
    }

    fn layout(&mut self) {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) } as i32;
        let scale = |v: i32| v * dpi / 96;

        unsafe {
            let mut metrics: NONCLIENTMETRICSW = std::mem::zeroed();
            metrics.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
            SystemParametersInfoForDpi(
                SPI_GETNONCLIENTMETRICS,
                metrics.cbSize,
                (&raw mut metrics).cast(),
                0,
                dpi as u32,
            );
            let font = CreateFontIndirectW(&metrics.lfMessageFont);
            if !self.font.is_null() {
                DeleteObject(self.font);
            }
            self.font = font;

            let mut rect = RECT {
                left: 0,
                top: 0,
                right: scale(WIDTH),
                bottom: scale(HEIGHT),
            };
            let style = GetWindowLongW(self.hwnd, GWL_STYLE) as u32;
            AdjustWindowRectExForDpi(&mut rect, style, 0, 0, dpi as u32);
            SetWindowPos(
                self.hwnd,
                null_mut(),
                0,
                0,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOMOVE | SWP_NOZORDER,
            );

            let mut child = GetWindow(self.hwnd, GW_CHILD);
            for &(_, _, _, _, [x, y, w, h]) in LAYOUT {
                SetWindowPos(child, null_mut(), scale(x), scale(y), scale(w), scale(h), SWP_NOZORDER);
                send(child, WM_SETFONT, font as usize, 1);
                child = GetWindow(child, GW_HWNDNEXT);
            }
        }
    }

    fn create(&mut self) {
        let instance = unsafe { GetModuleHandleW(null()) };
        for &(id, class, text, style, _) in LAYOUT {
            let name = wide(class);
            let class = if id == PROGRESS { PROGRESS_CLASSW } else { name.as_ptr() };
            let tab = if matches!(id, 0 | STATUS | PROGRESS) {
                0
            } else {
                WS_TABSTOP
            };
            unsafe {
                CreateWindowExW(
                    0,
                    class,
                    wide(text).as_ptr(),
                    WS_CHILD | WS_VISIBLE | tab | style,
                    0,
                    0,
                    0,
                    0,
                    self.hwnd,
                    id as usize as HMENU,
                    instance,
                    null(),
                );
            }
        }

        send(
            item(self.hwnd, EMAIL),
            EM_SETCUEBANNER,
            1,
            wide("Email").as_ptr() as isize,
        );
        send(item(self.hwnd, PROGRESS), PBM_SETRANGE32, 0, 1000);
        for (id, key) in CHECKS {
            let on = self.cfg.get(key) == Some("1");
            send(item(self.hwnd, id), BM_SETCHECK, on as usize, 0);
        }
        fill(
            item(self.hwnd, GAME),
            GAMES.iter().map(|g| format!("{} — {}", g.name, g.host)),
            GAMES.iter().position(|g| g == self.game()),
        );

        self.layout();
        self.switch_game();
    }

    fn switch_game(&mut self) {
        let game = self.game();
        let email = self.cfg.get(&self.email_key()).unwrap_or_default().to_owned();
        let saved = !email.is_empty() && store::load_secret(&email).is_some();

        set_text(item(self.hwnd, EMAIL), &email);
        set_text(item(self.hwnd, PASSWORD), "");
        let cue = if saved { "Saved password" } else { "Password" };
        send(
            item(self.hwnd, PASSWORD),
            EM_SETCUEBANNER,
            1,
            wide(cue).as_ptr() as isize,
        );

        self.account = None;
        for id in [GUILD, REGION, VISIBILITY] {
            fill(item(self.hwnd, id), [], None);
        }

        self.refresh();
        if saved {
            self.sign_in();
        } else {
            self.status(&format!("Sign in to {} to upload {} logs", game.host, game.name));
        }
    }

    fn refresh(&mut self) {
        let list = item(self.hwnd, LOGS);
        send(list, LB_RESETCONTENT, 0, 0);
        self.logs.clear();
        self.fights = None;
        set_text(item(self.hwnd, FIGHTS), "Fights");

        let Some(dir) = self.dir() else {
            set_text(item(self.hwnd, FOLDER), "");
            self.status(&format!(
                "No {} log folder found; choose one with Browse",
                self.game().name
            ));
            return;
        };

        set_text(item(self.hwnd, FOLDER), &dir.display().to_string());
        for log in self.game().logs(&dir) {
            let name = log.path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let row = format!("{name}    {:.1} MB    {}", log.size as f64 / 1e6, ago(&log));
            send(list, LB_ADDSTRING, 0, wide(&row).as_ptr() as isize);
            self.logs.push(log.path);
        }
        if !self.logs.is_empty() {
            send(list, LB_SETSEL, 1, 0);
        }
    }

    fn select_fights(&self) {
        let Some((_, fights)) = &self.fights else { return };
        let filter = self.filter();
        for (i, fight) in fights.iter().enumerate() {
            send(
                item(self.hwnd, LOGS),
                LB_SETSEL,
                filter.keep(self.game(), fight) as usize,
                i as isize,
            );
        }
    }

    fn show_fights(&mut self, path: PathBuf, fights: Vec<Fight>) {
        let list = item(self.hwnd, LOGS);
        send(list, LB_RESETCONTENT, 0, 0);
        for fight in &fights {
            send(list, LB_ADDSTRING, 0, wide(&fight.label(self.game())).as_ptr() as isize);
        }

        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        self.status(&format!("{} fights in {name}; pick the ones to upload", fights.len()));
        set_text(item(self.hwnd, FIGHTS), "Logs");
        self.fights = Some((path, fights));
        self.select_fights();
        self.set_job(Job::Idle);
    }

    fn check(&mut self, id: i32) {
        if let Some((_, key)) = CHECKS.iter().find(|(c, _)| *c == id) {
            self.cfg.set(key, if self.checked(id) { "1" } else { "0" });
            self.save();
        }
        self.select_fights();
    }

    fn scan(&mut self) {
        if self.fights.is_some() {
            return self.refresh();
        }
        let Some((email, password)) = self.credentials() else {
            return self.status("Sign in first");
        };
        let [path] = self.picked()[..] else {
            return self.status("Select one log to list its fights");
        };
        let Some(path) = self.logs.get(path).cloned() else {
            return;
        };

        let game = self.game();
        let hwnd = self.hwnd as usize;
        let cfg = Config::load();
        CANCEL.store(false, Ordering::Relaxed);
        self.set_job(Job::Busy);

        let worker = std::thread::Builder::new().stack_size(32 << 20).spawn(move || {
            let mut notify = |e| post(hwnd, Msg::Event(e));
            let result = (|| {
                let mut session = Session::open(game, &email, &password, &mut notify)?;
                let target = crate::target(&cfg, &Args::default(), &session.account)?;
                notify(Event::Step("scanning for fights".into()));
                session.scan(&path, &target.region, &CANCEL, &mut notify)
            })();
            match result {
                Ok(fights) => post(hwnd, Msg::Fights(path, fights)),
                Err(e) => post(hwnd, Msg::Done(Err(e))),
            }
        });

        if let Err(e) = worker {
            self.status(&e.to_string());
            self.set_job(Job::Idle);
        }
    }

    fn split(&mut self) {
        if self.fights.is_some() {
            return self.status("Press Logs to go back to the log list first");
        }
        let files: Vec<PathBuf> = self
            .picked()
            .iter()
            .filter_map(|&i| self.logs.get(i).cloned())
            .collect();
        if files.is_empty() {
            return self.status("Select one or more logs to split");
        }

        let game = self.game();
        let short = self.checked(SKIP_SHORT);
        let hwnd = self.hwnd as usize;
        CANCEL.store(false, Ordering::Relaxed);
        self.set_job(Job::Busy);
        self.status("Splitting…");

        std::thread::spawn(move || {
            let mut notify = |e| post(hwnd, Msg::Event(e));
            let result = files.iter().try_fold(0, |n, f| {
                crate::split::split(game, f, short, &CANCEL, &mut notify).map(|p| n + p.len())
            });
            post(hwnd, Msg::Split(result.map(|n| format!("Split into {n} logs"))));
        });
    }

    fn browse(&mut self) {
        let title = wide(&format!("Choose the {} log folder", self.game().name));
        let mut info: BROWSEINFOW = unsafe { std::mem::zeroed() };
        info.hwndOwner = self.hwnd;
        info.lpszTitle = title.as_ptr();
        info.ulFlags = BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE;

        unsafe {
            let list = SHBrowseForFolderW(&info);
            if list.is_null() {
                return;
            }

            let mut path = [0u16; 1024];
            let ok = SHGetPathFromIDListW(list, path.as_mut_ptr());
            CoTaskMemFree(list.cast());
            if ok == 0 {
                return;
            }

            let len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
            let dir = String::from_utf16_lossy(&path[..len]);
            self.cfg.set(&format!("dir.{}", self.game().id), &dir);
        }

        self.save();
        self.refresh();
    }

    fn credentials(&self) -> Option<(String, String)> {
        let email = get_text(item(self.hwnd, EMAIL)).trim().to_owned();
        let typed = get_text(item(self.hwnd, PASSWORD));
        let password = if typed.is_empty() {
            store::load_secret(&email)?
        } else {
            typed
        };
        (!email.is_empty()).then_some((email, password))
    }

    fn set_job(&mut self, job: Job) {
        let idle = job == Job::Idle;
        for id in [
            GAME,
            EMAIL,
            PASSWORD,
            SIGN_IN,
            GUILD,
            REGION,
            VISIBILITY,
            BROWSE,
            REFRESH,
            UPLOAD,
            FIGHTS,
            SPLIT,
            SKIP_TRASH,
            SKIP_WIPES,
            SKIP_FAILED,
            SKIP_SHORT,
            MERGE,
        ] {
            unsafe { EnableWindow(item(self.hwnd, id), idle as i32) };
        }
        unsafe { EnableWindow(item(self.hwnd, LIVE), (job != Job::Busy) as i32) };
        set_text(
            item(self.hwnd, LIVE),
            if job == Job::Live { "Stop live" } else { "Start live" },
        );
        self.job = job;
    }

    fn sign_in(&mut self) {
        let Some((email, password)) = self.credentials() else {
            self.status("Enter your email and password");
            return;
        };

        let game = self.game();
        let hwnd = self.hwnd as usize;
        self.set_job(Job::Busy);
        self.status(&format!("Signing in to {}…", game.host));

        std::thread::spawn(move || {
            let result = Site::new(game).login(&email, &password).and_then(|account| {
                store::save_secret(&email, &password)?;
                Ok(account)
            });

            match result {
                Ok(account) => post(hwnd, Msg::Account(account)),
                Err(e) => post(hwnd, Msg::Done(Err(e))),
            }
        });
    }

    fn signed_in(&mut self, account: Account) {
        let email = get_text(item(self.hwnd, EMAIL)).trim().to_owned();
        self.cfg.set(&self.email_key(), &email);
        self.save();

        for (id, key, list) in [
            (GUILD, "guild", &account.guilds),
            (REGION, "region", &account.regions),
            (VISIBILITY, "visibility", &account.visibilities),
        ] {
            let wanted = self.cfg.get(key).unwrap_or_default();
            let at = list
                .iter()
                .position(|c| c.value.to_string().trim_matches('"') == wanted)
                .or((!list.is_empty()).then_some(0));
            fill(item(self.hwnd, id), list.iter().map(|c| c.label.clone()), at);
        }

        set_text(item(self.hwnd, PASSWORD), "");
        send(
            item(self.hwnd, PASSWORD),
            EM_SETCUEBANNER,
            1,
            wide("Saved password").as_ptr() as isize,
        );
        self.status(&format!("Signed in as {}", account.name));
        self.account = Some(account);
        self.set_job(Job::Idle);
    }

    fn choose(&mut self, id: i32) {
        let Some(account) = &self.account else { return };
        let (key, list): (&str, &[Choice]) = match id {
            GUILD => ("guild", &account.guilds),
            REGION => ("region", &account.regions),
            _ => ("visibility", &account.visibilities),
        };

        if let Some(choice) = selection(item(self.hwnd, id)).and_then(|i| list.get(i)) {
            let value = choice.value.to_string().trim_matches('"').to_owned();
            self.cfg.set(key, &value);
            self.save();
        }
    }

    fn start(&mut self, live: bool) {
        let Some((email, password)) = self.credentials() else {
            self.status("Sign in first");
            return;
        };

        let mut filter = self.filter();
        let merge = self.checked(MERGE);
        let files: Vec<PathBuf> = match &self.fights {
            Some((path, fights)) => {
                let ranges = self.picked().into_iter().filter_map(|i| fights.get(i));
                filter.fights = Some(ranges.map(|f| (f.start, f.end)).collect());
                match filter.fights.as_ref().is_some_and(|f| !f.is_empty()) {
                    true => vec![path.clone()],
                    false => Vec::new(),
                }
            }
            None => self
                .picked()
                .iter()
                .filter_map(|&i| self.logs.get(i).cloned())
                .collect(),
        };

        let dir = self.dir();
        match (live, files.is_empty(), &dir) {
            (false, true, _) => return self.status("Select one or more logs to upload"),
            (true, _, None) => return self.status("Choose the log folder first"),
            _ => {}
        }

        let game = self.game();
        let hwnd = self.hwnd as usize;
        let cfg = Config::load();
        CANCEL.store(false, Ordering::Relaxed);
        self.set_job(if live { Job::Live } else { Job::Busy });
        send(item(self.hwnd, PROGRESS), PBM_SETPOS, 0, 0);

        let worker = std::thread::Builder::new().stack_size(32 << 20).spawn(move || {
            let mut notify = |e| post(hwnd, Msg::Event(e));
            let result = (|| {
                let mut session = Session::open(game, &email, &password, &mut notify)?;
                let target = crate::target(&cfg, &Args::default(), &session.account)?;

                match (live, dir) {
                    (true, Some(dir)) => session.live(&dir, &target, false, &CANCEL, &mut notify),
                    _ if merge => session.upload(&files, &target, &filter, &CANCEL, &mut notify),
                    _ => files.iter().try_fold(String::new(), |_, f| {
                        session.upload(std::slice::from_ref(f), &target, &filter, &CANCEL, &mut notify)
                    }),
                }
            })();
            post(hwnd, Msg::Done(result));
        });

        if let Err(e) = worker {
            self.status(&e.to_string());
            self.set_job(Job::Idle);
        }
    }

    fn message(&mut self, msg: Msg) {
        match msg {
            Msg::Account(account) => self.signed_in(account),
            Msg::Event(Event::Step(text)) => self.status(&text),
            Msg::Event(Event::Progress(p)) => {
                send(item(self.hwnd, PROGRESS), PBM_SETPOS, (p * 1000.0) as usize, 0);
            }
            Msg::Event(Event::Report(url)) => self.report = url,
            Msg::Fights(path, fights) => self.show_fights(path, fights),
            Msg::Split(result) => {
                self.set_job(Job::Idle);
                self.refresh();
                self.status(&result.unwrap_or_else(|e| e));
            }
            Msg::Done(Ok(url)) => {
                self.status(&format!("Done: {url}"));
                self.set_job(Job::Idle);
            }
            Msg::Done(Err(e)) => {
                self.status(&e);
                self.set_job(Job::Idle);
            }
        }
    }

    fn command(&mut self, id: i32, code: u32) {
        match (id, code) {
            (GAME, CBN_SELCHANGE) => {
                if let Some(game) = selection(item(self.hwnd, GAME)).and_then(|i| GAMES.get(i)) {
                    self.cfg.set("game", game.id);
                    self.save();
                    self.switch_game();
                }
            }
            (GUILD | REGION | VISIBILITY, CBN_SELCHANGE) => self.choose(id),
            (SIGN_IN, BN_CLICKED) => self.sign_in(),
            (BROWSE, BN_CLICKED) => self.browse(),
            (REFRESH, BN_CLICKED) => self.refresh(),
            (FIGHTS, BN_CLICKED) => self.scan(),
            (SPLIT, BN_CLICKED) => self.split(),
            (SKIP_TRASH | SKIP_WIPES | SKIP_FAILED | SKIP_SHORT | MERGE, BN_CLICKED) => self.check(id),
            (UPLOAD, BN_CLICKED) => self.start(false),
            (LOGS, LBN_DBLCLK) => self.start(false),
            (LIVE, BN_CLICKED) if self.job == Job::Live => {
                CANCEL.store(true, Ordering::Relaxed);
                self.status("Stopping; uploading the last fight…");
            }
            (LIVE, BN_CLICKED) => self.start(true),
            (REPORT, BN_CLICKED) if !self.report.is_empty() => unsafe {
                ShellExecuteW(
                    self.hwnd,
                    wide("open").as_ptr(),
                    wide(&self.report).as_ptr(),
                    null(),
                    null(),
                    SW_SHOWNORMAL,
                );
            },
            _ => {}
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let app = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut App;

    if msg == WM_NCCREATE {
        let create = unsafe { &*(l as *const CREATESTRUCTW) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
        let app = unsafe { &mut *(create.lpCreateParams as *mut App) };
        app.hwnd = hwnd;
        return unsafe { DefWindowProcW(hwnd, msg, w, l) };
    }

    let Some(app) = (unsafe { app.as_mut() }) else {
        return unsafe { DefWindowProcW(hwnd, msg, w, l) };
    };

    match msg {
        WM_CREATE => app.create(),
        WM_COMMAND => app.command((w & 0xFFFF) as i32, ((w >> 16) & 0xFFFF) as u32),
        WM_APP => app.message(*unsafe { Box::from_raw(l as *mut Msg) }),
        WM_DPICHANGED => app.layout(),
        WM_CTLCOLORSTATIC => unsafe {
            SetBkMode(w as HDC, TRANSPARENT as i32);
            return GetSysColorBrush(COLOR_WINDOW) as LRESULT;
        },
        WM_CLOSE if app.job == Job::Live => {
            CANCEL.store(true, Ordering::Relaxed);
            app.status("Finishing the live report before closing…");
            app.job = Job::Busy;
            unsafe { SetTimer(hwnd, 1, 500, None) };
        }
        WM_TIMER if app.job == Job::Idle => unsafe {
            DestroyWindow(hwnd);
        },
        WM_DESTROY => unsafe { PostQuitMessage(0) },
        _ => return unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
    0
}

pub fn run() -> ExitCode {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let controls = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_PROGRESS_CLASS | ICC_STANDARD_CLASSES,
        };
        InitCommonControlsEx(&controls);

        let instance = GetModuleHandleW(null());
        let class = wide("warlogger");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: LoadIconW(instance, 1 as _),
            hbrBackground: (COLOR_WINDOW + 1) as usize as HBRUSH,
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);

        let app = Box::into_raw(Box::new(App {
            hwnd: null_mut(),
            font: null_mut(),
            cfg: Config::load(),
            account: None,
            logs: Vec::new(),
            fights: None,
            report: String::new(),
            job: Job::Idle,
        }));

        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide("Warlogger").as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            app.cast(),
        );
        ShowWindow(hwnd, SW_SHOW);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        drop(Box::from_raw(app));
    }
    ExitCode::SUCCESS
}
