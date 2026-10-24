//! The theme's typeface, Alegreya Sans (SIL Open Font Licence, fonts/OFL.txt), in place
//! of the game's interface (Vogel) and dialogue (Alps) faces.
//!
//! Each font is rescaled (by lowering unitsPerEm: outlines, advances and kerning scale
//! together) and given the replaced face's vertical metrics, so line boxes and centring
//! in FFRS's widgets stay as they were. The battle "STAGGER" banners, words baked into
//! textures, are redrawn in Alegreya Sans Bold.

use std::collections::HashMap;
use std::io::BufWriter;
use std::path::Path;

use ab_glyph::{Font, FontRef, Glyph, PxScale};
use anyhow::{bail, Context, Result};

/// FFRS's font faces, as loose files in the theme's pak.
const FFRS_FONT_DIR: &str = "FFRS/Content/UI/Font/FontFace/SE/";

const MEDIUM: &[u8] = include_bytes!("../fonts/AlegreyaSans-Medium.ttf");
const BOLD: &[u8] = include_bytes!("../fonts/AlegreyaSans-Bold.ttf");
const ITALIC: &[u8] = include_bytes!("../fonts/AlegreyaSans-MediumItalic.ttf");

/// Vertical metrics (ascent, descent in em) of the faces they replace.
const VOGEL: (f32, f32) = (0.925, 0.223);
const ALPS: (f32, f32) = (0.949, 0.229);

/// FFRS face -> (font, scale, vertical metrics). Scales bring Alegreya's capitals close
/// to the replaced faces'.
const FACES: &[(&str, &[u8], f32, (f32, f32))] = &[
    ("SE-VOGEN.ufont", MEDIUM, 1.08, VOGEL),
    ("SE-VOGEB___.ufont", BOLD, 1.08, VOGEL),
    ("SE-VOGEI___.ufont", ITALIC, 1.08, VOGEL),
    ("SE-ALPSN.ufont", MEDIUM, 1.10, ALPS),
    ("SE-ALPSB.ufont", BOLD, 1.10, ALPS),
    ("SE-ALPSCB.ufont", BOLD, 0.95, ALPS), // the condensed slot: keep it narrow
    ("SE-ALPSI.ufont", ITALIC, 1.10, ALPS),
];
const BANNER_FONT: &[u8] = BOLD;

// ------------------------------------------------------------------ fonts

fn be16(d: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([d[at], d[at + 1]])
}
fn be32(d: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}
fn put16(d: &mut [u8], at: usize, v: u16) {
    d[at..at + 2].copy_from_slice(&v.to_be_bytes());
}

fn checksum(d: &[u8]) -> u32 {
    d.chunks(4).fold(0u32, |s, c| {
        let mut w = [0u8; 4];
        w[..c.len()].copy_from_slice(c);
        s.wrapping_add(u32::from_be_bytes(w))
    })
}

/// TrueType tables: tag -> (record offset, table offset, length).
fn tables(d: &[u8]) -> Result<HashMap<[u8; 4], (usize, usize, usize)>> {
    if d.len() < 12 || be32(d, 0) != 0x0001_0000 {
        bail!("not a TrueType font");
    }
    let mut out = HashMap::new();
    for i in 0..be16(d, 4) as usize {
        let rec = 12 + 16 * i;
        let tag: [u8; 4] = d[rec..rec + 4].try_into()?;
        let (offset, len) = (be32(d, rec + 8) as usize, be32(d, rec + 12) as usize);
        if offset + len > d.len() {
            bail!("truncated font");
        }
        out.insert(tag, (rec, offset, len));
    }
    Ok(out)
}

/// Rescale a TrueType font by `scale` and set its vertical metrics (in em).
fn retarget(font: &[u8], scale: f32, (ascent, descent): (f32, f32)) -> Result<Vec<u8>> {
    let mut d = font.to_vec();
    let t = tables(&d)?;
    let get = |tag: &[u8; 4]| t.get(tag).copied().with_context(|| format!("no {} table", String::from_utf8_lossy(tag)));
    let (_, head, _) = get(b"head")?;
    let (_, hhea, _) = get(b"hhea")?;
    let (_, os2, os2_len) = get(b"OS/2")?;
    let upm = ((be16(&d, head + 18) as f32 / scale).round() as u16).max(16);
    let (asc, desc) = ((ascent * upm as f32).round() as i16, (descent * upm as f32).round() as i16);
    put16(&mut d, head + 18, upm);
    put16(&mut d, hhea + 4, asc as u16);
    put16(&mut d, hhea + 6, (-desc) as u16);
    put16(&mut d, hhea + 8, 0);
    if os2_len >= 78 {
        let selection = be16(&d, os2 + 62) | 1 << 7; // USE_TYPO_METRICS
        put16(&mut d, os2 + 62, selection);
        put16(&mut d, os2 + 68, asc as u16);
        put16(&mut d, os2 + 70, (-desc) as u16);
        put16(&mut d, os2 + 72, 0);
        put16(&mut d, os2 + 74, asc as u16);
        put16(&mut d, os2 + 76, desc as u16);
    }
    // table checksums, then the whole-file adjustment in head
    d[head + 8..head + 12].copy_from_slice(&[0; 4]);
    for (rec, offset, len) in t.values().copied() {
        let sum = checksum(&d[offset..offset + len]);
        d[rec + 4..rec + 8].copy_from_slice(&sum.to_be_bytes());
    }
    let adjust = 0xB1B0_AFBAu32.wrapping_sub(checksum(&d));
    d[head + 8..head + 12].copy_from_slice(&adjust.to_be_bytes());
    Ok(d)
}

/// The theme's .pak: FFRS's font faces replaced by the theme's.
pub fn write_pak(path: &Path) -> Result<()> {
    let mut pak = repak::PakBuilder::new().writer(BufWriter::new(std::fs::File::create(path)?), repak::Version::V11, "../../../".into(), None);
    for &(face, font, scale, metrics) in FACES {
        let ttf = retarget(font, scale, metrics).with_context(|| format!("adapting the font for {face}"))?;
        // a cooked UFontFace payload: byte count, font data, 0
        let mut ufont = (ttf.len() as i32).to_le_bytes().to_vec();
        ufont.extend(&ttf);
        ufont.extend(0i32.to_le_bytes());
        pak.write_file(&format!("{FFRS_FONT_DIR}{face}"), false, ufont)?;
    }
    pak.write_index()?;
    Ok(())
}

// ------------------------------------------------------------------ banners

/// The battle banners: texture (under the game root, no extension) -> word.
/// Chinese keeps the game's (the typeface has no CJK glyphs).
pub const BANNERS: &[(&str, &str)] = &[
    ("FFRS/Content/UI/Textures/menu/T_Btl_Logo_Break", "BREAK"),
    ("FFRS/Content/UI/Textures/menu/T_Btl_Logo_FullBreak", "FULL BREAK"),
    ("FFRS/Content/L10N/en/UI/Textures/menu/T_Btl_Logo_Break", "STAGGERED"),
    ("FFRS/Content/L10N/en/UI/Textures/menu/T_Btl_Logo_FullBreak", "SWEEPING STAGGER"),
    ("FFRS/Content/L10N/de/UI/Textures/menu/T_Btl_Logo_Break", "GESCHOCKT"),
    ("FFRS/Content/L10N/de/UI/Textures/menu/T_Btl_Logo_FullBreak", "RADIKALER SCHOCK"),
    ("FFRS/Content/L10N/Fr/UI/Textures/menu/T_Btl_Logo_Break", "CHOC"),
    ("FFRS/Content/L10N/Fr/UI/Textures/menu/T_Btl_Logo_FullBreak", "CHOC TOTAL"),
    ("FFRS/Content/L10N/Es/UI/Textures/menu/T_Btl_Logo_Break", "VULNERABLE"),
    ("FFRS/Content/L10N/Es/UI/Textures/menu/T_Btl_Logo_FullBreak", "VENTAJA"),
];

const SS: usize = 4;
const KEYLINE: [f32; 3] = [150.0, 6.0, 10.0];
const GLOW: [f32; 3] = [214.0, 74.0, 52.0];
const FILL_STOPS: [(f32, [f32; 3]); 4] =
    [(0.0, [255.0, 252.0, 236.0]), (0.45, [255.0, 226.0, 160.0]), (0.75, [255.0, 176.0, 104.0]), (1.0, [232.0, 112.0, 64.0])];
const TRACKING: f32 = 0.05; // em
const WORD_SPACE: f32 = 0.12; // em

/// Banner size: texture width/height and capital height, in texture pixels.
fn banner_size(texture: &str) -> (usize, usize, f32) {
    if texture.ends_with("FullBreak") { (1024, 128, 62.0) } else { (512, 128, 58.0) }
}

fn gradient(t: f32) -> [f32; 3] {
    let i = FILL_STOPS.iter().rposition(|(p, _)| *p <= t).unwrap_or(0).min(FILL_STOPS.len() - 2);
    let ((p0, c0), (p1, c1)) = (FILL_STOPS[i], FILL_STOPS[i + 1]);
    let f = ((t - p0) / (p1 - p0)).clamp(0.0, 1.0);
    [0, 1, 2].map(|k| c0[k] + (c1[k] - c0[k]) * f)
}

/// Max filter (square, radius r), separable.
fn dilate(m: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut tmp = vec![0.0; m.len()];
    for y in 0..h {
        for x in 0..w {
            let (a, b) = (x.saturating_sub(r), (x + r).min(w - 1));
            tmp[y * w + x] = m[y * w + a..=y * w + b].iter().fold(0.0, |s: f32, v| s.max(*v));
        }
    }
    let mut out = vec![0.0; m.len()];
    for y in 0..h {
        let (a, b) = (y.saturating_sub(r), (y + r).min(h - 1));
        for x in 0..w {
            out[y * w + x] = (a..=b).fold(0.0, |s: f32, yy| s.max(tmp[yy * w + x]));
        }
    }
    out
}

/// Gaussian-like blur: three box blurs of radius r (separable, running sums).
fn blur(m: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let pass = |src: &[f32], horizontal: bool| -> Vec<f32> {
        let (n, lines) = if horizontal { (w, h) } else { (h, w) };
        let idx = |line: usize, i: usize| if horizontal { line * w + i } else { i * w + line };
        let mut out = vec![0.0; src.len()];
        let norm = 1.0 / (2 * r + 1) as f32;
        for line in 0..lines {
            let mut sum: f32 = (0..=r.min(n - 1)).map(|i| src[idx(line, i)]).sum();
            for i in 0..n {
                out[idx(line, i)] = sum * norm;
                if i + r + 1 < n {
                    sum += src[idx(line, i + r + 1)];
                }
                if i >= r {
                    sum -= src[idx(line, i - r)];
                }
            }
        }
        out
    };
    let mut v = m.to_vec();
    for _ in 0..3 {
        v = pass(&pass(&v, true), false);
    }
    v
}

/// Draw `text` as a banner: warm fiery fill, dark red keyline, red glow (RGBA, w x h).
fn render_banner(font: &FontRef, text: &str, w: usize, h: usize, cap: f32) -> Vec<u8> {
    let (bw, bh) = (w * SS, h * SS);
    let margin = 22.0 * SS as f32;
    let height_units = font.ascent_unscaled() - font.descent_unscaled();
    let em_units = font.units_per_em().unwrap_or(1000.0);
    // capital height in font units: 'H' drawn at one pixel per unit
    let cap_units = font
        .outline_glyph(font.glyph_id('H').with_scale(PxScale::from(height_units)))
        .map(|g| g.px_bounds().height())
        .unwrap_or(em_units * 0.6);
    // px per font unit: capitals `cap` texture pixels tall, shrunk to fit the width
    let mut k = cap * SS as f32 / cap_units;
    let step = |c: char, k: f32| font.h_advance_unscaled(font.glyph_id(c)) * k + (TRACKING + if c == ' ' { WORD_SPACE } else { 0.0 }) * em_units * k;
    let width = |k: f32| text.chars().map(|c| step(c, k)).sum::<f32>() - TRACKING * em_units * k;
    k *= ((bw as f32 - 2.0 * margin) / width(k)).min(1.0);
    let scale = PxScale::from(k * height_units);
    let cap_px = cap_units * k;
    let top = (bh as f32 - cap_px) / 2.0;
    let baseline = top + cap_px;

    let mut mask = vec![0.0f32; bw * bh];
    let mut x = (bw as f32 - width(k)) / 2.0;
    for c in text.chars() {
        let glyph: Glyph = font.glyph_id(c).with_scale_and_position(scale, ab_glyph::point(x, baseline));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let b = outlined.px_bounds();
            outlined.draw(|gx, gy, v| {
                let (px, py) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                if px >= 0 && py >= 0 && (px as usize) < bw && (py as usize) < bh {
                    let m = &mut mask[py as usize * bw + px as usize];
                    *m = m.max(v);
                }
            });
        }
        x += step(c, k);
    }

    let key = dilate(&mask, bw, bh, (1.4 * SS as f32).round() as usize);
    let glow = blur(&dilate(&mask, bw, bh, SS), bw, bh, 3 * SS);
    // flame mottling: coarse value noise, more of it low down
    let (nw, nh) = (bw / (6 * SS) + 2, bh / (6 * SS) + 2);
    let mut seed = 0x9E37_79B9u32 ^ text.len() as u32;
    let grid: Vec<f32> = (0..nw * nh)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed % 1000) as f32 / 1000.0
        })
        .collect();
    let noise = |x: usize, y: usize| {
        let (fx, fy) = (x as f32 / (6 * SS) as f32, y as f32 / (6 * SS) as f32);
        let (x0, y0) = (fx as usize, fy as usize);
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let g = |gx: usize, gy: usize| grid[gy.min(nh - 1) * nw + gx.min(nw - 1)];
        let a = g(x0, y0) + (g(x0 + 1, y0) - g(x0, y0)) * tx;
        let b = g(x0, y0 + 1) + (g(x0 + 1, y0 + 1) - g(x0, y0 + 1)) * tx;
        a + (b - a) * ty
    };
    let hot = FILL_STOPS[3].1;

    let mut big = vec![[0.0f32; 4]; bw * bh];
    for y in 0..bh {
        let row = ((y as f32 - top) / cap_px.max(1.0)).clamp(0.0, 1.0);
        let base = gradient(row);
        for x in 0..bw {
            let i = y * bw + x;
            let kmix = 0.35 * noise(x, y) * (0.3 + 0.7 * row);
            let fill = [0, 1, 2].map(|c| base[c] * (1.0 - kmix) + hot[c] * kmix);
            // glow, then keyline, then fill, each over what is below
            let mut px = [GLOW[0], GLOW[1], GLOW[2], (glow[i] * 1.15).clamp(0.0, 1.0) * 0.85];
            for (colour, alpha) in [(KEYLINE, key[i]), (fill, mask[i])] {
                let a0 = px[3];
                let na = alpha + a0 * (1.0 - alpha);
                if na > 1e-6 {
                    for c in 0..3 {
                        px[c] = (colour[c] * alpha + px[c] * a0 * (1.0 - alpha)) / na;
                    }
                }
                px[3] = na;
            }
            big[i] = px;
        }
    }
    // downsample SS x SS (alpha-weighted colour)
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let p = big[(y * SS + sy) * bw + x * SS + sx];
                    for c in 0..3 {
                        acc[c] += p[c] * p[3];
                    }
                    acc[3] += p[3];
                }
            }
            let o = (y * w + x) * 4;
            for c in 0..3 {
                out[o + c] = if acc[3] > 0.0 { (acc[c] / acc[3]).round().clamp(0.0, 255.0) as u8 } else { 0 };
            }
            out[o + 3] = (acc[3] / (SS * SS) as f32 * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// The banner textures' .uasset paths, to extract from the game.
pub fn banner_packages() -> Vec<String> {
    BANNERS.iter().map(|(t, _)| format!("{t}.uasset")).collect()
}

/// Redraw the banners into the extracted textures (`files`: path -> legacy file).
pub fn draw_banners(files: &mut HashMap<String, Vec<u8>>) -> Result<()> {
    let font = FontRef::try_from_slice(BANNER_FONT)?;
    for &(texture, word) in BANNERS {
        let (w, h, cap) = banner_size(texture);
        let rgba = render_banner(&font, word, w, h, cap);
        let uexp = files.get_mut(&format!("{texture}.uexp")).with_context(|| format!("{texture} not extracted"))?;
        crate::art::inject(uexp, &rgba, w, h).with_context(|| format!("drawing {texture}"))?;
    }
    Ok(())
}

