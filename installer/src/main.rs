//! FFRS Patcher: one window to install or remove the mod.
//!
//! The mod files are inside this exe. The user confirms (or browses to) the
//! FINAL FANTASY RESONANCE folder and, for the menu sounds, their OCTOPATH TRAVELER II
//! folder, picks the features and presses Install.

#![windows_subsystem = "windows"]

mod art;
mod detect;
mod edits;
mod fonts;
mod install;
mod package;
mod props;
mod schema;
mod theme;

use std::cell::RefCell;
use std::path::PathBuf;

use windows::core::PCWSTR;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontIndirectW, GetSysColorBrush, SetBkMode, UpdateWindow, COLOR_WINDOW, HBRUSH, HDC, HFONT, TRANSPARENT,
};
use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{InitCommonControlsEx, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX};
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const TITLE: &str = "FFRS Patcher";

// control ids
const ID_FFRS: i32 = 101;
const ID_FFRS_BROWSE: i32 = 102;
const ID_OCTO: i32 = 103;
const ID_OCTO_BROWSE: i32 = 104;
const ID_ANALOG: i32 = 110;
const ID_THEME: i32 = 111;
const ID_SOUNDS: i32 = 112;
const ID_INSTALL: i32 = 120;
const ID_UNINSTALL: i32 = 121;
const ID_CLOSE: i32 = 122;

/// Posted by the worker thread when an install/uninstall finishes; lParam: Box<Result<String, String>>.
const WM_DONE: u32 = WM_APP + 1;

const BN_CLICKED: u32 = 0;
const EN_CHANGE: u32 = 0x0300;
const BS_AUTOCHECKBOX: u32 = 3;
const BS_DEFPUSHBUTTON: u32 = 1;
const ES_AUTOHSCROLL: u32 = 0x80;
const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;

#[derive(Default)]
struct Ui {
    window: HWND,
    ffrs: HWND,
    octo: HWND,
    analog: HWND,
    theme: HWND,
    sounds: HWND,
    install: HWND,
    uninstall: HWND,
    status: HWND,
    font: HFONT,
}

thread_local! {
    static UI: RefCell<Ui> = RefCell::new(Ui::default());
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn scale(v: i32) -> i32 {
    v * unsafe { GetDpiForSystem() } as i32 / 96
}

fn text_of(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

fn set_text(hwnd: HWND, s: &str) {
    unsafe { SetWindowTextW(hwnd, wide(s).as_ptr()) };
}

fn checked(hwnd: HWND) -> bool {
    unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) == 1 }
}

fn set_checked(hwnd: HWND, on: bool) {
    unsafe { SendMessageW(hwnd, BM_SETCHECK, on as usize, 0) };
}

fn enable(hwnd: HWND, on: bool) {
    unsafe { EnableWindow(hwnd, on as i32) };
}

fn path_in(hwnd: HWND) -> Option<PathBuf> {
    let t = text_of(hwnd);
    let t = t.trim().trim_matches('"');
    (!t.is_empty()).then(|| PathBuf::from(t))
}

/// Folder picker; None when cancelled.
fn browse(owner: HWND, title: &str) -> Option<PathBuf> {
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        dialog.SetOptions(dialog.GetOptions().ok()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
        let title = wide(title);
        dialog.SetTitle(PCWSTR(title.as_ptr())).ok()?;
        dialog.Show(windows::Win32::Foundation::HWND(owner)).ok()?;
        let name = dialog.GetResult().ok()?.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as _));
        path.map(PathBuf::from)
    }
}

/// Re-check both folders and enable what can be used.
fn refresh() {
    UI.with(|ui| {
        let ui = ui.borrow();
        let ffrs = path_in(ui.ffrs).and_then(|p| detect::ffrs_root(&p));
        let octo = path_in(ui.octo).and_then(|p| detect::octopath_root(&p));
        enable(ui.install, ffrs.is_some());
        enable(ui.uninstall, ffrs.is_some());
        enable(ui.sounds, octo.is_some());
        if octo.is_none() {
            set_checked(ui.sounds, false);
        }
        let status = match (&ffrs, &octo) {
            (None, _) => "Pick your FINAL FANTASY RESONANCE folder (Steam: right-click the game > Manage > Browse local files).",
            (Some(_), None) if path_in(ui.octo).is_some() => "That is not an OCTOPATH TRAVELER II folder: its menu sounds stay off.",
            (Some(_), None) => "OCTOPATH TRAVELER II not found: its menu sounds need it installed. Everything else works without it.",
            (Some(_), Some(_)) => "Ready. Choose the features and press Install.",
        };
        set_text(ui.status, status);
    });
}

fn on_command(id: i32, code: u32) {
    let (window, ffrs_edit, octo_edit) = UI.with(|ui| {
        let ui = ui.borrow();
        (ui.window, ui.ffrs, ui.octo)
    });
    match (id, code) {
        (ID_FFRS | ID_OCTO, EN_CHANGE) => refresh(),
        (ID_FFRS_BROWSE, BN_CLICKED) => {
            if let Some(p) = browse(window, "FINAL FANTASY RESONANCE folder") {
                let root = detect::ffrs_root(&p).unwrap_or(p);
                set_text(ffrs_edit, &root.display().to_string());
                prefill(&root);
            }
        }
        (ID_OCTO_BROWSE, BN_CLICKED) => {
            if let Some(p) = browse(window, "OCTOPATH TRAVELER II folder") {
                let root = detect::octopath_root(&p).unwrap_or(p);
                set_text(octo_edit, &root.display().to_string());
                UI.with(|ui| {
                    let ui = ui.borrow();
                    if detect::octopath_root(&root).is_some() {
                        set_checked(ui.sounds, true);
                    }
                });
            }
        }
        (ID_INSTALL | ID_UNINSTALL, BN_CLICKED) => {
            let job = UI.with(|ui| {
                let ui = ui.borrow();
                let ffrs = path_in(ui.ffrs).and_then(|p| detect::ffrs_root(&p))?;
                let options = install::Options {
                    analog: checked(ui.analog),
                    theme: checked(ui.theme),
                    sounds: checked(ui.sounds),
                    octopath: path_in(ui.octo).and_then(|p| detect::octopath_root(&p)),
                };
                enable(ui.install, false);
                enable(ui.uninstall, false);
                let busy = if id == ID_INSTALL && options.theme { "Installing... (building the UI theme from your game files)" } else { "Working..." };
                set_text(ui.status, busy);
                Some((ffrs, options))
            });
            if let Some((ffrs, options)) = job {
                // the theme takes a few seconds to build: keep the window responsive
                let window = window as isize;
                std::thread::spawn(move || {
                    let result = if id == ID_INSTALL { install::install(&ffrs, &options) } else { install::uninstall(&ffrs) };
                    unsafe { PostMessageW(window as HWND, WM_DONE, 0, Box::into_raw(Box::new(result)) as LPARAM) };
                });
            }
        }
        (ID_CLOSE, BN_CLICKED) => unsafe {
            DestroyWindow(window);
        },
        _ => {}
    }
}

/// Tick the boxes to match an existing installation (all on for a new one).
fn prefill(ffrs: &std::path::Path) {
    UI.with(|ui| {
        let ui = ui.borrow();
        match install::installed(ffrs) {
            Some(o) => {
                set_checked(ui.analog, o.analog);
                set_checked(ui.theme, o.theme);
                set_checked(ui.sounds, o.sounds);
                if path_in(ui.octo).is_none() {
                    if let Some(p) = o.octopath {
                        set_text(ui.octo, &p.display().to_string());
                    }
                }
            }
            None => {
                set_checked(ui.analog, true);
                set_checked(ui.theme, true);
                set_checked(ui.sounds, true);
            }
        }
    });
    refresh();
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            on_command((wparam & 0xFFFF) as i32, ((wparam >> 16) & 0xFFFF) as u32);
            0
        }
        WM_DONE => {
            let result = Box::from_raw(lparam as *mut Result<String, String>);
            let (text, icon) = match *result {
                Ok(t) => (t, MB_ICONINFORMATION),
                Err(e) => (e, MB_ICONWARNING),
            };
            refresh();
            UI.with(|ui| set_text(ui.borrow().status, &text));
            MessageBoxW(hwnd, wide(&text).as_ptr(), wide(TITLE).as_ptr(), MB_OK | icon);
            0
        }
        WM_CTLCOLORSTATIC => {
            SetBkMode(wparam as HDC, TRANSPARENT as i32);
            GetSysColorBrush(COLOR_WINDOW) as LRESULT
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn control(class: &str, text: &str, style: u32, x: i32, y: i32, w: i32, h: i32, id: i32) -> HWND {
    let (parent, font) = UI.with(|ui| (ui.borrow().window, ui.borrow().font));
    let hwnd = CreateWindowExW(
        if class == "EDIT" { WS_EX_CLIENTEDGE } else { 0 },
        wide(class).as_ptr(),
        wide(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        scale(x),
        scale(y),
        scale(w),
        scale(h),
        parent,
        id as isize as _,
        GetModuleHandleW(core::ptr::null()),
        core::ptr::null(),
    );
    SendMessageW(hwnd, WM_SETFONT, font as usize, 1);
    hwnd
}

fn main() {
    unsafe {
        CoInitializeEx(core::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let icc = INITCOMMONCONTROLSEX { dwSize: core::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_STANDARD_CLASSES };
        InitCommonControlsEx(&icc);

        let instance = GetModuleHandleW(core::ptr::null());
        let class = wide("FFRSPatcher");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            hCursor: LoadCursorW(core::ptr::null_mut(), IDC_ARROW),
            hIcon: LoadIconW(core::ptr::null_mut(), IDI_APPLICATION),
            hbrBackground: GetSysColorBrush(COLOR_WINDOW) as HBRUSH,
            lpszClassName: class.as_ptr(),
            ..core::mem::zeroed()
        };
        RegisterClassW(&wc);

        // client area 600 x 330 (96 dpi units)
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut rect = windows_sys::Win32::Foundation::RECT { left: 0, top: 0, right: scale(600), bottom: scale(330) };
        AdjustWindowRect(&mut rect, style, 0);
        let window = CreateWindowExW(
            0,
            class.as_ptr(),
            wide(TITLE).as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            instance,
            core::ptr::null(),
        );

        let mut metrics: NONCLIENTMETRICSW = core::mem::zeroed();
        metrics.cbSize = core::mem::size_of::<NONCLIENTMETRICSW>() as u32;
        SystemParametersInfoW(SPI_GETNONCLIENTMETRICS, metrics.cbSize, (&mut metrics as *mut NONCLIENTMETRICSW).cast(), 0);
        let font = CreateFontIndirectW(&metrics.lfMessageFont);
        UI.with(|ui| {
            let mut ui = ui.borrow_mut();
            ui.window = window;
            ui.font = font;
        });

        let tab = WS_TABSTOP;
        control("STATIC", "FINAL FANTASY RESONANCE folder", 0, 16, 14, 568, 18, 0);
        let ffrs = control("EDIT", "", tab | ES_AUTOHSCROLL, 16, 34, 470, 24, ID_FFRS);
        control("BUTTON", "Browse...", tab, 494, 33, 90, 26, ID_FFRS_BROWSE);
        control("STATIC", "OCTOPATH TRAVELER II folder (only for its menu sounds)", 0, 16, 70, 568, 18, 0);
        let octo = control("EDIT", "", tab | ES_AUTOHSCROLL, 16, 90, 470, 24, ID_OCTO);
        control("BUTTON", "Browse...", tab, 494, 89, 90, 26, ID_OCTO_BROWSE);
        let analog = control("BUTTON", "Full analog movement (walk in any direction, not just 8)", tab | BS_AUTOCHECKBOX, 16, 132, 568, 22, ID_ANALOG);
        let theme = control("BUTTON", "Octopath-style battle screen, menus and fonts", tab | BS_AUTOCHECKBOX, 16, 158, 568, 22, ID_THEME);
        let sounds = control(
            "BUTTON",
            "OCTOPATH TRAVELER II menu sounds (read from your copy, nothing is copied)",
            tab | BS_AUTOCHECKBOX,
            16,
            184,
            568,
            22,
            ID_SOUNDS,
        );
        let status = control("STATIC", "", 0, 16, 222, 568, 40, 0);
        let install = control("BUTTON", "Install", tab | BS_DEFPUSHBUTTON, 298, 284, 90, 30, ID_INSTALL);
        let uninstall = control("BUTTON", "Uninstall", tab, 396, 284, 90, 30, ID_UNINSTALL);
        control("BUTTON", "Close", tab, 494, 284, 90, 30, ID_CLOSE);
        UI.with(|ui| {
            let mut ui = ui.borrow_mut();
            (ui.ffrs, ui.octo, ui.analog, ui.theme, ui.sounds, ui.install, ui.uninstall, ui.status) =
                (ffrs, octo, analog, theme, sounds, install, uninstall, status);
        });

        // fill in what can be found; the boxes follow an existing installation
        if let Some(p) = detect::find_octopath() {
            set_text(octo, &p.display().to_string());
        }
        match detect::find_ffrs() {
            Some(p) => {
                set_text(ffrs, &p.display().to_string());
                prefill(&p);
            }
            None => {
                set_checked(analog, true);
                set_checked(theme, true);
                set_checked(sounds, true);
                refresh();
            }
        }

        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
        let mut msg: MSG = core::mem::zeroed();
        while GetMessageW(&mut msg, core::ptr::null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(window, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
