//! Finding the two games: in Steam's libraries, or from a folder the user picked
//! (any folder inside the game folder is accepted).

use std::path::{Path, PathBuf};

use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

const FFRS_EXE: &str = r"FFRS\Binaries\Win64\FFRS-Win64-Shipping.exe";
const OCTOPATH_PAK: &str = r"Octopath_Traveler2\Content\Paks\Octopath_Traveler2-WindowsNoEditor.pak";

/// The FINAL FANTASY RESONANCE folder at or above `path`.
pub fn ffrs_root(path: &Path) -> Option<PathBuf> {
    path.ancestors().find(|p| p.join(FFRS_EXE).is_file()).map(on_disk)
}

/// The OCTOPATH TRAVELER II folder at or above `path`.
pub fn octopath_root(path: &Path) -> Option<PathBuf> {
    path.ancestors().find(|p| p.join(OCTOPATH_PAK).is_file()).map(on_disk)
}

/// `path` spelled as on disk (Steam reports its own path in lower case).
fn on_disk(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(p) => PathBuf::from(p.to_string_lossy().trim_start_matches(r"\\?\").to_string()),
        Err(_) => path.to_path_buf(),
    }
}

pub fn find_ffrs() -> Option<PathBuf> {
    steam_common().iter().find_map(|c| ffrs_root(&c.join("FINAL FANTASY RESONANCE")))
}

pub fn find_octopath() -> Option<PathBuf> {
    steam_common().iter().find_map(|c| octopath_root(&c.join("Octopath_Traveler2")))
}

/// `steamapps\common` of every Steam library.
fn steam_common() -> Vec<PathBuf> {
    let Some(steam) = steam_path() else { return Vec::new() };
    let mut libraries = vec![steam.clone()];
    let vdf = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")).unwrap_or_default();
    for line in vdf.lines() {
        let mut fields = line.split('"').skip(1).step_by(2);
        if let (Some("path"), Some(path)) = (fields.next(), fields.next()) {
            libraries.push(PathBuf::from(path.replace("\\\\", "\\")));
        }
    }
    libraries.into_iter().map(|l| l.join("steamapps").join("common")).collect()
}

fn steam_path() -> Option<PathBuf> {
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (key, value) = (wide("Software\\Valve\\Steam"), wide("SteamPath"));
    let mut buf = [0u16; 512];
    let mut size = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, core::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
    };
    let path = String::from_utf16_lossy(&buf[..(size as usize / 2).saturating_sub(1)]);
    (status == 0).then(|| PathBuf::from(path.replace('/', "\\")))
}
