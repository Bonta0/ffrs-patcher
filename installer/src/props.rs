//! Reading and writing objects saved with unversioned properties (cooked UE5 assets):
//! a header telling which properties of the type are present, then their values in the
//! order of the type's schema (schema.rs).

use anyhow::{bail, Context, Result};

use crate::schema::{self, Schema, Type};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// a value kept as saved: numbers, names, object references, strings, text...
    Raw(Vec<u8>),
    /// a struct saved in its own binary form (schema::NATIVE), e.g. LinearColor
    Native(String, Vec<u8>),
    Struct(Struct),
    Array(Vec<Value>),
    Set { removed: Vec<Value>, items: Vec<Value> },
    Map { removed: Vec<Value>, entries: Vec<(Value, Value)> },
    Optional(Option<Box<Value>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    /// serialization index in the struct's schema
    pub index: usize,
    /// None: present but zero (no value saved)
    pub value: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Struct {
    pub ty: String,
    /// present properties, by index
    pub props: Vec<Property>,
}

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }
    pub fn pos(&self) -> usize {
        self.pos
    }
    pub fn rest(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.data.len() - self.pos < n {
            bail!("unexpected end of data at {}", self.pos);
        }
        self.pos += n;
        Ok(&self.data[self.pos - n..self.pos])
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into()?))
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into()?))
    }
    fn count(&mut self) -> Result<usize> {
        let n = self.i32()?;
        if n < 0 || n as usize > self.data.len() - self.pos {
            bail!("bad count {n} at {}", self.pos - 4);
        }
        Ok(n as usize)
    }
    /// FString: length (negative for UTF-16), characters with terminator.
    fn string(&mut self) -> Result<()> {
        let n = self.i32()?;
        let bytes = if n < 0 { (-(n as i64) * 2) as usize } else { n as usize };
        self.take(bytes)?;
        Ok(())
    }
}

/// Skip a value of a type kept as raw bytes, returning them.
fn read_raw<'a>(r: &mut Reader<'a>, ty: &Type) -> Result<&'a [u8]> {
    let start = r.pos;
    match ty {
        Type::Bool | Type::Int8 | Type::Byte => {
            r.take(1)?;
        }
        Type::Int16 | Type::UInt16 => {
            r.take(2)?;
        }
        Type::Int | Type::UInt32 | Type::Float | Type::Object | Type::WeakObject | Type::Interface => {
            r.take(4)?;
        }
        Type::Int64 | Type::UInt64 | Type::Double | Type::Name => {
            r.take(8)?;
        }
        Type::LazyObject => {
            r.take(16)?;
        }
        Type::Enum(_, inner) => {
            read_raw(r, inner)?;
        }
        Type::Str => r.string()?,
        Type::Utf8Str | Type::AnsiStr => {
            let n = r.count()?;
            r.take(n)?;
        }
        Type::SoftObject => soft_object_path(r)?,
        Type::Delegate => {
            r.take(12)?;
        }
        Type::MulticastDelegate => {
            let n = r.count()?;
            r.take(n * 12)?;
        }
        Type::FieldPath => {
            let n = r.count()?;
            r.take(n * 8 + 4)?;
        }
        Type::Text => text(r)?,
        other => bail!("{other} is not a raw type"),
    }
    Ok(&r.data[start..r.pos])
}

/// FSoftObjectPath: package name, asset name, sub-path string.
fn soft_object_path(r: &mut Reader) -> Result<()> {
    r.take(16)?;
    r.string()
}

/// FText: flags, history type and that history's data.
fn text(r: &mut Reader) -> Result<()> {
    r.take(4)?;
    let history = r.take(1)?[0] as i8;
    match history {
        -1 => {
            if r.i32()? != 0 {
                r.string()?;
            }
        }
        0 => {
            r.string()?; // namespace
            r.string()?; // key
            r.string()?; // source string
        }
        11 => {
            r.take(8)?; // string table
            r.string()?; // key
        }
        other => bail!("unsupported text history type {other}"),
    }
    Ok(())
}

fn read_native(r: &mut Reader, name: &str) -> Result<Value> {
    let start = r.pos;
    match schema::native(name).context("not native")? {
        Some(size) => {
            r.take(size)?;
        }
        None => match name {
            "SoftObjectPath" | "SoftClassPath" => soft_object_path(r)?,
            "GameplayTagContainer" => {
                let n = r.count()?;
                r.take(n * 8)?;
            }
            "MovieSceneFloatChannel" => {
                r.take(2)?; // extrapolation before / after
                for _ in 0..2 {
                    // times, then values: element size, count, elements
                    let size = r.count()?;
                    let n = r.count()?;
                    r.take(size * n)?;
                }
                r.take(4 + 4 + 8 + 4)?; // default value, has default, tick resolution, show curve
            }
            other => bail!("no reader for native struct {other}"),
        },
    }
    Ok(Value::Native(name.to_string(), r.data[start..r.pos].to_vec()))
}

fn read_value(r: &mut Reader, schema: &Schema, ty: &Type) -> Result<Value> {
    Ok(match ty {
        Type::Struct(name) => {
            if schema::native(name).is_some() {
                read_native(r, name)?
            } else {
                Value::Struct(read_struct(r, schema, name)?)
            }
        }
        Type::Array(inner) => {
            let n = r.count()?;
            Value::Array((0..n).map(|_| read_value(r, schema, inner)).collect::<Result<_>>()?)
        }
        Type::Set(inner) => {
            let removed = (0..r.count()?).map(|_| read_value(r, schema, inner)).collect::<Result<_>>()?;
            let items = (0..r.count()?).map(|_| read_value(r, schema, inner)).collect::<Result<_>>()?;
            Value::Set { removed, items }
        }
        Type::Map(k, v) => {
            let removed = (0..r.count()?).map(|_| read_value(r, schema, k)).collect::<Result<_>>()?;
            let entries = (0..r.count()?)
                .map(|_| Ok((read_value(r, schema, k)?, read_value(r, schema, v)?)))
                .collect::<Result<_>>()?;
            Value::Map { removed, entries }
        }
        Type::Optional(inner) => {
            let set = r.take(1)?[0] != 0;
            Value::Optional(if set { Some(Box::new(read_value(r, schema, inner)?)) } else { None })
        }
        raw => Value::Raw(read_raw(r, raw)?.to_vec()),
    })
}

const SKIP_MAX: usize = 127;
const VALUE_MAX: usize = 127;

/// Read the properties of a `ty` (header, then values).
pub fn read_struct(r: &mut Reader, schema: &Schema, ty: &str) -> Result<Struct> {
    // header: fragments of (skip, values, any zeroes, last), then the zero mask
    let mut fragments = Vec::new();
    loop {
        let f = r.u16()? as usize;
        let (skip, zeroes, last, values) = (f & 0x7f, f & 0x80 != 0, f & 0x100 != 0, f >> 9);
        fragments.push((skip, values, zeroes));
        if last {
            break;
        }
    }
    let masked: usize = fragments.iter().filter(|f| f.2).map(|f| f.1).sum();
    let mask: Vec<bool> = if masked == 0 {
        Vec::new()
    } else {
        let bytes = if masked <= 8 {
            1
        } else if masked <= 16 {
            2
        } else {
            masked.div_ceil(32) * 4
        };
        let raw = r.take(bytes)?;
        (0..masked).map(|i| raw[i / 8] & (1 << (i % 8)) != 0).collect()
    };
    let mut props = Vec::new();
    let (mut index, mut bit) = (0, 0);
    for (skip, values, zeroes) in fragments {
        index += skip;
        for _ in 0..values {
            let zero = zeroes && {
                bit += 1;
                mask[bit - 1]
            };
            let value = if zero {
                None
            } else {
                let (prop, _) = schema.property(ty, index)?;
                Some(read_value(r, schema, &prop.ty).with_context(|| format!("{ty}.{}", prop.name))?)
            };
            props.push(Property { index, value });
            index += 1;
        }
    }
    Ok(Struct { ty: ty.to_string(), props })
}

fn write_value(out: &mut Vec<u8>, schema: &Schema, value: &Value) -> Result<()> {
    match value {
        Value::Raw(b) | Value::Native(_, b) => out.extend(b),
        Value::Struct(s) => write_struct(out, schema, s)?,
        Value::Array(items) => {
            out.extend((items.len() as i32).to_le_bytes());
            for v in items {
                write_value(out, schema, v)?;
            }
        }
        Value::Set { removed, items } => {
            for list in [removed, items] {
                out.extend((list.len() as i32).to_le_bytes());
                for v in list {
                    write_value(out, schema, v)?;
                }
            }
        }
        Value::Map { removed, entries } => {
            out.extend((removed.len() as i32).to_le_bytes());
            for v in removed {
                write_value(out, schema, v)?;
            }
            out.extend((entries.len() as i32).to_le_bytes());
            for (k, v) in entries {
                write_value(out, schema, k)?;
                write_value(out, schema, v)?;
            }
        }
        Value::Optional(v) => {
            out.push(v.is_some() as u8);
            if let Some(v) = v {
                write_value(out, schema, v)?;
            }
        }
    }
    Ok(())
}

/// Write the properties of `s` (header, then values) the way the engine does.
pub fn write_struct(out: &mut Vec<u8>, schema: &Schema, s: &Struct) -> Result<()> {
    let mut props: Vec<&Property> = s.props.iter().collect();
    props.sort_by_key(|p| p.index);
    // (skip, values, any zeroes), with the zero flag of each value
    let mut fragments: Vec<(usize, usize, bool)> = Vec::new();
    let mut mask = Vec::new();
    if props.is_empty() {
        fragments.push((schema.total(&s.ty)?.min(SKIP_MAX), 0, false));
    } else {
        let mut next = 0;
        let mut i = 0;
        while i < props.len() {
            let mut skip = props[i].index - next;
            while skip > SKIP_MAX {
                fragments.push((SKIP_MAX, 0, false));
                skip -= SKIP_MAX;
            }
            let mut run = 1;
            while i + run < props.len() && props[i + run].index == props[i].index + run {
                run += 1;
            }
            for chunk in props[i..i + run].chunks(VALUE_MAX) {
                let zeroes: Vec<bool> = chunk.iter().map(|p| p.value.is_none()).collect();
                let any = zeroes.iter().any(|&z| z);
                if any {
                    mask.extend(zeroes);
                }
                fragments.push((skip, chunk.len(), any));
                skip = 0;
            }
            next = props[i].index + run;
            i += run;
        }
    }
    let last = fragments.len() - 1;
    for (n, (skip, values, zeroes)) in fragments.iter().enumerate() {
        let packed = *skip | if *zeroes { 0x80 } else { 0 } | if n == last { 0x100 } else { 0 } | values << 9;
        out.extend((packed as u16).to_le_bytes());
    }
    if !mask.is_empty() {
        let bytes = if mask.len() <= 8 {
            1
        } else if mask.len() <= 16 {
            2
        } else {
            mask.len().div_ceil(32) * 4
        };
        let mut raw = vec![0u8; bytes];
        for (i, &z) in mask.iter().enumerate() {
            if z {
                raw[i / 8] |= 1 << (i % 8);
            }
        }
        out.extend(raw);
    }
    for p in props {
        if let Some(v) = &p.value {
            write_value(out, schema, v)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- access for edits

impl Value {
    pub fn as_struct(&mut self) -> Result<&mut Struct> {
        match self {
            Value::Struct(s) => Ok(s),
            other => bail!("expected a struct, found {}", other.kind()),
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            Value::Raw(_) => "a value",
            Value::Native(..) => "a native struct",
            Value::Struct(_) => "a struct",
            Value::Array(_) => "an array",
            Value::Set { .. } => "a set",
            Value::Map { .. } => "a map",
            Value::Optional(_) => "an optional",
        }
    }
    pub fn float(v: f32) -> Value {
        Value::Raw(v.to_le_bytes().to_vec())
    }
    /// FName value: (index in the package's name list, number)
    pub fn name(&self) -> Result<(i32, i32)> {
        match self {
            Value::Raw(b) if b.len() == 8 => {
                Ok((i32::from_le_bytes(b[..4].try_into()?), i32::from_le_bytes(b[4..].try_into()?)))
            }
            other => bail!("expected a name, found {}", other.kind()),
        }
    }
    /// Object reference (package index).
    pub fn object(&self) -> Result<i32> {
        match self {
            Value::Raw(b) if b.len() == 4 => Ok(i32::from_le_bytes(b[..].try_into()?)),
            other => bail!("expected an object reference, found {}", other.kind()),
        }
    }

    /// Visit every LinearColor (linear RGBA) and Color (sRGB BGRA bytes) inside.
    pub fn colors(&mut self, f: &mut dyn FnMut(Color)) {
        match self {
            Value::Native(ty, b) if ty == "LinearColor" => f(Color::Linear(b)),
            Value::Native(ty, b) if ty == "Color" => f(Color::Srgb(b)),
            Value::Struct(s) => s.colors(f),
            Value::Array(items) | Value::Set { items, .. } => items.iter_mut().for_each(|v| v.colors(f)),
            Value::Map { entries, .. } => entries.iter_mut().for_each(|(k, v)| {
                k.colors(f);
                v.colors(f)
            }),
            Value::Optional(Some(v)) => v.colors(f),
            _ => {}
        }
    }
}

pub enum Color<'a> {
    Linear(&'a mut Vec<u8>),
    Srgb(&'a mut Vec<u8>),
}

impl Struct {
    /// The value of property `name` (element `element` of a static array).
    pub fn get(&mut self, schema: &Schema, name: &str, element: usize) -> Result<Option<&mut Value>> {
        let index = schema.index_of(&self.ty, name, element)?;
        Ok(self.props.iter_mut().find(|p| p.index == index).and_then(|p| p.value.as_mut()))
    }
    pub fn field(&mut self, schema: &Schema, name: &str) -> Result<&mut Value> {
        let ty = self.ty.clone();
        self.get(schema, name, 0)?.with_context(|| format!("{ty}.{name} is not set"))
    }
    /// Set property `name`, adding it if absent.
    pub fn set(&mut self, schema: &Schema, name: &str, element: usize, value: Value) -> Result<()> {
        let index = schema.index_of(&self.ty, name, element)?;
        match self.props.iter_mut().find(|p| p.index == index) {
            Some(p) => p.value = Some(value),
            None => {
                self.props.push(Property { index, value: Some(value) });
                self.props.sort_by_key(|p| p.index);
            }
        }
        Ok(())
    }
    pub fn remove(&mut self, schema: &Schema, name: &str) -> Result<()> {
        let index = schema.index_of(&self.ty, name, 0)?;
        self.props.retain(|p| p.index != index);
        Ok(())
    }
    /// Set an enum property by value name.
    pub fn set_enum(&mut self, schema: &Schema, name: &str, value: &str) -> Result<()> {
        let index = schema.index_of(&self.ty, name, 0)?;
        let (prop, _) = schema.property(&self.ty, index)?;
        let Type::Enum(enum_name, inner) = &prop.ty else { bail!("{}.{name} is not an enum", self.ty) };
        let n = schema.enum_value(enum_name, value)?;
        let bytes = match **inner {
            Type::Byte => vec![n as u8],
            Type::Int => (n as i32).to_le_bytes().to_vec(),
            _ => bail!("{}.{name}: unsupported enum storage", self.ty),
        };
        self.set(schema, name, 0, Value::Raw(bytes))
    }
    pub fn colors(&mut self, f: &mut dyn FnMut(Color)) {
        for p in &mut self.props {
            if let Some(v) = &mut p.value {
                v.colors(f);
            }
        }
    }
}
