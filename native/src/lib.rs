//! FFRS Patcher: native part of the FINAL FANTASY RESONANCE mod.
//!
//! Loaded by the game as a `dsound.dll` proxy (all DirectSound exports are forwarded
//! to the system DLL). On load a worker thread:
//!   * removes the per-axis input snap so field movement is fully analog (analog.rs)
//!   * replaces the menu/UI sound effects with Octopath Traveler II's, read from the
//!     player's own install (octopath.rs, sound.rs); without it the game's stay
//! Both are located by byte signature and can be toggled in FFRSPatcher.ini.
//! Nothing is written to disk except FFRSPatcher.log.

#![allow(non_snake_case)]

mod analog;
mod audio;
mod config;
mod log;
mod octopath;
mod proxy;
mod scan;
mod sound;

use core::ffi::c_void;
use windows_sys::Win32::Foundation::{BOOL, HMODULE, TRUE};
use windows_sys::Win32::System::LibraryLoader::DisableThreadLibraryCalls;
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows_sys::Win32::System::Threading::CreateThread;

static mut SELF_MODULE: HMODULE = core::ptr::null_mut();

pub(crate) fn self_module() -> HMODULE {
    unsafe { SELF_MODULE }
}

unsafe extern "system" fn worker(_: *mut c_void) -> u32 {
    log::reset();
    log::log!("FFRS Patcher {}", env!("CARGO_PKG_VERSION"));
    let cfg = config::load();
    if cfg.analog_movement {
        analog::apply();
    } else {
        log::log!("[analog] disabled in FFRSPatcher.ini");
    }
    if !cfg.octopath_sounds {
        log::log!("[sound] Octopath sounds disabled in FFRSPatcher.ini");
    } else if let Some(sounds) = octopath::load(cfg.octopath_path.as_deref(), &sound::octopath_cues()) {
        sound::install(cfg.sound_volume, cfg.log_sounds, sounds);
    }
    0
}

#[no_mangle]
pub unsafe extern "system" fn DllMain(module: HMODULE, reason: u32, _: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        SELF_MODULE = module;
        DisableThreadLibraryCalls(module);
        // Loader lock is held here: do the real work on a separate thread.
        CreateThread(core::ptr::null(), 0, Some(worker), core::ptr::null(), 0, core::ptr::null_mut());
    }
    TRUE
}
