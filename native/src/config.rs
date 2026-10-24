//! `FFRSPatcher.ini` next to the DLL (written by the installer). Missing file or keys
//! fall back to defaults.

use std::path::PathBuf;

pub struct Config {
    pub analog_movement: bool,
    pub octopath_sounds: bool,
    pub octopath_path: Option<PathBuf>,
    pub sound_volume: f32,
    pub log_sounds: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { analog_movement: true, octopath_sounds: true, octopath_path: None, sound_volume: 1.0, log_sounds: false }
    }
}

pub fn load() -> Config {
    let mut cfg = Config::default();
    let Ok(text) = std::fs::read_to_string(crate::log::module_dir().join("FFRSPatcher.ini")) else {
        return cfg;
    };
    for line in text.lines() {
        let line = line.split([';', '#']).next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
        let flag = || matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on");
        match key.as_str() {
            "analogmovement" => cfg.analog_movement = flag(),
            "octopathsounds" => cfg.octopath_sounds = flag(),
            "octopathpath" if !value.is_empty() => cfg.octopath_path = Some(PathBuf::from(value.trim_matches('"'))),
            "soundvolume" => {
                if let Ok(v) = value.parse::<f32>() {
                    cfg.sound_volume = v.clamp(0.0, 4.0);
                }
            }
            "logsounds" => cfg.log_sounds = flag(),
            _ => {}
        }
    }
    cfg
}
