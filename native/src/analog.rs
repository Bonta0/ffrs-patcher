//! Full analog field movement.
//!
//! `ACPP_MapUnit_Playable::InputMove` (game v1.0.1.0, UE 5.6) does:
//!
//! ```text
//! m_InputActionValue = value;                       // FVector2D, after the IA's radial dead zone
//! if (abs(m_InputActionValue.X) < 0.4) m_InputActionValue.X = 0;
//! if (abs(m_InputActionValue.Y) < 0.4) m_InputActionValue.Y = 0;
//! ```
//!
//! Those per-axis cut-offs snap every stick angle within ~24 degrees of an axis onto
//! that axis (much wider at partial tilt), which is what locks the character to
//! cardinals and diagonals; everything downstream (Movement -> AddMovementInput) is
//! analog. Each guard is `comisd; jae +7; mov [rcx+off], rax`: turning the `jae`
//! (73 07) into `jmp` (EB 07) skips the zeroing. The input action's own radial dead
//! zone is untouched, so the stick still rests cleanly at centre.

use crate::log::log;
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_EXECUTE_READWRITE};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// `ACPP_MapUnit_Playable::InputMove`; field offsets and RIP-relative constants are wildcards.
const INPUT_MOVE_SIG: &str = "48 8B 02 F2 0F 10 42 08 48 89 81 ?? ?? ?? ?? 33 C0 \
     F2 0F 11 81 ?? ?? ?? ?? F2 0F 10 89 ?? ?? ?? ?? \
     F2 0F 10 15 ?? ?? ?? ?? F2 0F 10 1D ?? ?? ?? ?? \
     0F 54 CA 66 0F 2F CB 73 07 48 89 81 ?? ?? ?? ?? \
     0F 54 C2 66 0F 2F C3 73 07";

/// Offsets (from the signature start) of the two `jae` opcodes.
const JAE_OFFSETS: [usize; 2] = [0x38, 0x48];

unsafe fn write_byte(at: *mut u8, value: u8) -> bool {
    let mut old = 0u32;
    if VirtualProtect(at.cast(), 1, PAGE_EXECUTE_READWRITE, &mut old) == 0 {
        return false;
    }
    at.write_volatile(value);
    VirtualProtect(at.cast(), 1, old, &mut old);
    FlushInstructionCache(GetCurrentProcess(), at.cast(), 1);
    true
}

pub fn apply() {
    match crate::scan::find_unique(INPUT_MOVE_SIG) {
        Err(e) => log!("[analog] InputMove: {e} - nothing patched."),
        Ok(func) => unsafe {
            log!("[analog] InputMove found at {func:p}");
            for off in JAE_OFFSETS {
                let at = func.add(off);
                if write_byte(at, 0xEB) {
                    log!("[analog]   per-axis snap removed at {at:p}");
                } else {
                    log!("[analog]   VirtualProtect failed at {at:p}");
                }
            }
        },
    }
}
