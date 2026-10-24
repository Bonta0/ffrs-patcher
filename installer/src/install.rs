//! Installing / removing the mod files in the FINAL FANTASY RESONANCE folder.
//!
//!   FFRS\Binaries\Win64\dsound.dll            analog movement + Octopath sounds
//!   FFRS\Binaries\Win64\FFRSPatcher.ini      their settings
//!   FFRS\Content\Paks\~mods\zFFRS_Patcher_P.pak/.ucas/.utoc   the UI theme, generated
//!                                             from the game's own files (theme.rs)

use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

const DLL: &[u8] = include_bytes!("../../native/target/release/dsound.dll");
const THEME_NAME: &str = "zFFRS_Patcher_P";
const THEME_FILES: [&str; 3] = ["pak", "ucas", "utoc"];
/// Text inside our dsound.dll, to tell it apart from another mod's proxy DLL.
const DLL_MARKER: &[u8] = b"FFRS Patcher";
const GAME_EXE: &str = "FFRS-Win64-Shipping.exe";

#[derive(Clone)]
pub struct Options {
    pub analog: bool,
    pub theme: bool,
    pub sounds: bool,
    pub octopath: Option<PathBuf>,
}

struct Paths {
    dll: PathBuf,
    ini: PathBuf,
    log: PathBuf,
    mods: PathBuf,
}

fn paths(ffrs: &Path) -> Paths {
    let bin = ffrs.join(r"FFRS\Binaries\Win64");
    Paths {
        dll: bin.join("dsound.dll"),
        ini: bin.join("FFRSPatcher.ini"),
        log: bin.join("FFRSPatcher.log"),
        mods: ffrs.join(r"FFRS\Content\Paks\~mods"),
    }
}

pub fn game_running() -> bool {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: PROCESSENTRY32W = core::mem::zeroed();
        entry.dwSize = core::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        let mut ok = Process32FirstW(snap, &mut entry) != 0;
        while ok && !found {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            found = String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(GAME_EXE);
            ok = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
        found
    }
}

fn ours(dll: &Path) -> bool {
    std::fs::read(dll).is_ok_and(|b| b.windows(DLL_MARKER.len()).any(|w| w == DLL_MARKER))
}

/// `key=value` lines of an existing FFRSPatcher.ini.
fn ini_value(ini: &str, key: &str) -> Option<String> {
    ini.lines().find_map(|line| {
        let line = line.split(';').next()?.trim();
        let (k, v) = line.split_once('=')?;
        k.trim().eq_ignore_ascii_case(key).then(|| v.trim().to_string())
    })
}

/// What is currently installed, to pre-fill the checkboxes.
pub fn installed(ffrs: &Path) -> Option<Options> {
    let p = paths(ffrs);
    let theme = theme_files(&p).iter().all(|f| f.is_file());
    let ini = std::fs::read_to_string(&p.ini).ok().filter(|_| ours(&p.dll));
    if ini.is_none() && !theme {
        return None;
    }
    let ini = ini.unwrap_or_default();
    let flag = |key| ini_value(&ini, key).is_some_and(|v| v == "1");
    Some(Options {
        analog: flag("AnalogMovement"),
        theme,
        sounds: flag("OctopathSounds"),
        octopath: ini_value(&ini, "OctopathPath").filter(|v| !v.is_empty()).map(PathBuf::from),
    })
}

fn ini_text(o: &Options, previous: &str) -> String {
    let flag = |b: bool| if b { "1" } else { "0" };
    // settings people tune by hand survive a reinstall
    let volume = ini_value(previous, "SoundVolume").unwrap_or_else(|| "1.0".into());
    let trace = ini_value(previous, "LogSounds").unwrap_or_else(|| "0".into());
    let octopath = o.octopath.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    format!(
        "; FFRS Patcher settings - written by FFRS-Patcher.exe, safe to edit (1 = on, 0 = off)\r\n\
         [FFRSPatcher]\r\n\
         ; Full 360-degree analog stick movement in the field\r\n\
         AnalogMovement={}\r\n\
         ; OCTOPATH TRAVELER II menu sounds, read from your own copy each time the game starts\r\n\
         OctopathSounds={}\r\n\
         OctopathPath={octopath}\r\n\
         ; Loudness of those sounds relative to the game's sound effect volume (0.0 - 4.0)\r\n\
         SoundVolume={volume}\r\n\
         ; Write every sound the game plays to FFRSPatcher.log (troubleshooting)\r\n\
         LogSounds={trace}\r\n",
        flag(o.analog),
        flag(o.sounds),
    )
}

fn remove(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("Could not remove {}: {e}", path.display())),
        _ => Ok(()),
    }
}

fn theme_files(p: &Paths) -> Vec<PathBuf> {
    THEME_FILES.iter().map(|ext| p.mods.join(format!("{THEME_NAME}.{ext}"))).collect()
}

/// Build the theme container from the game's own UI files (theme.rs), plus the theme's
/// fonts (fonts.rs).
fn build_theme(ffrs: &Path, p: &Paths) -> anyhow::Result<()> {
    let files = crate::theme::build(&ffrs.join(r"FFRS\Content\Paks"))?;
    std::fs::create_dir_all(&p.mods)?;
    crate::theme::pack(&files, &p.mods.join(format!("{THEME_NAME}.utoc")))?;
    crate::fonts::write_pak(&p.mods.join(format!("{THEME_NAME}.pak")))
}

fn write(path: &Path, data: &[u8]) -> Result<(), String> {
    std::fs::write(path, data).map_err(|e| format!("Could not write {}: {e}", path.display()))
}

pub fn install(ffrs: &Path, o: &Options) -> Result<String, String> {
    if game_running() {
        return Err("Close FINAL FANTASY RESONANCE first, then try again.".into());
    }
    let p = paths(ffrs);
    let native = o.analog || o.sounds;
    let foreign_dll = p.dll.is_file() && !ours(&p.dll);
    if native && foreign_dll {
        return Err(format!(
            "{} belongs to another mod. Remove it first: analog movement and the sounds need that file name.",
            p.dll.display()
        ));
    }
    // the theme first: if this game version is not supported, nothing else is touched
    for f in theme_files(&p) {
        remove(&f)?;
    }
    if o.theme {
        if let Err(e) = build_theme(ffrs, &p) {
            for f in theme_files(&p) {
                let _ = std::fs::remove_file(f);
            }
            return Err(format!("Could not build the Octopath-style UI from your game files:
{e:#}"));
        }
    }
    let previous = std::fs::read_to_string(&p.ini).unwrap_or_default();
    if native {
        write(&p.dll, DLL)?;
        write(&p.ini, ini_text(o, &previous).as_bytes())?;
    } else {
        if !foreign_dll {
            remove(&p.dll)?;
        }
        for f in [&p.ini, &p.log] {
            remove(f)?;
        }
    }
    let mut done = Vec::new();
    if o.analog {
        done.push("analog movement");
    }
    if o.theme {
        done.push("Octopath-style UI");
    }
    if o.sounds {
        done.push("Octopath II menu sounds");
    }
    Ok(if done.is_empty() {
        "Nothing selected: the mod is removed.".into()
    } else {
        format!("Installed: {}. Start the game to play with it.", done.join(", "))
    })
}

pub fn uninstall(ffrs: &Path) -> Result<String, String> {
    if game_running() {
        return Err("Close FINAL FANTASY RESONANCE first, then try again.".into());
    }
    let p = paths(ffrs);
    if ours(&p.dll) {
        remove(&p.dll)?;
    }
    for f in [&p.ini, &p.log] {
        remove(f)?;
    }
    for f in theme_files(&p) {
        remove(&f)?;
    }
    Ok("Removed. The game is back to normal.".into())
}
