//! Octopath Traveler II menu sounds in place of the game's.
//!
//! Every 2D sound the game plays, however it was requested (`PlaySound2D` by cue name,
//! `PlaySound2DByID`, `PlaySound2DByIndex`, gameplay-tag lookups), ends in the sound
//! manager's internal play function
//! `PlayCue(this, USoundAtomCue* Cue, const FString& CueSheet, FPLAY_SOUND_PARAMETER* Param)`.
//! That single function is detoured: the cue is identified by `USoundAtomCue::CueName`,
//! replaced cues play their Octopath Traveler II counterpart (decoded from the player's
//! own install, octopath.rs) through XAudio2 at the game's current Master x SE volume,
//! and the call returns -1 without playing anything, which every caller treats like a
//! missing cue (callers free Param themselves). Cues mapped to None are only silenced.
//!
//! FFRS often voices one UI action with several cues, where Octopath plays one sound:
//!   * several cues in the same game frame (confirm + window open, close + cancel +
//!     cancel, cursor + map target): only the first is replaced, the rest are silenced;
//!   * the cursor cue fired when focus settles after an action (a page change, leaving
//!     a sub menu, opening the main menu) arrives frames later with no input in between:
//!     a cursor cue that follows a different sound without any UI input since is
//!     silenced once.
//! Frames are the engine's frame counter; UI inputs are counted at the game's UI input
//! handler gate (`UCPP_GameWidgetInputManager`), which every accepted UI input action
//! passes through once.
//!
//! With `LogSounds=1` every cue the game plays and every UI input action is written to
//! the log with a timestamp, to find sounds that still need a replacement.

use core::ffi::c_void;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use retour::GenericDetour;

use crate::audio::{self, Sound};
use crate::log::log;

/// Game cue -> the Octopath Traveler II cue for the same action (None: silent).
pub const CUES: &[(&str, Option<&str>)] = &[
    ("SE_COM_PP_SYSTEM_CURSOR", Some("se_menu_select")),
    ("SE_COM_PP_SYSTEM_COMMON_OK", Some("se_menu_ok")),
    ("SE_COM_PP_SYSTEM_COMMON_NO", Some("se_menu_cancel")),
    ("SE_COM_PP_ERROR", Some("se_menu_unavailable")),
    ("SE_COM_PP_SYSTEM_MENU_SUB_OPEN", Some("se_menu_open")),
    ("SE_COM_PP_SYSTEM_MENU_SUB_CLOSE", Some("se_menu_close")),
    ("SE_COM_PP_SHOP_WINDOW_OPEN", Some("se_menu_open")),
    ("SE_COM_PP_SYSTEM_MENU_MAIN_OPEN", Some("se_menu_open")),
    ("SE_COM_PP_SYSTEM_MENU_MAIN_CLOSE", Some("se_menu_close")),
    // LB/RB pages: Octopath's lists use the cursor sound
    ("SE_COM_PP_SYSTEM_TAB_CHANGE", Some("se_menu_select")),
    ("SE_COM_PP_EQUIPMENT_EQ_ON", Some("se_menu_equip")),
    ("SE_COM_PP_EQUIPMENT_AB_ON", Some("se_menu_equip")),
    ("SE_COM_PP_EQUIPMENT_VC_ON", Some("se_menu_equip")),
    ("SE_COM_PP_EQUIPMENT_EQ_OFF", Some("se_menu_equip_off")),
    ("SE_COM_PP_EQUIPMENT_AB_OFF", Some("se_menu_equip_off")),
    ("SE_COM_PP_EQUIPMENT_VC_OFF", Some("se_menu_equip_off")),
    ("SE_COM_PP_SHOP_COMMON_BUY", Some("se_menu_buy")),
    ("SE_COM_PP_SYSTEM_MINIMAP", Some("se_menu_mapzoom")),
    // the map cursor reaching a destination, and leaving it (silent, as in Octopath)
    ("SE_COM_PP_MAP_POINT_TARGET_ON", Some("se_menu_select")),
    ("SE_COM_PP_MAP_POINT_TARGET_OFF", None),
];

/// The Octopath Traveler II cues the table uses.
pub fn octopath_cues() -> Vec<&'static str> {
    let mut cues: Vec<_> = CUES.iter().filter_map(|&(_, o)| o).collect();
    cues.sort_unstable();
    cues.dedup();
    cues
}

/// `USoundAtomCue::CueName`.
const CUE_NAME_OFFSET: usize = 0x30;

/// The sound manager's internal play-cue function that all PlaySound2D* variants call.
const PLAY_CUE_SIG: &str = "48 89 5C 24 10 55 56 57 41 56 41 57 48 8D 6C 24 C9 48 81 EC D0 00 00 00 \
     48 8B F1 49 8B F9 48 8D 4D C7 4D 8B F0 4C 8B FA E8 ?? ?? ?? ?? 80 7F 04 00 F3 0F 10 07";

/// `UCPP_GameWidgetInputManager`'s gate shared by all its UI input handlers:
/// `bool (this, UWidget* Target, const FString& ActionName)`.
const INPUT_GATE_SIG: &str = "40 53 48 83 EC 40 33 C0 48 89 7C 24 50 48 89 44 24 20 49 8B F8 48 89 44 24 28 \
     48 85 D2 74 ?? 8B 42 08 C1 E8 1E F6 D0 A8 01 74 ?? 48 8B 42 10 B3 01";

/// End of the engine tick: `++GFrameCounter` (`inc qword ptr [rip+rel32]` at +24).
const FRAME_COUNTER_SIG: &str = "83 FF 01 75 13 48 8B 03 8B D7 48 8B CB FF 50 08 EB 06 48 8B 01 FF 50 30 48 FF 05";

/// `UCPP_SoundManager::GetSoundTotalVolumeForCategory(eSoundCategory)`.
const VOLUME_SIG: &str = "48 8B 81 ?? ?? ?? ?? F3 0F 10 05 ?? ?? ?? ?? 0F B6 CA 48 8B 40 28 48 8B 14 C8 \
     48 8B 42 28 48 63 4A 30 48 C1 E1 06 48 03 C8 48 3B C1 74 ?? 66 90 F3 0F 59 40 08";

const CATEGORY_MASTER: u8 = 0;
const CATEGORY_SE: u8 = 2;

/// Octopath's raw samples peak near full scale; this sets them under the music.
const OCTOPATH_GAIN: f32 = 0.25;

/// The cue the game plays when focus moves to another entry.
const CURSOR_CUE: &str = "SE_COM_PP_SYSTEM_CURSOR";

#[repr(C)]
struct FString {
    data: *const u16,
    num: i32, // includes the terminating NUL
    max: i32,
}

type PlayCue = unsafe extern "system" fn(*mut c_void, *mut c_void, *const FString, *mut c_void) -> i32;
type InputGate = unsafe extern "system" fn(*mut c_void, *mut c_void, *const FString) -> bool;
type GetVolume = unsafe extern "system" fn(*mut c_void, u8) -> f32;

static PLAY_CUE: OnceLock<GenericDetour<PlayCue>> = OnceLock::new();
static INPUT_GATE: OnceLock<GenericDetour<InputGate>> = OnceLock::new();
static FRAME_COUNTER: OnceLock<usize> = OnceLock::new();
static GET_VOLUME: OnceLock<GetVolume> = OnceLock::new();
static SOUND_BANK: OnceLock<Vec<Option<Sound>>> = OnceLock::new();
static VOLUME: OnceLock<f32> = OnceLock::new();
static TRACE: AtomicBool = AtomicBool::new(false);
static START: OnceLock<Instant> = OnceLock::new();
static FIRST_USE: [AtomicBool; CUES.len()] = [const { AtomicBool::new(true) }; CUES.len()];
/// UI input actions accepted by the game so far.
static INPUTS: AtomicU64 = AtomicU64::new(0);
static LAST: Mutex<Last> = Mutex::new(Last { frame: u64::MAX, inputs: 0, cue: None });

/// The last replaced cue that took effect.
struct Last {
    frame: u64,
    inputs: u64,
    cue: Option<usize>,
}

fn fstring_text(s: *const FString) -> String {
    unsafe {
        match s.as_ref() {
            Some(s) if !s.data.is_null() && s.num > 1 => {
                String::from_utf16_lossy(core::slice::from_raw_parts(s.data, s.num as usize - 1))
            }
            _ => String::new(),
        }
    }
}

fn elapsed_ms() -> u128 {
    START.get_or_init(Instant::now).elapsed().as_millis()
}

/// Play the replacement for CUES[ci] if there is one. Returns false to let the game
/// play its own sound.
unsafe fn replace(this: *mut c_void, ci: usize) -> bool {
    let (name, octopath) = CUES[ci];
    let Some(octopath) = octopath else { return true };
    let Some(sound) = &SOUND_BANK.get().unwrap()[ci] else { return false };
    if let Some(reason) = same_action(ci) {
        if TRACE.load(Ordering::Relaxed) {
            log!("[trace] {:>8} {name} silenced ({reason})", elapsed_ms());
        }
        return true;
    }
    let game = GET_VOLUME
        .get()
        .map(|f| (f(this, CATEGORY_MASTER) * f(this, CATEGORY_SE)).clamp(0.0, 1.0))
        .unwrap_or(1.0);
    // first use of each cue is logged, so the log shows the replacement is live
    if FIRST_USE[ci].swap(false, Ordering::Relaxed) {
        log!("[sound] {name} -> {octopath} (game SE volume {game:.2})");
    }
    audio::play(sound, game * VOLUME.get().copied().unwrap_or(1.0));
    true
}

fn frame() -> Option<u64> {
    FRAME_COUNTER.get().map(|&at| unsafe { (at as *const u64).read_volatile() })
}

/// Whether CUES[ci] belongs to the action the last replaced sound already voiced
/// (see the module docs), and why. Records CUES[ci] as the last cue unless it shares
/// the last cue's frame.
fn same_action(ci: usize) -> Option<&'static str> {
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let frame = frame();
    let inputs = INPUTS.load(Ordering::Relaxed);
    let cursor = CUES[ci].0 == CURSOR_CUE;
    let reason = match last.cue {
        Some(_) if frame.is_some_and(|f| f == last.frame) => Some("same frame"),
        Some(prev) if cursor && CUES[prev].0 != CURSOR_CUE && INPUT_GATE.get().is_some() && inputs == last.inputs => {
            Some("focus settling, no input")
        }
        _ => None,
    };
    if reason != Some("same frame") {
        // a silenced focus cursor still counts as the latest cue, so cursor moves
        // that follow it (e.g. with the mouse) play normally
        *last = Last { frame: frame.unwrap_or(u64::MAX), inputs, cue: Some(ci) };
    }
    reason
}

unsafe extern "system" fn input_gate_hook(this: *mut c_void, target: *mut c_void, action: *const FString) -> bool {
    INPUTS.fetch_add(1, Ordering::Relaxed);
    if TRACE.load(Ordering::Relaxed) {
        log!("[trace] {:>8} input {}", elapsed_ms(), fstring_text(action));
    }
    INPUT_GATE.get().unwrap().call(this, target, action)
}

unsafe extern "system" fn play_cue_hook(this: *mut c_void, cue: *mut c_void, sheet: *const FString, param: *mut c_void) -> i32 {
    let original = PLAY_CUE.get().unwrap();
    let name = if cue.is_null() { String::new() } else { fstring_text((cue as *const u8).add(CUE_NAME_OFFSET).cast()) };
    let ci = CUES.iter().position(|(c, _)| *c == name);
    if TRACE.load(Ordering::Relaxed) {
        let how = if ci.is_some() { "replaced" } else { "game" };
        log!("[trace] {:>8} {}/{name} ({how})", elapsed_ms(), fstring_text(sheet));
    }
    if let Some(ci) = ci {
        if replace(this, ci) {
            return -1;
        }
    }
    original.call(this, cue, sheet, param)
}

/// One decoded sound per CUES entry (None for silenced cues).
fn load_bank(octopath: HashMap<&'static str, Vec<u8>>) -> Vec<Option<Sound>> {
    let pcm: HashMap<&str, &'static [u8]> = octopath.into_iter().map(|(cue, wav)| (cue, &*Box::leak(wav.into_boxed_slice()))).collect();
    CUES.iter()
        .map(|&(_, octopath)| {
            let octopath = octopath?;
            let sound = audio::parse_wav(pcm.get(octopath)?).map(|s| Sound { gain: OCTOPATH_GAIN, ..s });
            if sound.is_none() {
                log!("[sound] {octopath} is not a 16-bit PCM WAV; the game keeps its sound");
            }
            sound
        })
        .collect()
}

/// `octopath`: Octopath cue -> WAV for every cue in `octopath_cues()`.
pub fn install(volume: f32, trace: bool, octopath: HashMap<&'static str, Vec<u8>>) {
    let _ = VOLUME.set(volume);
    let _ = SOUND_BANK.set(load_bank(octopath));
    TRACE.store(trace, Ordering::Relaxed);
    START.get_or_init(Instant::now);

    match crate::scan::find_unique(VOLUME_SIG) {
        Ok(f) => {
            let _ = GET_VOLUME.set(unsafe { core::mem::transmute::<*mut u8, GetVolume>(f) });
        }
        Err(e) => log!("[sound] volume getter: {e} - using full volume"),
    }
    let target = match crate::scan::find_unique(PLAY_CUE_SIG) {
        Ok(t) => t,
        Err(e) => return log!("[sound] PlayCue: {e} - the game keeps its own sounds."),
    };
    let result = unsafe {
        GenericDetour::<PlayCue>::new(core::mem::transmute::<*mut u8, PlayCue>(target), play_cue_hook)
            .and_then(|d| d.enable().map(|_| d))
    };
    match result {
        Ok(d) => {
            let _ = PLAY_CUE.set(d);
            log!("[sound] PlayCue hooked at {target:p}");
        }
        Err(e) => return log!("[sound] hook failed: {e}"),
    }
    // one sound per UI action (module docs); without these the sounds still play, layered
    match crate::scan::find_unique(FRAME_COUNTER_SIG) {
        Ok(at) => unsafe {
            let inc = at.add(24);
            let rel = (inc.add(3) as *const i32).read_unaligned();
            let _ = FRAME_COUNTER.set(inc.add(7).offset(rel as isize) as usize);
        },
        Err(e) => log!("[sound] frame counter: {e}"),
    }
    match crate::scan::find_unique(INPUT_GATE_SIG) {
        Ok(t) => {
            let gate = unsafe {
                GenericDetour::<InputGate>::new(core::mem::transmute::<*mut u8, InputGate>(t), input_gate_hook)
                    .and_then(|d| d.enable().map(|_| d))
            };
            match gate {
                Ok(d) => {
                    let _ = INPUT_GATE.set(d);
                }
                Err(e) => log!("[sound] UI input gate hook failed: {e}"),
            }
        }
        Err(e) => log!("[sound] UI input gate: {e}"),
    }
    log!("[sound] {} cues replaced{}", CUES.len(), if trace { "; tracing every cue (LogSounds=1)" } else { "" });
}
