use super::wide;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegGetValueW, RegSetValueExW,
};
use windows_sys::Win32::UI::Controls::{
    TASKDIALOG_BUTTON, TASKDIALOGCONFIG, TDCBF_CANCEL_BUTTON, TDE_CONTENT, TDF_ALLOW_DIALOG_CANCELLATION,
    TDF_USE_HICON_MAIN, TDM_SET_ELEMENT_TEXT, TDN_BUTTON_CLICKED,
};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Shell::{
    BFFM_INITIALIZED, BFFM_SETSELECTIONW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW, SHBrowseForFolderW,
    SHGetPathFromIDListW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IDYES, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, MB_ICONINFORMATION, MB_ICONQUESTION, MB_YESNO, MessageBoxW,
    SendMessageW,
};
use windows_sys::core::{BOOL, HRESULT};

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Warlogger";
const FOLDER: &str = "Warlogger";
const EXE: &str = "warlogger.exe";
const INSTALL: i32 = 100;
const BROWSE: i32 = 101;
const S_FALSE: HRESULT = 1;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

type TaskDialogIndirect = unsafe extern "system" fn(*const TASKDIALOGCONFIG, *mut i32, *mut i32, *mut BOOL) -> HRESULT;

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

impl Key {
    fn create(path: &str) -> Result<Self, String> {
        let mut key = null_mut();
        let path = wide(path);
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                null(),
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_SET_VALUE,
                null(),
                &mut key,
                null_mut(),
            )
        };
        match status {
            0 => Ok(Self(key)),
            e => Err(format!("registry: {}", std::io::Error::from_raw_os_error(e as i32))),
        }
    }

    fn text(&self, name: &str, value: &str) -> Result<(), String> {
        let data: Vec<u8> = wide(value).iter().flat_map(|u| u.to_le_bytes()).collect();
        self.set(name, REG_SZ, &data)
    }

    fn number(&self, name: &str, value: u32) -> Result<(), String> {
        self.set(name, REG_DWORD, &value.to_le_bytes())
    }

    fn set(&self, name: &str, kind: u32, data: &[u8]) -> Result<(), String> {
        let name = wide(name);
        match unsafe { RegSetValueExW(self.0, name.as_ptr(), 0, kind, data.as_ptr(), data.len() as u32) } {
            0 => Ok(()),
            e => Err(format!("registry: {}", std::io::Error::from_raw_os_error(e as i32))),
        }
    }
}

fn read(path: &str, name: &str) -> Option<String> {
    let (path, name) = (wide(path), wide(name));
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
    (status == 0 && len > 0).then(|| String::from_utf16_lossy(&buf[..len]))
}

fn env_dir(var: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

fn install_dir() -> PathBuf {
    read(UNINSTALL, "InstallLocation")
        .map(PathBuf::from)
        .unwrap_or_else(|| env_dir("LOCALAPPDATA").join("Programs").join(FOLDER))
}

fn shortcut() -> PathBuf {
    env_dir("APPDATA").join(r"Microsoft\Windows\Start Menu\Programs\Warlogger.lnk")
}

fn dedicated(dir: &Path) -> PathBuf {
    match dir.file_name().is_some_and(|n| n.eq_ignore_ascii_case(FOLDER)) {
        true => dir.to_path_buf(),
        false => dir.join(FOLDER),
    }
}

fn hidden(program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("{program}: {e}"))?;
    match status.success() {
        true => Ok(()),
        false => Err(format!("{program} failed")),
    }
}

fn message(text: &str, flags: u32) -> i32 {
    unsafe { MessageBoxW(null_mut(), wide(text).as_ptr(), wide("Warlogger").as_ptr(), flags) }
}

fn register(exe: &Path) -> Result<(), String> {
    let exe_text = exe.to_string_lossy();
    let uninstall = format!("\"{exe_text}\" uninstall");
    let size = std::fs::metadata(exe).map_or(0, |m| (m.len() / 1024) as u32);

    let key = Key::create(UNINSTALL)?;
    key.text("DisplayName", "Warlogger")?;
    key.text("DisplayVersion", env!("CARGO_PKG_VERSION"))?;
    key.text("Publisher", "patvuk")?;
    key.text("URLInfoAbout", "https://github.com/patvuk/warlogger")?;
    key.text("DisplayIcon", &exe_text)?;
    key.text("InstallLocation", &exe.parent().unwrap_or(exe).to_string_lossy())?;
    key.text("UninstallString", &uninstall)?;
    key.text("QuietUninstallString", &format!("{uninstall} --quiet"))?;
    key.number("EstimatedSize", size)?;
    key.number("NoModify", 1)?;
    key.number("NoRepair", 1)
}

fn link(exe: &Path) -> Result<(), String> {
    let quote = |p: &Path| p.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s.TargetPath = '{}'; $s.WorkingDirectory = '{}'; $s.Save()",
        quote(&shortcut()),
        quote(exe),
        quote(exe.parent().unwrap_or(exe))
    );
    hidden("powershell", &["-NoProfile", "-NonInteractive", "-Command", &script])
}

fn task_dialog() -> Option<TaskDialogIndirect> {
    let lib = unsafe { LoadLibraryW(wide("comctl32.dll").as_ptr()) };
    if lib.is_null() {
        return None;
    }
    let function = unsafe { GetProcAddress(lib, c"TaskDialogIndirect".as_ptr().cast()) }?;
    Some(unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, TaskDialogIndirect>(function) })
}

unsafe extern "system" fn browse_callback(hwnd: HWND, msg: u32, _: LPARAM, start: LPARAM) -> i32 {
    if msg == BFFM_INITIALIZED {
        unsafe { SendMessageW(hwnd, BFFM_SETSELECTIONW, 1, start) };
    }
    0
}

fn pick_folder(owner: HWND, current: &Path) -> Option<PathBuf> {
    unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) };

    let start = wide(&current.parent().unwrap_or(current).to_string_lossy());
    let title = wide("Select the install location. Warlogger goes into a Warlogger folder in it.");
    let mut name = [0u16; 260];
    let info = BROWSEINFOW {
        hwndOwner: owner,
        pidlRoot: null_mut(),
        pszDisplayName: name.as_mut_ptr(),
        lpszTitle: title.as_ptr(),
        ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
        lpfn: Some(browse_callback),
        lParam: start.as_ptr() as LPARAM,
        iImage: 0,
    };

    let pidl = unsafe { SHBrowseForFolderW(&info) };
    if pidl.is_null() {
        return None;
    }
    let mut path = [0u16; 260];
    let ok = unsafe { SHGetPathFromIDListW(pidl, path.as_mut_ptr()) } != 0;
    unsafe { CoTaskMemFree(pidl.cast()) };

    let len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
    ok.then(|| PathBuf::from(String::from_utf16_lossy(&path[..len])))
}

struct State {
    dir: PathBuf,
    content: Vec<u16>,
}

fn content(dir: &Path) -> Vec<u16> {
    wide(&format!(
        "Install location:\n{}\n\nWarlogger adds a Start menu shortcut and an uninstaller in Installed apps.",
        dir.display()
    ))
}

unsafe extern "system" fn dialog_callback(hwnd: HWND, msg: u32, wparam: WPARAM, _: LPARAM, data: isize) -> HRESULT {
    let state = unsafe { &mut *(data as *mut State) };
    if msg as i32 == TDN_BUTTON_CLICKED && wparam as i32 == BROWSE {
        if let Some(dir) = pick_folder(hwnd, &state.dir) {
            state.dir = dedicated(&dir);
            state.content = content(&state.dir);
            unsafe {
                SendMessageW(
                    hwnd,
                    TDM_SET_ELEMENT_TEXT as u32,
                    TDE_CONTENT as WPARAM,
                    state.content.as_ptr() as LPARAM,
                )
            };
        }
        return S_FALSE;
    }
    0
}

fn ask(dir: &Path) -> Option<PathBuf> {
    let Some(show) = task_dialog() else {
        return Some(dir.to_path_buf());
    };

    let mut state = State {
        dir: dir.to_path_buf(),
        content: content(dir),
    };
    let title = wide("Warlogger Setup");
    let heading = wide(concat!("Install Warlogger ", env!("CARGO_PKG_VERSION")));
    let (install, browse) = (wide("Install"), wide("Change location..."));
    let buttons = [
        TASKDIALOG_BUTTON {
            nButtonID: INSTALL,
            pszButtonText: install.as_ptr(),
        },
        TASKDIALOG_BUTTON {
            nButtonID: BROWSE,
            pszButtonText: browse.as_ptr(),
        },
    ];

    let mut config: TASKDIALOGCONFIG = unsafe { std::mem::zeroed() };
    config.cbSize = size_of::<TASKDIALOGCONFIG>() as u32;
    config.hInstance = unsafe { GetModuleHandleW(null()) };
    config.dwFlags = TDF_ALLOW_DIALOG_CANCELLATION | TDF_USE_HICON_MAIN;
    config.dwCommonButtons = TDCBF_CANCEL_BUTTON;
    config.pszWindowTitle = title.as_ptr();
    let size = (32 * unsafe { GetDpiForSystem() } / 96) as i32;
    config.Anonymous1.hMainIcon =
        unsafe { LoadImageW(config.hInstance, 1 as _, IMAGE_ICON, size, size, LR_DEFAULTCOLOR) };
    config.pszMainInstruction = heading.as_ptr();
    config.pszContent = state.content.as_ptr();
    config.cButtons = buttons.len() as u32;
    config.pButtons = buttons.as_ptr();
    config.nDefaultButton = INSTALL;
    config.pfCallback = Some(dialog_callback);
    config.lpCallbackData = (&raw mut state) as isize;

    let mut pressed = 0;
    let shown = unsafe { show(&config, &mut pressed, null_mut(), null_mut()) };
    (shown == 0 && pressed == INSTALL).then_some(state.dir)
}

fn install(dir: &Path) -> Result<PathBuf, String> {
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = dir.join(EXE);
    let fail = |e: std::io::Error| format!("{}: {e}", exe.display());

    std::fs::create_dir_all(dir).map_err(fail)?;
    if exe.exists() {
        let old = exe.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old).map_err(fail)?;
    }
    std::fs::copy(&current, &exe).map_err(fail)?;

    register(&exe)?;
    link(&exe)?;
    Ok(exe)
}

fn sharp() {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

pub fn adopt() -> bool {
    let Ok(current) = std::env::current_exe() else {
        return false;
    };
    if current.starts_with(install_dir()) {
        let _ = register(&current);
        return false;
    }

    sharp();
    let Some(dir) = ask(&install_dir()) else {
        return true;
    };
    match install(&dir).and_then(|exe| Command::new(exe).spawn().map_err(|e| e.to_string())) {
        Ok(_) => {}
        Err(e) => {
            message(
                &format!("Warlogger is not installed: {e}\n\nThe portable version runs without installation."),
                MB_ICONINFORMATION,
            );
        }
    }
    true
}

pub fn uninstall(quiet: bool) -> Result<(), String> {
    sharp();
    if !quiet
        && message(
            "Remove Warlogger, its settings and its saved passwords?",
            MB_YESNO | MB_ICONQUESTION,
        ) != IDYES
    {
        return Ok(());
    }

    let dir = install_dir();
    let _ = std::fs::remove_file(shortcut());
    unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, wide(UNINSTALL).as_ptr()) };
    let _ = std::fs::remove_dir_all(crate::store::dir());
    super::cred::forget_all();

    if dir.file_name().is_some_and(|n| n.eq_ignore_ascii_case(FOLDER)) {
        let script = format!("/C ping -n 3 127.0.0.1 >NUL & rmdir /S /Q \"{}\"", dir.display());
        Command::new("cmd")
            .raw_arg(script)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    if !quiet {
        message("Warlogger is removed.", MB_ICONINFORMATION);
    }
    Ok(())
}
