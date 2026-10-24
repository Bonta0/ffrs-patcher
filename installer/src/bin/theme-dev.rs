//! Development tool for the theme.
//!
//! theme-dev schema <FFRS.usmap> <out schema.txt>
//!     write the types the theme edits (src/schema.rs) from a type dump of the game,
//!     made with UE4SS (DumpUSMAP)
//! theme-dev preview <game Content\Paks> <out dir>
//!     build the theme as the patcher does and write its files, to inspect

#[allow(dead_code)]
#[path = "../art.rs"]
mod art;
#[path = "../edits.rs"]
mod edits;
#[allow(dead_code)]
#[path = "../fonts.rs"]
mod fonts;
#[path = "../package.rs"]
mod package;
#[allow(dead_code)]
#[path = "../props.rs"]
mod props;
#[allow(dead_code)]
#[path = "../schema.rs"]
mod schema;
#[path = "../theme.rs"]
mod theme;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};
use schema::Type;

/// Classes and table row types whose objects the theme edits.
const ROOTS: &[&str] = &[
    "Button",
    "CPP_DA_HorizontalGradient",
    "CPP_MuchaScrollBox",
    "CPP_UI_Cmn_ListView",
    "CPP_UI_Cmn_TileView",
    "CmnWindow_BorderStyle",
    "CommonColor",
    "CommonLine",
    "CommonTextBlock",
    "CompositeDataTable",
    "DataTable",
    "Image",
    "LeftLineWindowStyle",
    "MaterialInstanceConstant",
    "MovieScene2DTransformSection",
    "OverlaySlot",
    "SizeBox",
    "Slider",
    "TextBlock",
    "WBP_DirectionLayoutMenu_C",
];

struct Usmap {
    names: Vec<String>,
    enums: BTreeMap<String, Vec<(String, i64)>>,
    structs: BTreeMap<String, schema::Struct>,
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        if self.0.len() < n {
            bail!("usmap truncated");
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into()?))
    }
}

fn read_usmap(path: &Path) -> Result<Usmap> {
    let mut data = Vec::new();
    std::fs::File::open(path)?.read_to_end(&mut data)?;
    let mut r = Reader(&data);
    if r.u16()? != 0x30C4 {
        bail!("not a .usmap file");
    }
    let version = r.u8()?;
    if version != 4 {
        bail!("usmap version {version} (expected 4)");
    }
    if r.u32()? != 0 {
        bail!("usmap with package versioning is not supported");
    }
    if r.u8()? != 0 {
        bail!("compressed usmap is not supported");
    }
    let (_, size) = (r.u32()?, r.u32()? as usize);
    let mut r = Reader(r.take(size)?);
    let mut names = Vec::new();
    for _ in 0..r.u32()? {
        let n = r.u16()? as usize;
        names.push(String::from_utf8(r.take(n)?.to_vec())?);
    }
    let name = |r: &mut Reader, names: &Vec<String>| -> Result<String> { Ok(names[r.u32()? as usize].clone()) };
    let mut enums = BTreeMap::new();
    for _ in 0..r.u32()? {
        let enum_name = name(&mut r, &names)?;
        let mut values = Vec::new();
        for _ in 0..r.u16()? {
            let v = r.i64()?;
            values.push((name(&mut r, &names)?, v));
        }
        enums.insert(enum_name, values);
    }
    fn ty(r: &mut Reader, names: &Vec<String>) -> Result<Type> {
        let name = |r: &mut Reader| -> Result<String> { Ok(names[r.u32()? as usize].clone()) };
        Ok(match r.u8()? {
            0 => Type::Byte,
            1 => Type::Bool,
            2 => Type::Int,
            3 => Type::Float,
            4 => Type::Object,
            5 => Type::Name,
            6 => Type::Delegate,
            7 => Type::Double,
            8 => Type::Array(Box::new(ty(r, names)?)),
            9 => Type::Struct(name(r)?),
            10 => Type::Str,
            11 => Type::Text,
            12 => Type::Interface,
            13 => Type::MulticastDelegate,
            14 => Type::WeakObject,
            15 => Type::LazyObject,
            16 | 17 => Type::SoftObject,
            18 => Type::UInt64,
            19 => Type::UInt32,
            20 => Type::UInt16,
            21 => Type::Int64,
            22 => Type::Int16,
            23 => Type::Int8,
            24 => {
                let k = ty(r, names)?;
                Type::Map(Box::new(k), Box::new(ty(r, names)?))
            }
            25 => Type::Set(Box::new(ty(r, names)?)),
            26 => {
                let inner = ty(r, names)?;
                Type::Enum(name(r)?, Box::new(inner))
            }
            27 => Type::FieldPath,
            28 => Type::Optional(Box::new(ty(r, names)?)),
            29 => Type::Utf8Str,
            30 => Type::AnsiStr,
            other => bail!("unknown usmap property type {other}"),
        })
    }
    let mut structs = BTreeMap::new();
    for _ in 0..r.u32()? {
        let struct_name = name(&mut r, &names)?;
        let super_ = match r.u32()? {
            u32::MAX => None,
            i => Some(names[i as usize].clone()),
        };
        let count = r.u16()? as usize;
        let mut props = Vec::new();
        for _ in 0..r.u16()? {
            let index = r.u16()? as usize;
            let dim = r.u8()? as usize;
            let prop_name = name(&mut r, &names)?;
            props.push(schema::Property { index, name: prop_name, dim, ty: ty(&mut r, &names)? });
        }
        structs.insert(struct_name, schema::Struct { super_, count, props });
    }
    Ok(Usmap { names, enums, structs })
}

fn write_schema(usmap: &Path, out: &Path) -> Result<()> {
    let map = read_usmap(usmap)?;
    let _ = &map.names;
    let mut structs = BTreeSet::new();
    let mut enums = BTreeSet::new();
    fn visit_type(t: &Type, map: &Usmap, structs: &mut BTreeSet<String>, enums: &mut BTreeSet<String>) -> Result<()> {
        match t {
            Type::Struct(n) => visit(n, map, structs, enums)?,
            Type::Enum(n, _) => {
                enums.insert(n.clone());
            }
            Type::Array(t) | Type::Set(t) | Type::Optional(t) => visit_type(t, map, structs, enums)?,
            Type::Map(k, v) => {
                visit_type(k, map, structs, enums)?;
                visit_type(v, map, structs, enums)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn visit(name: &str, map: &Usmap, structs: &mut BTreeSet<String>, enums: &mut BTreeSet<String>) -> Result<()> {
        if schema::native(name).is_some() || !structs.insert(name.to_string()) {
            return Ok(());
        }
        let s = map.structs.get(name).with_context(|| format!("{name} is not in the usmap"))?;
        if let Some(sup) = &s.super_ {
            visit(sup, map, structs, enums)?;
        }
        for p in &s.props {
            visit_type(&p.ty, map, structs, enums)?;
        }
        Ok(())
    }
    for root in ROOTS {
        visit(root, &map, &mut structs, &mut enums)?;
    }
    let mut text = String::from(
        "# Game types the theme edits (see schema.rs). Generated by `theme-dev schema` from a\n\
         # UE4SS type dump of FINAL FANTASY RESONANCE.\n",
    );
    for name in &structs {
        let s = &map.structs[name];
        match &s.super_ {
            Some(sup) => text += &format!("struct {name} : {sup} {}\n", s.count),
            None => text += &format!("struct {name} {}\n", s.count),
        }
        for p in &s.props {
            let dim = if p.dim > 1 { format!("[{}]", p.dim) } else { String::new() };
            text += &format!("  {} {}{dim} {}\n", p.index, p.name, p.ty);
        }
    }
    for name in &enums {
        let values = map.enums.get(name).with_context(|| format!("enum {name} is not in the usmap"))?;
        text += &format!("enum {name}");
        for (v, n) in values {
            text += &format!(" {v}={n}");
        }
        text += "\n";
    }
    schema::Schema::parse(&text)?;
    std::fs::write(out, text)?;
    println!("{} types, {} enums -> {}", structs.len(), enums.len(), out.display());
    Ok(())
}

/// Minimal PNG (RGBA, uncompressed deflate) for previews.
fn png(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = !0u32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend(kind);
        out.extend(data);
        let c = crc(&out[start..]);
        out.extend(c.to_be_bytes());
    }
    let mut raw = Vec::with_capacity((w * 4 + 1) * h);
    for row in rgba.chunks(w * 4) {
        raw.push(0);
        raw.extend(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push((i + 1 == blocks.len()) as u8);
        z.extend((block.len() as u16).to_le_bytes());
        z.extend((!(block.len() as u16)).to_le_bytes());
        z.extend(*block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend((w as u32).to_be_bytes());
    ihdr.extend((h as u32).to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn preview(paks: &Path, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out.join("textures"))?;
    for (name, w, h, rgba) in art::render_all() {
        std::fs::write(out.join("textures").join(format!("{name}.png")), png(w, h, &rgba))?;
    }
    let files = theme::build(paks)?;
    theme::pack(&files, &out.join("zFFRS_Patcher_P.utoc"))?;
    fonts::write_pak(&out.join("zFFRS_Patcher_P.pak"))?;
    for (path, data) in &files {
        let dst = out.join("files").join(path);
        std::fs::create_dir_all(dst.parent().unwrap())?;
        std::fs::write(dst, data)?;
    }
    println!("{} files -> {}", files.len(), out.display());
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["schema", usmap, out] => write_schema(Path::new(usmap), Path::new(out)),
        ["preview", paks, out] => preview(Path::new(paks), Path::new(out)),
        _ => bail!("usage: theme-dev schema <FFRS.usmap> <out schema.txt> | theme-dev preview <Content\\Paks> <out dir>"),
    }
}
