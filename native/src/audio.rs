//! Small XAudio2 one-shot player for the replacement menu sounds.
//!
//! Each PCM format in use gets a small pool of source voices, reused round-robin; a
//! voice that is still playing when its turn comes is simply cut off (menu sounds are
//! short and the pool is deep enough for fast cursoring).

use std::sync::Mutex;

use windows::Win32::Media::Audio::XAudio2::{
    IXAudio2, IXAudio2MasteringVoice, IXAudio2SourceVoice, XAudio2CreateWithVersionInfo, XAUDIO2_BUFFER,
    XAUDIO2_DEFAULT_PROCESSOR, XAUDIO2_END_OF_STREAM,
};
use windows::Win32::Media::Audio::{AudioCategory_GameEffects, WAVEFORMATEX};

const POOL: usize = 12;
const NTDDI_WIN10: u32 = 0x0A00_0000;

/// A decoded WAV: format header, a reference to its PCM payload and a playback gain.
pub struct Sound {
    pub format: WAVEFORMATEX,
    pub pcm: &'static [u8],
    pub gain: f32,
}

/// Minimal RIFF/WAVE parser (PCM only).
pub fn parse_wav(bytes: &'static [u8]) -> Option<Sound> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let (mut fmt, mut pcm) = (None, None);
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = bytes.get(pos + 8..pos + 8 + len)?;
        match id {
            b"fmt " if len >= 16 => {
                let u16_at = |o: usize| u16::from_le_bytes([body[o], body[o + 1]]);
                let u32_at = |o: usize| u32::from_le_bytes(body[o..o + 4].try_into().unwrap());
                fmt = Some(WAVEFORMATEX {
                    wFormatTag: u16_at(0),
                    nChannels: u16_at(2),
                    nSamplesPerSec: u32_at(4),
                    nAvgBytesPerSec: u32_at(8),
                    nBlockAlign: u16_at(12),
                    wBitsPerSample: u16_at(14),
                    cbSize: 0,
                })
            }
            b"data" => pcm = Some(body),
            _ => {}
        }
        pos += 8 + len + (len & 1);
    }
    let format = fmt?;
    (format.wFormatTag == 1).then_some(Sound { format, pcm: pcm?, gain: 1.0 })
}

struct Pool {
    key: (u16, u32, u16), // channels, sample rate, bits
    voices: Vec<IXAudio2SourceVoice>,
    next: usize,
}

struct Engine {
    xaudio: IXAudio2,
    _master: IXAudio2MasteringVoice,
    pools: Vec<Pool>,
}

// XAudio2 interfaces are free-threaded; access is serialised by the mutex anyway.
unsafe impl Send for Engine {}

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

unsafe fn create() -> windows::core::Result<Engine> {
    let mut xaudio: Option<IXAudio2> = None;
    XAudio2CreateWithVersionInfo(&mut xaudio, 0, XAUDIO2_DEFAULT_PROCESSOR, NTDDI_WIN10)?;
    let xaudio = xaudio.unwrap();
    let mut master = None;
    xaudio.CreateMasteringVoice(&mut master, 0, 0, 0, None, None, AudioCategory_GameEffects)?;
    Ok(Engine { xaudio, _master: master.unwrap(), pools: Vec::new() })
}

unsafe fn create_pool(xaudio: &IXAudio2, format: &WAVEFORMATEX) -> windows::core::Result<Pool> {
    let mut voices = Vec::with_capacity(POOL);
    for _ in 0..POOL {
        let mut v = None;
        xaudio.CreateSourceVoice(&mut v, format, 0, 2.0, None, None, None)?;
        voices.push(v.unwrap());
    }
    Ok(Pool { key: (format.nChannels, format.nSamplesPerSec, format.wBitsPerSample), voices, next: 0 })
}

/// Play `sound` once at `volume` x its own gain. Errors are logged and ignored.
pub fn play(sound: &Sound, volume: f32) {
    let mut guard = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        match unsafe { create() } {
            Ok(e) => *guard = Some(e),
            Err(e) => {
                crate::log::log!("[sound] XAudio2 init failed: {e}");
                return;
            }
        }
    }
    let engine = guard.as_mut().unwrap();
    let f = &sound.format;
    let key = (f.nChannels, f.nSamplesPerSec, f.wBitsPerSample);
    let index = match engine.pools.iter().position(|p| p.key == key) {
        Some(i) => i,
        None => match unsafe { create_pool(&engine.xaudio, f) } {
            Ok(pool) => {
                engine.pools.push(pool);
                engine.pools.len() - 1
            }
            Err(e) => {
                crate::log::log!("[sound] no voice for {key:?}: {e}");
                return;
            }
        },
    };
    let pool = &mut engine.pools[index];
    let voice = &pool.voices[pool.next];
    pool.next = (pool.next + 1) % pool.voices.len();
    let buffer = XAUDIO2_BUFFER {
        Flags: XAUDIO2_END_OF_STREAM,
        AudioBytes: sound.pcm.len() as u32,
        pAudioData: sound.pcm.as_ptr(),
        ..Default::default()
    };
    unsafe {
        let _ = voice.Stop(0, 0);
        let _ = voice.FlushSourceBuffers();
        let _ = voice.SetVolume(volume * sound.gain, 0);
        if voice.SubmitSourceBuffer(&buffer, None).is_ok() {
            let _ = voice.Start(0, 0);
        }
    }
}
