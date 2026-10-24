//! Byte-signature scanning over the game executable's code sections.

use windows_sys::Win32::System::Diagnostics::Debug::{IMAGE_NT_HEADERS64, IMAGE_SECTION_HEADER};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemServices::IMAGE_DOS_HEADER;

const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;

/// Parse "48 8B ?? 05" style signatures; `??` is a wildcard.
pub fn parse(sig: &str) -> Vec<Option<u8>> {
    sig.split_whitespace()
        .map(|b| if b == "??" { None } else { Some(u8::from_str_radix(b, 16).expect("bad signature byte")) })
        .collect()
}

fn find_in(hay: &[u8], sig: &[Option<u8>], hits: &mut Vec<usize>) {
    let first = sig[0].expect("signature must start with a concrete byte");
    let mut i = 0;
    while i + sig.len() <= hay.len() {
        match hay[i..hay.len() - sig.len() + 1].iter().position(|&b| b == first) {
            None => break,
            Some(p) => i += p,
        }
        if sig.iter().zip(&hay[i..]).all(|(s, h)| s.map_or(true, |s| s == *h)) {
            hits.push(i);
        }
        i += 1;
    }
}

/// Executable sections of the main game module.
unsafe fn code_sections() -> Vec<&'static [u8]> {
    let base = GetModuleHandleW(core::ptr::null()) as *const u8;
    let dos = &*(base as *const IMAGE_DOS_HEADER);
    let nt = &*(base.add(dos.e_lfanew as usize) as *const IMAGE_NT_HEADERS64);
    let first = (nt as *const _ as *const u8)
        .add(core::mem::size_of::<u32>() + core::mem::size_of_val(&nt.FileHeader))
        .add(nt.FileHeader.SizeOfOptionalHeader as usize) as *const IMAGE_SECTION_HEADER;
    (0..nt.FileHeader.NumberOfSections as usize)
        .map(|i| &*first.add(i))
        .filter(|s| s.Characteristics & IMAGE_SCN_MEM_EXECUTE != 0)
        .map(|s| core::slice::from_raw_parts(base.add(s.VirtualAddress as usize), s.Misc.VirtualSize as usize))
        .collect()
}

/// Every address in the game's code matching `sig`.
pub fn find_all(sig: &str) -> Vec<*mut u8> {
    let sig = parse(sig);
    let mut out = Vec::new();
    unsafe {
        for sec in code_sections() {
            let mut hits = Vec::new();
            find_in(sec, &sig, &mut hits);
            out.extend(hits.into_iter().map(|o| sec.as_ptr().add(o) as *mut u8));
        }
    }
    out
}

/// The single address matching `sig`, or a description of why there isn't exactly one.
pub fn find_unique(sig: &str) -> Result<*mut u8, String> {
    match find_all(sig).as_slice() {
        [one] => Ok(*one),
        [] => Err("signature not found (unsupported game version?)".into()),
        many => Err(format!("signature matched {} places", many.len())),
    }
}
