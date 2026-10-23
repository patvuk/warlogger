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
    COLOR_WINDOW, CreateFontIndirectW, DeleteObject, FW_BOLD, GetSysColorBrush, HBRUSH, HDC, HFONT, SelectObject,
    SetBkMode, TRANSPARENT,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    CDDS_ITEMPREPAINT, CDDS_PREPAINT, CDRF_DODEFAULT, CDRF_NEWFONT, CDRF_NOTIFYITEMDRAW, EM_SETCUEBANNER,
    ICC_LISTVIEW_CLASSES, ICC_PROGRESS_CLASS, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx,
    LVCF_TEXT, LVCF_WIDTH, LVCOLUMNW, LVIF_TEXT, LVIS_SELECTED, LVITEMW, LVM_DELETEALLITEMS, LVM_DELETECOLUMN,
    LVM_GETITEMCOUNT, LVM_GETNEXTITEM, LVM_INSERTCOLUMNW, LVM_INSERTITEMW, LVM_SETCOLUMNWIDTH,
    LVM_SETEXTENDEDLISTVIEWSTYLE, LVM_SETITEMSTATE, LVM_SETITEMTEXTW, LVNI_SELECTED, LVS_EX_DOUBLEBUFFER,
    LVS_EX_FULLROWSELECT, LVS_REPORT, LVS_SHOWSELALWAYS, NM_CUSTOMDRAW, NM_DBLCLK, NMHDR, NMLVCUSTOMDRAW, PBM_SETPOS,
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
const UPDATES: i32 = 33;

const CHECKS: [(i32, &str); 5] = [
    (SKIP_TRASH, "skip.trash"),
    (SKIP_WIPES, "skip.wipes"),
    (SKIP_FAILED, "skip.failed"),
    (SKIP_SHORT, "skip.short"),
    (MERGE, "merge"),
];

const LOG_COLUMNS: &[(&str, i32)] = &[("Log", 300), ("Size", 90), ("Modified", 110)];
const FIGHT_COLUMNS: &[(&str, i32)] = &[("Fight", 250), ("Difficulty", 90), ("Result", 90), ("Duration", 70)];

const WIDTH: i32 = 560;
const HEIGHT: i32 = 560;

const LAYOUT: &[(i32, &str, &str, u32, [i32; 4])] = &[
    (0, "STATIC", "Game", 0, [16, 20, 70, 20]),
    (
        GAME,
        "COMBOBOX",
        "",
        CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
        [90, 16, 348, 320],
    ),
    (UPDATES, "BUTTON", "Updates", BS_PUSHBUTTON as u32, [446, 15, 98, 26]),
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
        "SysListView32",
        "",
        LVS_REPORT | LVS_SHOWSELALWAYS | WS_BORDER,
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
    Update(crate::update::Release),
    Updated(Result<PathBuf, String>),
    Checked(Result<(), String>),
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
    bold: HFONT,
    columns: &'static [(&'static str, i32)],
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
        let mut picked = Vec::new();
        let mut at = usize::MAX;
        while let Ok(next) = usize::try_from(send(list, LVM_GETNEXTITEM, at, LVNI_SELECTED as isize)) {
            picked.push(next);
            at = next;
        }
        picked
    }

    fn widths(&self) {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) } as i32;
        for (i, &(_, width)) in self.columns.iter().enumerate() {
            send(
                item(self.hwnd, LOGS),
                LVM_SETCOLUMNWIDTH,
                i,
                (width * dpi / 96) as isize,
            );
        }
    }

    fn table(&mut self, columns: &'static [(&'static str, i32)]) {
        let list = item(self.hwnd, LOGS);
        send(list, LVM_DELETEALLITEMS, 0, 0);
        while send(list, LVM_DELETECOLUMN, 0, 0) != 0 {}

        for (i, &(title, _)) in columns.iter().enumerate() {
            let text = wide(title);
            let mut column: LVCOLUMNW = unsafe { std::mem::zeroed() };
            column.mask = LVCF_TEXT | LVCF_WIDTH;
            column.pszText = text.as_ptr() as *mut u16;
            send(list, LVM_INSERTCOLUMNW, i, (&raw const column) as isize);
        }
        self.columns = columns;
        self.widths();
    }

    fn row(&self, cells: &[String]) {
        let list = item(self.hwnd, LOGS);
        let index = send(list, LVM_GETITEMCOUNT, 0, 0) as usize;
        for (sub, text) in cells.iter().enumerate() {
            let text = wide(text);
            let mut row: LVITEMW = unsafe { std::mem::zeroed() };
            row.mask = LVIF_TEXT;
            row.iItem = index as i32;
            row.iSubItem = sub as i32;
            row.pszText = text.as_ptr() as *mut u16;
            let msg = if sub == 0 { LVM_INSERTITEMW } else { LVM_SETITEMTEXTW };
            send(list, msg, index, (&raw const row) as isize);
        }
    }

    fn select(&self, index: usize, on: bool) {
        let mut row: LVITEMW = unsafe { std::mem::zeroed() };
        row.stateMask = LVIS_SELECTED;
        row.state = if on { LVIS_SELECTED } else { 0 };
        send(
            item(self.hwnd, LOGS),
            LVM_SETITEMSTATE,
            index,
            (&raw const row) as isize,
        );
    }

    fn notify(&mut self, l: LPARAM) -> Option<LRESULT> {
        let header = unsafe { &*(l as *const NMHDR) };
        if header.idFrom != LOGS as usize {
            return None;
        }
        match header.code {
            NM_DBLCLK => {
                self.start(false);
                None
            }
            NM_CUSTOMDRAW => {
                let draw = unsafe { &*(l as *const NMLVCUSTOMDRAW) };
                let dungeon = || {
                    let fights = self.fights.as_ref().map(|(_, f)| f);
                    fights
                        .and_then(|f| f.get(draw.nmcd.dwItemSpec))
                        .is_some_and(Fight::dungeon)
                };
                Some(match draw.nmcd.dwDrawStage {
                    CDDS_PREPAINT => CDRF_NOTIFYITEMDRAW as LRESULT,
                    CDDS_ITEMPREPAINT if dungeon() => {
                        unsafe { SelectObject(draw.nmcd.hdc, self.bold) };
                        CDRF_NEWFONT as LRESULT
                    }
                    _ => CDRF_DODEFAULT as LRESULT,
                })
            }
            _ => None,
        }
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
            metrics.lfMessageFont.lfWeight = FW_BOLD as i32;
            let bold = CreateFontIndirectW(&metrics.lfMessageFont);
            for old in [self.font, self.bold] {
                if !old.is_null() {
                    DeleteObject(old);
                }
            }
            self.font = font;
            self.bold = bold;

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
        self.widths();
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
        send(
            item(self.hwnd, LOGS),
            LVM_SETEXTENDEDLISTVIEWSTYLE,
            0,
            (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as isize,
        );
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

        self.check_updates(false);
    }

    fn check_updates(&mut self, manual: bool) {
        if manual {
            self.status("Checking for updates…");
        }
        let hwnd = self.hwnd as usize;
        std::thread::spawn(move || match crate::update::check() {
            Ok(Some(release)) => post(hwnd, Msg::Update(release)),
            Ok(None) if manual => post(hwnd, Msg::Checked(Ok(()))),
            Err(e) if manual => post(hwnd, Msg::Checked(Err(e))),
            _ => {}
        });
    }

    fn offer(&mut self, release: crate::update::Release) {
        if self.job != Job::Idle {
            return;
        }
        let text = format!(
            "Warlogger {} is available (you have {}).\n\nUpdate and restart now?",
            release.version,
            env!("CARGO_PKG_VERSION")
        );
        let answer = unsafe {
            MessageBoxW(
                self.hwnd,
                wide(&text).as_ptr(),
                wide("Warlogger update").as_ptr(),
                MB_YESNO | MB_ICONINFORMATION,
            )
        };
        if answer != IDYES {
            return;
        }

        let hwnd = self.hwnd as usize;
        self.set_job(Job::Busy);
        self.status(&format!("Downloading Warlogger {}…", release.version));
        std::thread::spawn(move || post(hwnd, Msg::Updated(crate::update::install(&release))));
    }

    fn updated(&mut self, result: Result<PathBuf, String>) {
        match result.and_then(|exe| std::process::Command::new(exe).spawn().map_err(|e| e.to_string())) {
            Ok(_) => unsafe {
                DestroyWindow(self.hwnd);
            },
            Err(e) => {
                self.status(&format!("Update failed: {e}"));
                self.set_job(Job::Idle);
            }
        }
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
        self.table(LOG_COLUMNS);
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
            self.row(&[name, format!("{:.1} MB", log.size as f64 / 1e6), ago(&log)]);
            self.logs.push(log.path);
        }
        if !self.logs.is_empty() {
            self.select(0, true);
        }
    }

    fn select_fights(&self) {
        let Some((_, fights)) = &self.fights else { return };
        let filter = self.filter();
        for (i, fight) in fights.iter().enumerate() {
            self.select(i, filter.keep(self.game(), fight));
        }
    }

    fn show_fights(&mut self, path: PathBuf, fights: Vec<Fight>) {
        self.table(FIGHT_COLUMNS);
        for fight in &fights {
            self.row(&fight.columns(self.game()));
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
        let credentials = self.credentials();
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
            notify(Event::Step("scanning for fights".into()));
            let result = crate::upload::fights(game, &path, &CANCEL, &mut notify).and_then(|offline| match offline {
                Some(fights) => Ok(fights),
                None => {
                    let (email, password) = credentials.ok_or("Sign in first")?;
                    let mut session = Session::open(game, &email, &password, &mut notify)?;
                    let target = crate::target(&cfg, &Args::default(), &session.account)?;
                    session.scan(&path, &target.region, &CANCEL, &mut notify)
                }
            });
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
            UPDATES,
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
                let picked = self.picked().into_iter().filter_map(|i| fights.get(i).cloned());
                filter.fights = Some(picked.collect());
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
            Msg::Update(release) => self.offer(release),
            Msg::Updated(result) => self.updated(result),
            Msg::Checked(Ok(())) => self.status(concat!("Warlogger ", env!("CARGO_PKG_VERSION"), " is up to date")),
            Msg::Checked(Err(e)) => self.status(&format!("Update check failed: {e}")),
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
            (UPDATES, BN_CLICKED) => self.check_updates(true),
            (FIGHTS, BN_CLICKED) => self.scan(),
            (SPLIT, BN_CLICKED) => self.split(),
            (SKIP_TRASH | SKIP_WIPES | SKIP_FAILED | SKIP_SHORT | MERGE, BN_CLICKED) => self.check(id),
            (UPLOAD, BN_CLICKED) => self.start(false),
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
        WM_NOTIFY => {
            return app
                .notify(l)
                .unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, msg, w, l) });
        }
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
            dwICC: ICC_PROGRESS_CLASS | ICC_STANDARD_CLASSES | ICC_LISTVIEW_CLASSES,
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
            bold: null_mut(),
            columns: LOG_COLUMNS,
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
