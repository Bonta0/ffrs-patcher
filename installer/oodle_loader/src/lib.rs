//! Stands in for repak's `oodle_loader`, used by retoc and repak to read Oodle-compressed
//! game files. The original downloads Oodle's DLL at runtime; this decodes with the
//! pure-Rust oozextract instead. There is no open-source Oodle encoder, so compressing
//! fails: the patcher only writes uncompressed files.

#[derive(Debug, Clone, Copy)]
pub enum Compressor {
    Mermaid,
}

#[derive(Debug, Clone, Copy)]
pub enum CompressionLevel {
    Normal,
}

#[derive(Debug)]
pub enum Error {
    CompressionUnavailable,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Oodle compression is not available")
    }
}

impl std::error::Error for Error {}

pub struct Oodle;

pub fn oodle() -> Result<&'static Oodle, Error> {
    Ok(&Oodle)
}

impl Oodle {
    pub fn compress(&self, _input: &[u8], _compressor: Compressor, _level: CompressionLevel) -> Result<Vec<u8>, Error> {
        Err(Error::CompressionUnavailable)
    }

    /// Bytes written to `output`, or -1 on failure (as Oodle's own decoder).
    pub fn decompress(&self, input: &[u8], output: &mut [u8]) -> isize {
        oozextract::Extractor::new().read_from_slice(input, output).map_or(-1, |n| n as isize)
    }
}
