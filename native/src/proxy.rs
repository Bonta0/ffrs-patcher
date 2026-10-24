//! Forwarders for every export of the system `dsound.dll`.
//!
//! The game imports DirectSound by ordinal, so `dsound.def` pins each export
//! to its original ordinal. The real DLL is loaded from the system directory
//! on first use (never from inside DllMain).

use core::ffi::c_void;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{FARPROC, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;

type HRESULT = i32;
const E_FAIL: HRESULT = 0x80004005u32 as i32;

struct Real(HMODULE);
unsafe impl Send for Real {}
unsafe impl Sync for Real {}

static REAL: OnceLock<Real> = OnceLock::new();

fn real() -> HMODULE {
    REAL.get_or_init(|| unsafe {
        let mut dir = [0u16; 260];
        let n = GetSystemDirectoryW(dir.as_mut_ptr(), dir.len() as u32) as usize;
        let mut path: Vec<u16> = dir[..n].to_vec();
        path.extend("\\dsound.dll\0".encode_utf16());
        Real(LoadLibraryW(path.as_ptr()))
    })
    .0
}

fn proc(name: &[u8]) -> FARPROC {
    let m = real();
    if m.is_null() {
        return None;
    }
    unsafe { GetProcAddress(m, name.as_ptr()) }
}

macro_rules! forward {
    ($name:ident ( $($arg:ident : $ty:ty),* )) => {
        #[no_mangle]
        pub unsafe extern "system" fn $name($($arg: $ty),*) -> HRESULT {
            type F = unsafe extern "system" fn($($ty),*) -> HRESULT;
            match proc(concat!(stringify!($name), "\0").as_bytes()) {
                Some(p) => core::mem::transmute::<_, F>(p)($($arg),*),
                None => E_FAIL,
            }
        }
    };
}

type P = *mut c_void;

forward!(DirectSoundCreate(guid: P, ds: P, outer: P));
forward!(DirectSoundCreate8(guid: P, ds: P, outer: P));
forward!(DirectSoundEnumerateA(cb: P, ctx: P));
forward!(DirectSoundEnumerateW(cb: P, ctx: P));
forward!(DirectSoundCaptureCreate(guid: P, dsc: P, outer: P));
forward!(DirectSoundCaptureCreate8(guid: P, dsc: P, outer: P));
forward!(DirectSoundCaptureEnumerateA(cb: P, ctx: P));
forward!(DirectSoundCaptureEnumerateW(cb: P, ctx: P));
forward!(GetDeviceID(src: P, dest: P));
forward!(DirectSoundFullDuplexCreate(
    cap_guid: P, ren_guid: P, cap_desc: P, ren_desc: P, hwnd: P,
    level: u32, fd: P, cap_buf: P, ren_buf: P, outer: P
));
forward!(DllGetClassObject(clsid: P, iid: P, out: P));
forward!(DllCanUnloadNow());
