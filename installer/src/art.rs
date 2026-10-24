//! The theme's menu textures (Octopath II-inspired: charcoal translucent panels, thin
//! white hairlines, white outlines on the selection, a gloved hand cursor), drawn here
//! from scratch at the exact size of each texture they replace.
//!
//! Shapes are drawn without anti-aliasing at SS x the size, then downsampled (Lanczos),
//! which gives clean edges.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};

const SS: usize = 4;
const DIR: &str = "FFRS/Content/UI/Textures/menu/";

type Rgb = [f32; 3];
const INK: Rgb = [12.0, 12.0, 14.0];
const INK_LIGHT: Rgb = [46.0, 46.0, 52.0];
const LINE: Rgb = [214.0, 214.0, 220.0];
const LINE_BRIGHT: Rgb = [255.0, 255.0, 255.0];
const LINE_DIM: Rgb = [118.0, 118.0, 126.0];
const SELECT: Rgb = [120.0, 120.0, 130.0];

/// Texture name -> drawing.
const TEXTURES: &[(&str, fn() -> Img)] = &[
    ("T_CommonInsideFrame", || inside_frame(128)),
    ("T_Window_Frame_Box_3_2", || window_fill(256, 0.0)),
    ("T_Window_Frame_Box_4_2", || window_fill(256, 14.0)),
    ("T_Event_TalkWindow_BG", || talk_window_bg(256)),
    ("T_CommonBalloon", || balloon(256, 128)),
    ("T_CommonListSelect", || select_bar(256, 32, false)),
    ("T_Btl_CommandListSelect", || select_bar(256, 32, false)),
    ("T_Btl_CommandGoldListSelect", || select_bar(256, 32, true)),
    ("T_Dialog_Default", || dialog_row(512, 32, false)),
    ("T_Dialog_Select", || dialog_row(512, 32, true)),
    ("T_mainmenu_Listlabel", || plate(256, 64, 10, Plate::Idle)),
    ("T_mainmenu_Listlabel_Hold", || plate(256, 64, 10, Plate::Hold)),
    ("T_mainmenu_Listlabel_Select", || plate(256, 64, 10, Plate::Select)),
    ("T_CharaListBG", || plate(128, 64, 9, Plate::Idle)),
    ("T_CharaListBGSelect", || plate(128, 64, 9, Plate::Select)),
    ("T_CharaMenuListBg", || chara_list_bg(300, 512)),
    ("T_CharaMenuListFrame", || chara_list_frame(300, 512)),
    ("T_menu_Title_Line00", || rule(512, 128, Fade::Both, 1.8, 8, true)),
    ("T_menu_Line_Light", || rule(2048, 128, Fade::Both, 2.0, 8, false)),
    ("T_menu_Line_Light_Left", || rule(1024, 128, Fade::Left, 2.0, 8, false)),
    ("T_Menu_line_center", || rule(512, 4, Fade::Both, 1.4, 0, false)),
    ("T_CommonWindowSelectFrame", || select_frame(64)),
    ("T_menu_Finger_Light", || radial_glow(64, LINE_BRIGHT, 2.0, 0.9)),
    ("MenuIcon_FingerCursor_R", || cursor(64)),
    // battle
    ("T_BtlWindow_Active", || battle_panel(128, true)),
    ("T_BtlWindow_Inactive", || battle_panel(128, false)),
    ("T_Vision_Unselect", || ink_band(256, 32)),
    ("T_Btl_FinishBlow_KeyBg", || key_disc(128)),
];

/// The textures' .uasset paths, to extract from the game.
pub fn packages() -> Vec<String> {
    TEXTURES.iter().map(|(n, _)| format!("{DIR}{n}.uasset")).collect()
}

/// Draw the textures into the extracted texture files (`files`: path -> legacy file).
pub fn draw(files: &mut HashMap<String, Vec<u8>>) -> Result<()> {
    for (name, make) in TEXTURES {
        let img = make();
        let uexp = files.get_mut(&format!("{DIR}{name}.uexp")).with_context(|| format!("{name} not extracted"))?;
        inject(uexp, &img.rgba8(), img.w, img.h).with_context(|| format!("drawing {name}"))?;
    }
    Ok(())
}

/// All textures as (name, width, height, RGBA), to preview.
#[allow(dead_code)] // theme-dev
pub fn render_all() -> Vec<(&'static str, usize, usize, Vec<u8>)> {
    TEXTURES.iter().map(|(n, make)| {
        let img = make();
        (*n, img.w, img.h, img.rgba8())
    }).collect()
}

/// Write RGBA pixels into every mip of a cooked uncompressed BGRA texture (.uexp).
/// Mip payloads sit right before each mip's (SizeX, SizeY, SizeZ = 1) record.
pub fn inject(uexp: &mut [u8], rgba: &[u8], w: usize, h: usize) -> Result<()> {
    let format = b"PF_B8G8R8A8\0";
    let at = uexp.windows(format.len()).position(|x| x == format).context("not a BGRA texture")?;
    let le = |i: usize| i32::from_le_bytes(uexp[i..i + 4].try_into().unwrap());
    let (sx, sy) = (le(at - 16) as usize, le(at - 12) as usize);
    if (sx, sy) != (w, h) {
        bail!("texture is {sx}x{sy}, image is {w}x{h}");
    }
    let mut pos = at + format.len();
    let mut mip = 0;
    loop {
        let (mw, mh) = ((sx >> mip).max(1), (sy >> mip).max(1));
        let record: Vec<u8> = [mw as i32, mh as i32, 1].iter().flat_map(|v| v.to_le_bytes()).collect();
        let from = pos + if mip > 0 { mw * mh * 4 } else { 0 };
        let Some(found) = uexp.get(from..).and_then(|s| s.windows(12).position(|x| x == record.as_slice())) else { break };
        let rec = from + found;
        let n = mw * mh * 4;
        if rec < n {
            bail!("bad mip layout");
        }
        // box-downsample for smaller mips, BGRA order
        let f = sx / mw;
        for y in 0..mh {
            for x in 0..mw {
                let mut acc = [0u32; 4];
                for yy in 0..f {
                    for xx in 0..f {
                        let s = ((y * f + yy) * w + x * f + xx) * 4;
                        for c in 0..4 {
                            acc[c] += rgba[s + c] as u32;
                        }
                    }
                }
                let d = rec - n + (y * mw + x) * 4;
                let n2 = (f * f) as u32;
                uexp[d..d + 4].copy_from_slice(&[(acc[2] / n2) as u8, (acc[1] / n2) as u8, (acc[0] / n2) as u8, (acc[3] / n2) as u8]);
            }
        }
        pos = rec + 12;
        mip += 1;
    }
    if mip == 0 {
        bail!("mip data not found");
    }
    Ok(())
}

// ---------------------------------------------------------------- image & drawing

/// RGBA image, straight alpha, channels 0..255.
#[derive(Clone)]
struct Img {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

fn rgba(c: Rgb, a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

impl Img {
    fn new(w: usize, h: usize) -> Img {
        Img { w, h, px: vec![[0.0; 4]; w * h] }
    }

    /// Canvas at SS x the final size.
    fn canvas(w: usize, h: usize) -> Img {
        Img::new(w * SS, h * SS)
    }

    fn from_fn(w: usize, h: usize, f: impl Fn(usize, usize) -> [f32; 4]) -> Img {
        let mut img = Img::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.px[y * w + x] = f(x, y);
            }
        }
        img
    }

    fn rgba8(&self) -> Vec<u8> {
        self.px.iter().flat_map(|p| p.map(|c| c.clamp(0.0, 255.0).round() as u8)).collect()
    }

    /// Fill pixels (x, y) with x0 <= x <= x1, y0 <= y <= y1 for which `inside` holds.
    fn fill(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: [f32; 4], inside: impl Fn(f32, f32) -> bool) {
        let (xa, xb) = (x0.floor().max(0.0) as isize, (x1.ceil() as isize).min(self.w as isize - 1));
        let (ya, yb) = (y0.floor().max(0.0) as isize, (y1.ceil() as isize).min(self.h as isize - 1));
        for y in ya..=yb {
            for x in xa..=xb {
                if inside(x as f32, y as f32) {
                    self.px[y as usize * self.w + x as usize] = c;
                }
            }
        }
    }

    /// PIL rectangle: inclusive corners (coordinates truncated, as PIL does).
    fn rect(&mut self, b: [f32; 4], c: [f32; 4]) {
        let [x0, y0, x1, y1] = b.map(f32::trunc);
        self.fill(x0, y0, x1, y1, c, |x, y| x >= x0 && x <= x1 && y >= y0 && y <= y1);
    }

    /// PIL rectangle outline: `width` pixels inward.
    fn rect_outline(&mut self, b: [f32; 4], c: [f32; 4], width: f32) {
        let [x0, y0, x1, y1] = b.map(f32::trunc);
        self.fill(x0, y0, x1, y1, c, |x, y| {
            x >= x0 && x <= x1 && y >= y0 && y <= y1 && (x < x0 + width || x > x1 - width || y < y0 + width || y > y1 - width)
        });
    }

    /// PIL wide line, horizontal or vertical.
    fn line(&mut self, (x0, y0): (f32, f32), (x1, y1): (f32, f32), c: [f32; 4], width: usize) {
        let (x0, y0, x1, y1) = (x0.trunc(), y0.trunc(), x1.trunc(), y1.trunc());
        if width <= 1 {
            self.rect([x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)], c);
            return;
        }
        // as ImagingDrawWideLine: a polygon around the segment
        let (dx, dy) = (x1 - x0, y1 - y0);
        let big = dx.hypot(dy);
        let small = (width as f32 - 1.0) / 2.0;
        let round_up = |f: f32| if f >= 0.0 { (f + 0.5).floor() } else { -(f.abs() + 0.5).floor() };
        let round_down = |f: f32| if f >= 0.0 { (f - 0.5).ceil() } else { -(f.abs() - 0.5).ceil() };
        let (rmax, rmin) = (round_up(small) / big, round_down(small) / big);
        let (dxmin, dxmax) = (round_down(rmin * dy), round_down(rmax * dy));
        let (dymin, dymax) = (round_down(rmin * dx), round_down(rmax * dx));
        let xs = [x0 - dxmin, x1 - dxmin, x1 + dxmax, x0 + dxmax];
        let ys = [y0 + dymax, y1 + dymax, y1 - dymin, y0 - dymin];
        let fold = |v: [f32; 4], f: fn(f32, f32) -> f32| v.into_iter().reduce(f).unwrap();
        self.rect([fold(xs, f32::min), fold(ys, f32::min), fold(xs, f32::max), fold(ys, f32::max)], c);
    }

    /// Filled polygon (even-odd, sampled at pixel centres).
    fn polygon(&mut self, pts: &[(f32, f32)], c: [f32; 4]) {
        let x0 = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
        let x1 = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
        let y0 = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        let y1 = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
        self.fill(x0, y0, x1, y1, c, |x, y| {
            let (px, py) = (x + 0.5, y + 0.5);
            let mut inside = false;
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                if (a.1 > py) != (b.1 > py) && px < a.0 + (py - a.1) * (b.0 - a.0) / (b.1 - a.1) {
                    inside = !inside;
                }
            }
            inside
        });
    }

    fn rounded_rect(&mut self, [x0, y0, x1, y1]: [f32; 4], r: f32, c: [f32; 4]) {
        let r = r.min((x1 + 1.0 - x0) / 2.0).min((y1 + 1.0 - y0) / 2.0);
        self.fill(x0, y0, x1, y1, c, |x, y| {
            let (px, py) = (x + 0.5, y + 0.5);
            if px < x0 || px > x1 + 1.0 || py < y0 || py > y1 + 1.0 {
                return false;
            }
            let cx = px.clamp(x0 + r, x1 + 1.0 - r);
            let cy = py.clamp(y0 + r, y1 + 1.0 - r);
            (px - cx).hypot(py - cy) <= r
        });
    }

    /// Ellipse inside the inclusive box: distance-to-edge test in units of its radii.
    fn ellipse_ring(&mut self, [x0, y0, x1, y1]: [f32; 4], c: [f32; 4], width: Option<f32>, top_half: bool) {
        let (cx, cy) = ((x0 + x1 + 1.0) / 2.0, (y0 + y1 + 1.0) / 2.0);
        let (a, b) = ((x1 - x0 + 1.0) / 2.0, (y1 - y0 + 1.0) / 2.0);
        let inside = |px: f32, py: f32, a: f32, b: f32| ((px - cx) / a).powi(2) + ((py - cy) / b).powi(2) <= 1.0;
        self.fill(x0, y0, x1, y1, c, |x, y| {
            let (px, py) = (x + 0.5, y + 0.5);
            if top_half && py > cy {
                return false;
            }
            inside(px, py, a, b) && width.is_none_or(|w| !inside(px, py, a - w, b - w))
        });
    }

    fn ellipse(&mut self, b: [f32; 4], fill: [f32; 4], outline: Option<([f32; 4], f32)>) {
        self.ellipse_ring(b, fill, None, false);
        if let Some((c, w)) = outline {
            self.ellipse_ring(b, c, Some(w), false);
        }
    }

    /// PIL GaussianBlur: three box blurs per direction, edges extended.
    fn blur(&self, radius: f32) -> Img {
        let sigma2 = radius * radius / 3.0;
        let l_full = (12.0 * sigma2 + 1.0).sqrt();
        let l = ((l_full - 1.0) / 2.0).floor();
        let a = (2.0 * l + 1.0) * (l * (l + 1.0) - 3.0 * sigma2) / (6.0 * (sigma2 - (l + 1.0) * (l + 1.0)));
        let (l, a) = (l as isize, a);
        let mut out = self.clone();
        let mut line = Vec::new();
        let mut sums = Vec::new();
        let mut pass = |px: &mut Vec<[f32; 4]>, n: usize, count: usize, at: &dyn Fn(usize, usize) -> usize| {
            for k in 0..count {
                for _ in 0..3 {
                    line.clear();
                    line.extend((0..n).map(|i| px[at(k, i)]));
                    sums.clear();
                    sums.push([0.0f64; 4]);
                    for p in &line {
                        let last = *sums.last().unwrap();
                        sums.push([0, 1, 2, 3].map(|c| last[c] + p[c] as f64));
                    }
                    let get = |i: isize| line[i.clamp(0, n as isize - 1) as usize];
                    // sum of the samples from..=to, edges extended
                    let range = |from: isize, to: isize| -> [f64; 4] {
                        let last = n as isize - 1;
                        let (lo, hi) = (from.max(0), to.min(last));
                        let below = (to.min(-1) - from + 1).max(0) as f64;
                        let above = (to - from.max(last + 1) + 1).max(0) as f64;
                        [0, 1, 2, 3].map(|c| {
                            let inner = if lo <= hi { sums[hi as usize + 1][c] - sums[lo as usize][c] } else { 0.0 };
                            inner + below * line[0][c] as f64 + above * line[n - 1][c] as f64
                        })
                    };
                    let norm = (2 * l + 1) as f64 + 2.0 * a as f64;
                    for i in 0..n {
                        let i2 = i as isize;
                        let s = range(i2 - l, i2 + l);
                        let (e1, e2) = (get(i2 - l - 1), get(i2 + l + 1));
                        px[at(k, i)] = [0, 1, 2, 3].map(|c| ((s[c] + a as f64 * (e1[c] + e2[c]) as f64) / norm) as f32);
                    }
                }
            }
        };
        let (w, h) = (self.w, self.h);
        pass(&mut out.px, w, h, &|y, x| y * w + x);
        pass(&mut out.px, h, w, &|x, y| y * w + x);
        out
    }

    /// Alpha-composite `top` over this image.
    fn composite(&mut self, top: &Img) {
        for (d, s) in self.px.iter_mut().zip(&top.px) {
            let (sa, da) = (s[3] / 255.0, d[3] / 255.0);
            let oa = sa + da * (1.0 - sa);
            if oa > 0.0 {
                for c in 0..3 {
                    d[c] = (s[c] * sa + d[c] * da * (1.0 - sa)) / oa;
                }
            }
            d[3] = oa * 255.0;
        }
    }

    /// Lanczos resize (as PIL: premultiplied alpha, one direction at a time).
    fn resize(&self, w: usize, h: usize) -> Img {
        let pre: Vec<[f32; 4]> = self.px.iter().map(|p| [p[0] * p[3] / 255.0, p[1] * p[3] / 255.0, p[2] * p[3] / 255.0, p[3]]).collect();
        let horiz = resample(&pre, self.w, self.h, w, true, lanczos, 3.0);
        let both = resample(&horiz, w, self.h, h, false, lanczos, 3.0);
        let px = both
            .into_iter()
            .map(|p| {
                let a = p[3].clamp(0.0, 255.0);
                if a <= 0.0 {
                    [0.0; 4]
                } else {
                    [p[0] * 255.0 / a, p[1] * 255.0 / a, p[2] * 255.0 / a, a]
                }
            })
            .collect();
        Img { w, h, px }
    }

    fn finish(&self, w: usize, h: usize) -> Img {
        self.resize(w, h)
    }

    fn map(mut self, f: impl Fn(usize, usize, &mut [f32; 4])) -> Img {
        for y in 0..self.h {
            for x in 0..self.w {
                f(x, y, &mut self.px[y * self.w + x]);
            }
        }
        self
    }
}

fn sinc(x: f32) -> f32 {
    if x == 0.0 {
        1.0
    } else {
        let x = x * std::f32::consts::PI;
        x.sin() / x
    }
}

fn lanczos(x: f32) -> f32 {
    if x.abs() < 3.0 {
        sinc(x) * sinc(x / 3.0)
    } else {
        0.0
    }
}

fn bicubic(x: f32) -> f32 {
    let (a, x) = (-0.5, x.abs());
    if x < 1.0 {
        ((a + 2.0) * x - (a + 3.0)) * x * x + 1.0
    } else if x < 2.0 {
        (((x - 5.0) * x + 8.0) * x - 4.0) * a
    } else {
        0.0
    }
}

/// Resample along one axis (PIL's precomputed-coefficient scheme).
fn resample<const N: usize>(src: &[[f32; N]], sw: usize, sh: usize, out: usize, horizontal: bool, kernel: fn(f32) -> f32, support: f32) -> Vec<[f32; N]> {
    let n_in = if horizontal { sw } else { sh };
    let scale = n_in as f32 / out as f32;
    let fscale = scale.max(1.0);
    let support = support * fscale;
    let coeffs: Vec<(usize, Vec<f32>)> = (0..out)
        .map(|i| {
            let center = (i as f32 + 0.5) * scale;
            let lo = ((center - support + 0.5) as isize).max(0) as usize;
            let hi = ((center + support + 0.5) as isize).min(n_in as isize) as usize;
            let mut w: Vec<f32> = (lo..hi).map(|j| kernel((j as f32 - center + 0.5) / fscale)).collect();
            let total: f32 = w.iter().sum();
            if total != 0.0 {
                w.iter_mut().for_each(|v| *v /= total);
            }
            (lo, w)
        })
        .collect();
    let (ow, oh) = if horizontal { (out, sh) } else { (sw, out) };
    let mut dst = vec![[0.0; N]; ow * oh];
    for y in 0..oh {
        for x in 0..ow {
            let (lo, w) = &coeffs[if horizontal { x } else { y }];
            let mut acc = [0.0; N];
            for (k, wk) in w.iter().enumerate() {
                let s = if horizontal { src[y * sw + lo + k] } else { src[(lo + k) * sw + x] };
                for c in 0..N {
                    acc[c] += s[c] * wk;
                }
            }
            dst[y * ow + x] = acc;
        }
    }
    dst
}

/// Small deterministic random numbers in [0, 1).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Random grid `gw` x `gh`, smoothly enlarged to `w` x `h` (bicubic).
fn noise_layer(rng: &mut Rng, gw: usize, gh: usize, w: usize, h: usize) -> Vec<f32> {
    let grid: Vec<[f32; 1]> = (0..gw * gh).map(|_| [rng.next()]).collect();
    let horiz = resample(&grid, gw, gh, w, true, bicubic, 2.0);
    resample(&horiz, w, gh, h, false, bicubic, 2.0).into_iter().map(|[v]| v.clamp(0.0, 1.0)).collect()
}

/// Soft fibrous parchment noise in [0, 1].
fn grain(w: usize, h: usize, seed: u64) -> Vec<f32> {
    let mut rng = Rng(seed);
    let mut acc = vec![0.0f32; w * h];
    let (mut amp, mut total) = (1.0, 0.0);
    for octave in 0..5 {
        let cell = 64.0 / (1 << octave) as f32;
        let (gw, gh) = (((w as f32 / cell) as usize).max(2), ((h as f32 / cell) as usize).max(2));
        for (a, v) in acc.iter_mut().zip(noise_layer(&mut rng, gw, gh, w, h)) {
            *a += v * amp;
        }
        total += amp;
        amp *= 0.55;
    }
    // horizontal fibres
    let fibres = noise_layer(&mut rng, (w / 24).max(2), h, w, h);
    acc.iter().zip(fibres).map(|(a, f)| (a / total * 0.8 + f * 0.2).clamp(0.0, 1.0)).collect()
}

fn linspace(from: f32, to: f32, n: usize, i: usize) -> f32 {
    if n <= 1 {
        from
    } else {
        from + (to - from) * i as f32 / (n - 1) as f32
    }
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [0, 1, 2].map(|c| a[c] * (1.0 - t) + b[c] * t)
}

// ---------------------------------------------------------------- frames & windows

/// Short L-shaped brackets at the four corners of the box.
fn corner_ticks(img: &mut Img, [x0, y0, x1, y1]: [f32; 4], length: f32, width: usize, c: [f32; 4]) {
    for (cx, cy, dx, dy) in [(x0, y0, 1.0, 1.0), (x1, y0, -1.0, 1.0), (x0, y1, 1.0, -1.0), (x1, y1, -1.0, -1.0)] {
        img.line((cx, cy), (cx + dx * length, cy), c, width);
        img.line((cx, cy), (cx, cy + dy * length), c, width);
    }
}

/// 9-slice window frame: one thin light rule, brighter corner brackets.
fn inside_frame(size: usize) -> Img {
    let s = (size * SS) as f32;
    let mut img = Img::canvas(size, size);
    let o = (8 * SS) as f32;
    img.rect_outline([o, o, s - o, s - o], rgba(LINE, 150.0), (1.4 * SS as f32).floor());
    let t = (2 * SS) as f32;
    corner_ticks(&mut img, [o - t, o - t, s - o + t, s - o + t], (14 * SS) as f32, 2 * SS, rgba(LINE_BRIGHT, 235.0));
    img.finish(size, size)
}

/// Near-white fill that the game tints with its window gradient; a soft darkening toward
/// the edges gives the panel depth once multiplied by the tint.
fn window_fill(size: usize, rounded: f32) -> Img {
    let n = (size - 1) as f32;
    Img::from_fn(size, size, |x, y| {
        let (xx, yy) = (x as f32 / n, y as f32 / n);
        let edge = xx.min(1.0 - xx).min(yy.min(1.0 - yy));
        let lum = 0.86 + 0.14 * (edge / 0.22).clamp(0.0, 1.0).powf(0.6);
        let alpha = if rounded > 0.0 {
            let r = rounded;
            let cx = (r - xx * n).max(xx * n - (n - r)).max(0.0);
            let cy = (r - yy * n).max(yy * n - (n - r)).max(0.0);
            (r - cx.hypot(cy) + 0.5).clamp(0.0, 1.0)
        } else {
            (edge * n + 0.5).clamp(0.0, 1.0)
        };
        [lum * 255.0, lum * 255.0, lum * 255.0, alpha * 255.0]
    })
}

/// Soft backing for the dialogue window: neutral charcoal.
fn talk_window_bg(size: usize) -> Img {
    let g = grain(size, size, 11);
    let n = (size - 1) as f32;
    Img::from_fn(size, size, |x, y| {
        let (xx, yy) = (x as f32 / n, y as f32 / n);
        let edge = xx.min(1.0 - xx).min(yy.min(1.0 - yy));
        let a = ((edge - 0.06) / 0.16).clamp(0.0, 1.0).powf(1.4);
        let col = mix(INK_LIGHT, INK, yy).map(|c| c * (0.96 + 0.08 * g[y * size + x]));
        rgba(col, a * 0.92 * 255.0)
    })
}

fn balloon(w: usize, h: usize) -> Img {
    let g = grain(w, h, 23);
    let mut img = Img::from_fn(w, h, |x, y| {
        let col = mix(INK_LIGHT, INK, linspace(0.0, 1.0, h, y)).map(|c| c * (0.96 + 0.08 * g[y * w + x]));
        let border = x < 2 || y < 2 || x >= w - 2 || y >= h - 2;
        rgba(col, if border { 0.0 } else { 0.88 * 255.0 })
    });
    img.rect_outline([2.0, 2.0, (w - 3) as f32, (h - 3) as f32], rgba(LINE_DIM, 200.0), 1.0);
    img
}

// ---------------------------------------------------------------- selection bars & plates

/// Selected row: lighter grey body, thin white outline and brighter corner brackets.
/// `gold` keeps the game's special highlight distinguishable.
fn select_bar(w: usize, h: usize, gold: bool) -> Img {
    let (sw, sh) = (w * SS, h * SS);
    let base = if gold { [150.0, 140.0, 110.0] } else { SELECT };
    let mut img = Img::from_fn(sw, sh, |_, y| {
        let yy = linspace(0.0, 1.0, sh, y);
        rgba(base.map(|c| c * (1.1 - 0.25 * yy)), (0.55 - 0.15 * yy) * 255.0)
    });
    let m = SS as f32;
    let b = [m, m, sw as f32 - m - 1.0, sh as f32 - m - 1.0];
    img.rect_outline(b, rgba(LINE_BRIGHT, 200.0), SS as f32);
    corner_ticks(&mut img, b, (6 * SS) as f32, 2 * SS, rgba(LINE_BRIGHT, 255.0));
    img.finish(w, h)
}

/// Dialog choice rows: centred band. Default = faint ink band; selected = light band.
fn dialog_row(w: usize, h: usize, selected: bool) -> Img {
    let (sw, sh) = (w * SS, h * SS);
    let band = |x: usize| 1.0 - linspace(-1.0, 1.0, sw, x).abs().powi(3);
    let yy = |y: usize| linspace(-1.0, 1.0, sh, y);
    let mut img = Img::from_fn(sw, sh, |x, y| {
        let yy = yy(y);
        if selected {
            let v = 1.0 - yy * yy;
            let col = [0, 1, 2].map(|c| SELECT[c] * (1.0 - v * 0.4) + LINE_BRIGHT[c] * v * 0.4);
            rgba(col, band(x) * (0.30 + 0.45 * v) * 255.0)
        } else {
            rgba(INK_LIGHT, band(x) * 0.55 * 255.0 * (1.0 - yy.abs().powi(6)))
        }
    });
    let (right, bottom) = (sw as f32, (sh - 2 * SS) as f32);
    if selected {
        img.line((0.0, SS as f32), (right, SS as f32), rgba(LINE_BRIGHT, 230.0), SS);
        img.line((0.0, bottom), (right, bottom), rgba(LINE, 230.0), SS);
    } else {
        img.line((0.0, bottom), (right, bottom), rgba(LINE_DIM, 160.0), SS);
    }
    img.map(|x, y, p| {
        if yy(y).abs() > 0.8 {
            p[3] *= band(x);
        }
    })
    .finish(w, h)
}

enum Plate {
    Idle,
    Hold,
    Select,
}

/// Menu list plates.
fn plate(w: usize, h: usize, notch: usize, state: Plate) -> Img {
    let mut img = Img::canvas(w, h);
    let inset = (1.5 * SS as f32).floor();
    let b = [inset, inset, (w * SS) as f32 - inset - 1.0, (h * SS) as f32 - inset - 1.0];
    match state {
        Plate::Select => {
            img.rect(b, rgba(SELECT, 170.0));
            img.rect_outline(b, rgba(LINE_BRIGHT, 210.0), SS as f32);
            corner_ticks(&mut img, b, (notch * SS) as f32, 2 * SS, rgba(LINE_BRIGHT, 255.0));
        }
        Plate::Hold => {
            img.rect(b, rgba(INK_LIGHT, 170.0));
            img.rect_outline(b, rgba(LINE_DIM, 220.0), SS as f32);
        }
        Plate::Idle => {
            img.rect(b, rgba(INK_LIGHT, 140.0));
            img.line((b[0], b[3]), (b[2], b[3]), rgba(LINE_DIM, 150.0), SS);
        }
    }
    img.finish(w, h)
}

/// Party member card backing: arched top, charcoal.
fn chara_list_bg(w: usize, h: usize) -> Img {
    let g = grain(w, h, 31);
    let (sw, sh) = (w * SS, h * SS);
    let r = (sw / 2) as f32;
    let mut mask = Img::new(sw, sh);
    let white = [255.0; 4];
    mask.rect([0.0, r, sw as f32, sh as f32], white);
    mask.ellipse([0.0, 0.0, sw as f32 - 1.0, 2.0 * r], white, None);
    let mask = mask.resize(w, h);
    Img::from_fn(w, h, |x, y| {
        let col = mix(INK_LIGHT, INK, linspace(0.0, 1.0, h, y)).map(|c| c * (0.96 + 0.08 * g[y * w + x]));
        rgba(col, mask.px[y * w + x][3] / 255.0 * 0.62 * 255.0)
    })
}

/// Arched hairline frame (the arch follows the card backing's shape).
fn chara_list_frame(w: usize, h: usize) -> Img {
    let (sw, sh) = ((w * SS) as f32, (h * SS) as f32);
    let mut img = Img::canvas(w, h);
    let inset = (3 * SS) as f32;
    let r = (w * SS / 2) as f32 - inset;
    let c = rgba(LINE, 170.0);
    let width = (1.4 * SS as f32) as usize;
    img.ellipse_ring([inset, inset, sw - inset - 1.0, inset + 2.0 * r], c, Some(width as f32), true);
    img.line((inset, inset + r), (inset, sh - inset), c, width);
    img.line((sw - inset - 1.0, inset + r), (sw - inset - 1.0, sh - inset), c, width);
    img.line((inset, sh - inset - 1.0), (sw - inset, sh - inset - 1.0), c, width);
    img.finish(w, h)
}

// ---------------------------------------------------------------- lines, glows, cursor

#[derive(PartialEq)]
enum Fade {
    Both,
    Left,
}

/// Light horizontal rule centred vertically, fading at one or both ends.
fn rule(w: usize, h: usize, fade: Fade, thickness: f32, glow_r: usize, ornament: bool) -> Img {
    let (sw, sh) = (w * SS, h * SS);
    let mut img = Img::canvas(w, h);
    let cy = (sh / 2) as f32;
    let t = SS.max((thickness * SS as f32) as usize);
    img.line((0.0, cy), (sw as f32, cy), rgba(LINE_BRIGHT, 255.0), t);
    if glow_r > 0 {
        // a soft glow of the line, in the dimmer line colour
        let blurred = img.blur((glow_r * SS) as f32);
        for (p, b) in img.px.iter_mut().zip(&blurred.px) {
            let a = (p[3] + b[3] * 0.9).clamp(0.0, 255.0);
            *p = if a > 0.0 { rgba(LINE, a * 0.6) } else { [0.0; 4] };
        }
        img.line((0.0, cy), (sw as f32, cy), rgba(LINE_BRIGHT, 255.0), t);
    }
    let mut img = img.map(|x, _, p| {
        let xx = linspace(0.0, 1.0, sw, x);
        p[3] *= match fade {
            Fade::Both => (1.0 - (xx - 0.5).abs() * 2.0).clamp(0.0, 1.0),
            Fade::Left => (1.0 - xx).clamp(0.0, 1.0).powf(1.2),
        };
    });
    if ornament {
        // a small diamond at the centre: white, with a grey keyline
        let (cx, r, keyline) = ((sw / 2) as f32, (5 * SS) as f32, SS as f32);
        for y in (cy - r) as usize..=(cy + r) as usize {
            for x in (cx - r) as usize..=(cx + r) as usize {
                let d = (r - ((x as f32 - cx).abs() + (y as f32 - cy).abs())) / std::f32::consts::SQRT_2;
                if d >= 0.0 {
                    img.px[y * sw + x] = if d < keyline { rgba(LINE_DIM, 255.0) } else { rgba(LINE_BRIGHT, 255.0) };
                }
            }
        }
    }
    img.finish(w, h)
}

fn radial_glow(size: usize, color: Rgb, falloff: f32, alpha: f32) -> Img {
    let c = (size - 1) as f32 / 2.0;
    Img::from_fn(size, size, |x, y| {
        let r = (x as f32 - c).hypot(y as f32 - c) / c;
        rgba(color, (1.0 - r).clamp(0.0, 1.0).powf(falloff) * alpha * 255.0)
    })
}

/// Square focus frame (icon grid selection): thin white outline with corner brackets.
fn select_frame(size: usize) -> Img {
    let s = (size * SS) as f32;
    let mut img = Img::canvas(size, size);
    let (m, t) = ((12 * SS) as f32, (2 * SS) as f32);
    img.rect_outline([m, m, s - m, s - m], rgba(LINE_BRIGHT, 200.0), SS as f32);
    corner_ticks(&mut img, [m - t, m - t, s - m + t, s - m + t], (9 * SS) as f32, 2 * SS, rgba(LINE_BRIGHT, 255.0));
    img.finish(size, size)
}

/// White gloved hand pointing right, with a dark keyline and soft drop shadow.
fn cursor(size: usize) -> Img {
    let s = size * SS;
    let u = s as f32 / 64.0; // design units on a 64-grid
    // rounded rectangles (x0, y0, x1, y1, radius): palm, index finger, curled middle and
    // ring fingers, cuff; and the thumb over the knuckles
    const RRECTS: [[f32; 5]; 5] = [
        [12.0, 24.0, 36.0, 46.0, 7.0],
        [28.0, 24.0, 58.0, 32.0, 4.0],
        [28.0, 31.0, 42.0, 38.0, 3.5],
        [27.0, 37.0, 40.0, 43.0, 3.5],
        [5.0, 26.0, 15.0, 44.0, 2.5],
    ];
    const THUMB: [(f32, f32); 5] = [(16.0, 26.0), (24.0, 18.0), (31.0, 18.0), (31.0, 25.0), (22.0, 29.0)];
    let draw = |img: &mut Img, c: [f32; 4], grow: f32, (ox, oy): (f32, f32)| {
        // in the order of the original: palm, finger, middle, ring, thumb, cuff
        for (i, [x0, y0, x1, y1, r]) in RRECTS.iter().enumerate() {
            if i == 4 {
                let pts: Vec<(f32, f32)> = THUMB.iter().map(|&(x, y)| ((x + ox) * u, (y + oy) * u)).collect();
                let n = pts.len() as f32;
                let (cx, cy) = (pts.iter().map(|p| p.0).sum::<f32>() / n, pts.iter().map(|p| p.1).sum::<f32>() / n);
                let k = 1.0 + grow * 0.12;
                let pts: Vec<(f32, f32)> = pts.iter().map(|&(x, y)| (cx + (x - cx) * k, cy + (y - cy) * k)).collect();
                img.polygon(&pts, c);
            }
            let b = [(x0 - grow + ox) * u, (y0 - grow + oy) * u, (x1 + grow + ox) * u, (y1 + grow + oy) * u];
            img.rounded_rect(b.map(f32::round), (r + grow) * u, c);
        }
    };
    let mut img = Img::canvas(size, size);
    let mut shadow = Img::new(s, s);
    draw(&mut shadow, [0.0, 0.0, 0.0, 140.0], 1.4, (1.5, 2.0));
    img.composite(&shadow.blur((2 * SS) as f32));
    draw(&mut img, [36.0, 36.0, 42.0, 255.0], 1.4, (0.0, 0.0)); // keyline
    draw(&mut img, rgba(LINE_BRIGHT, 255.0), 0.0, (0.0, 0.0));
    // finger separations and cuff seam
    let seam = [150.0, 150.0, 160.0, 255.0];
    let width = 1.max((0.9 * u) as usize);
    for y in [31.2, 37.2] {
        img.line((29.0 * u, y * u), (38.0 * u, y * u), seam, width);
    }
    img.line((15.0 * u, 26.0 * u), (15.0 * u, 44.0 * u), seam, width);
    img.finish(size, size)
}

// ---------------------------------------------------------------- battle

/// Party status panel as an Octopath II style strip: dark, fading out toward the left, no
/// box or outline. The acting member's strip is lighter and more opaque.
fn battle_panel(size: usize, active: bool) -> Img {
    let n = (size - 1) as f32;
    let base = if active { [70.0, 71.0, 78.0] } else { INK };
    Img::from_fn(size, size, |x, y| {
        let (xx, yy) = (x as f32 / n, y as f32 / n);
        let mut a = (xx * 1.25).clamp(0.0, 1.0).powf(if active { 1.0 } else { 1.3 }) * if active { 0.80 } else { 0.62 };
        if active {
            a = a.max(0.30); // the active strip never fully fades out
        }
        a *= (1.0 - (yy - 0.5).abs() * 0.4).clamp(0.0, 1.0);
        rgba(base, a * 255.0)
    })
}

fn ink_band(w: usize, h: usize) -> Img {
    Img::from_fn(w, h, |x, y| {
        let (xx, yy) = (linspace(-1.0, 1.0, w, x), linspace(-1.0, 1.0, h, y));
        rgba(INK_LIGHT, (1.0 - xx.abs().powf(2.5)) * (1.0 - yy.abs().powi(4)) * 0.75 * 255.0)
    })
}

fn key_disc(size: usize) -> Img {
    let s = size * SS;
    let mut img = Img::canvas(size, size);
    let (r, c) = ((s as f32 * 0.30).floor(), (s / 2) as f32);
    img.ellipse([c - r, c - r, c + r, c + r], rgba(INK, 225.0), Some((rgba(LINE_BRIGHT, 255.0), (2.2 * SS as f32).floor())));
    let i = (5 * SS) as f32;
    img.ellipse_ring([c - r + i, c - r + i, c + r - i, c + r - i], rgba(LINE_DIM, 200.0), Some(SS as f32), false);
    img.finish(size, size)
}
