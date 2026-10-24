//! Octopath Traveler II interface sounds, taken from the player's own installation.
//!
//! Nothing from Octopath is shipped with the mod. At startup the system sound bank
//! (`CueSheet_SE_System`) is read from the local Octopath Traveler II pak and the cues
//! the mod uses are decoded in memory (CRI HCA -> PCM WAV). Nothing is written to disk.
//! Without an Octopath Traveler II install the game keeps its own sounds.

use std::collections::HashMap;
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};

use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

use crate::log::log;

const PAK: &str = "Octopath_Traveler2/Content/Paks/Octopath_Traveler2-WindowsNoEditor.pak";
const CUE_SHEET: &str = "Octopath_Traveler2/Content/Sound/Resource/SE/System/CueSheet_SE_System.uexp";

/// Octopath Traveler II's install folder (the one holding `Octopath_Traveler2/`).
fn find_install(configured: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = configured.into_iter().map(Path::to_path_buf).collect();
    // FFRS's own Steam library first, then every library Steam knows about
    let here = crate::log::module_dir();
    if let Some(common) = here.ancestors().find(|p| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("common"))) {
        candidates.push(common.join("Octopath_Traveler2"));
    }
    for library in steam_libraries() {
        candidates.push(library.join("steamapps").join("common").join("Octopath_Traveler2"));
    }
    candidates.into_iter().find(|c| c.join(PAK).is_file())
}

fn steam_libraries() -> Vec<PathBuf> {
    let Some(steam) = steam_path() else { return Vec::new() };
    let mut libraries = vec![steam.clone()];
    let vdf = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")).unwrap_or_default();
    for line in vdf.lines() {
        let mut fields = line.split('"').skip(1).step_by(2);
        if let (Some("path"), Some(path)) = (fields.next(), fields.next()) {
            libraries.push(PathBuf::from(path.replace("\\\\", "\\")));
        }
    }
    libraries
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

/// Decode the cue sheet and return Octopath cue -> WAV bytes for the requested cues.
fn decode(install: &Path, cues: &[&'static str]) -> Result<HashMap<&'static str, Vec<u8>>, String> {
    let mut file = BufReader::new(std::fs::File::open(install.join(PAK)).map_err(|e| e.to_string())?);
    let pak = repak::PakBuilder::new().reader(&mut file).map_err(|e| e.to_string())?;
    let uexp = pak.get(CUE_SHEET, &mut file).map_err(|e| e.to_string())?;
    // the cooked SoundAtomCueSheet carries the raw ACB (with its AWB inlined) after the tag data
    let start = uexp.windows(4).position(|w| w == b"@UTF").ok_or("no ACB in cue sheet")?;
    let tracks = cridecoder::decode_acb_to_wav_to_memory(Cursor::new(&uexp[start..]), None, None).map_err(|e| e.to_string())?;
    let mut by_cue: HashMap<String, Vec<u8>> = tracks.into_iter().filter(|t| t.extension == "wav").map(|t| (t.name, t.data)).collect();
    let sounds: HashMap<_, _> = cues.iter().filter_map(|&cue| Some((cue, by_cue.remove(cue)?))).collect();
    match cues.iter().find(|cue| !sounds.contains_key(*cue)) {
        Some(missing) => Err(format!("cue {missing} not in the sound bank")),
        None => Ok(sounds),
    }
}

/// Octopath cue -> WAV for each of `cues`, or None when Octopath Traveler II is not
/// installed or its sound bank cannot be read (the game then keeps its own sounds).
pub fn load(configured: Option<&Path>, cues: &[&'static str]) -> Option<HashMap<&'static str, Vec<u8>>> {
    let Some(install) = find_install(configured) else {
        log!("[octopath] Octopath Traveler II not found - the game keeps its own sounds");
        return None;
    };
    let started = std::time::Instant::now();
    // a malformed pak must never take the game down with it
    match std::panic::catch_unwind(|| decode(&install, cues)) {
        Ok(Ok(sounds)) => {
            log!("[octopath] {} sounds from {} ({} ms)", sounds.len(), install.display(), started.elapsed().as_millis());
            Some(sounds)
        }
        Ok(Err(e)) => {
            log!("[octopath] could not read {}: {e} - the game keeps its own sounds", install.display());
            None
        }
        Err(_) => {
            log!("[octopath] decoder failed - the game keeps its own sounds");
            None
        }
    }
}
