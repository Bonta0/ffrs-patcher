//! Minimal append-only log next to the DLL (`FFRSPatcher.log`).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;

static LOCK: Mutex<()> = Mutex::new(());

/// Directory that contains this DLL.
pub fn module_dir() -> PathBuf {
    let mut buf = [0u16; 1024];
    let n = unsafe { GetModuleFileNameW(crate::self_module(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    let path = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
}

pub fn reset() {
    let _ = std::fs::remove_file(module_dir().join("FFRSPatcher.log"));
}

pub fn write(msg: &str) {
    let _g = LOCK.lock();
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(module_dir().join("FFRSPatcher.log")) {
        let _ = writeln!(f, "{msg}");
    }
}

macro_rules! log {
    ($($t:tt)*) => { $crate::log::write(&format!($($t)*)) };
}
pub(crate) use log;
