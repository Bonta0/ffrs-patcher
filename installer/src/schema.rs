//! Layout of the game types the theme edits: which properties a class or struct has, in
//! which order, and of which type. The game saves its assets with "unversioned"
//! properties (values only, no names or types), so this is what makes them readable.
//!
//! `schema.txt` lists the needed types; `theme-dev schema` generates it from a type dump
//! of the game (a UE4SS .usmap). Format, one entry per line:
//!
//!   struct <Name> [: <Super>] <property count>
//!     <index> <Name>[<array size>] <type>
//!   enum <Name> <Value>=<n> ...
//!
//! A type is `Bool`, `Int`, `Float`, ... or `Struct <Name>`, `Enum <Name> <underlying>`,
//! `Array <type>`, `Set <type>`, `Optional <type>`, `Map <key> <value>`.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};

/// Structs saved in their own binary form rather than as properties, with their size when
/// fixed. Their fields are not listed in the schema.
pub const NATIVE: &[(&str, Option<usize>)] = &[
    ("LinearColor", Some(16)),
    ("Color", Some(4)),
    ("Guid", Some(16)),
    ("Vector", Some(24)),
    ("Vector2D", Some(16)),
    ("Vector4", Some(32)),
    ("Vector4d", Some(32)),
    ("Vector2f", Some(8)),
    ("Vector3f", Some(12)),
    ("Vector4f", Some(16)),
    ("DeprecateSlateVector2D", Some(8)),
    ("Rotator", Some(24)),
    ("Quat", Some(32)),
    ("IntPoint", Some(8)),
    ("IntVector", Some(12)),
    ("Box2D", Some(33)),
    ("Box2f", Some(17)),
    ("Box", Some(49)),
    ("DateTime", Some(8)),
    ("Timespan", Some(8)),
    ("FrameNumber", Some(4)),
    ("MovieSceneFrameRange", Some(10)),
    ("SoftObjectPath", None),
    ("SoftClassPath", None),
    ("GameplayTagContainer", None),
    ("MovieSceneFloatChannel", None),
];

pub fn native(name: &str) -> Option<Option<usize>> {
    NATIVE.iter().find(|(n, _)| *n == name).map(|(_, size)| *size)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Bool,
    Int8,
    Int16,
    Int,
    Int64,
    Byte,
    UInt16,
    UInt32,
    UInt64,
    Float,
    Double,
    Name,
    Str,
    Utf8Str,
    AnsiStr,
    Text,
    Object,
    WeakObject,
    LazyObject,
    SoftObject,
    Interface,
    Delegate,
    MulticastDelegate,
    FieldPath,
    Struct(String),
    Enum(String, Box<Type>),
    Array(Box<Type>),
    Set(Box<Type>),
    Optional(Box<Type>),
    Map(Box<Type>, Box<Type>),
}

impl Type {
    /// Parse from whitespace-separated tokens.
    pub fn parse<'a>(tokens: &mut impl Iterator<Item = &'a str>) -> Result<Type> {
        let token = tokens.next().context("missing type")?;
        let mut name = || tokens.next().map(str::to_string).context("missing type name");
        Ok(match token {
            "Bool" => Type::Bool,
            "Int8" => Type::Int8,
            "Int16" => Type::Int16,
            "Int" => Type::Int,
            "Int64" => Type::Int64,
            "Byte" => Type::Byte,
            "UInt16" => Type::UInt16,
            "UInt32" => Type::UInt32,
            "UInt64" => Type::UInt64,
            "Float" => Type::Float,
            "Double" => Type::Double,
            "Name" => Type::Name,
            "Str" => Type::Str,
            "Utf8Str" => Type::Utf8Str,
            "AnsiStr" => Type::AnsiStr,
            "Text" => Type::Text,
            "Object" => Type::Object,
            "WeakObject" => Type::WeakObject,
            "LazyObject" => Type::LazyObject,
            "SoftObject" => Type::SoftObject,
            "Interface" => Type::Interface,
            "Delegate" => Type::Delegate,
            "MulticastDelegate" => Type::MulticastDelegate,
            "FieldPath" => Type::FieldPath,
            "Struct" => Type::Struct(name()?),
            "Enum" => {
                let enum_name = name()?;
                Type::Enum(enum_name, Box::new(Type::parse(tokens)?))
            }
            "Array" => Type::Array(Box::new(Type::parse(tokens)?)),
            "Set" => Type::Set(Box::new(Type::parse(tokens)?)),
            "Optional" => Type::Optional(Box::new(Type::parse(tokens)?)),
            "Map" => {
                let key = Type::parse(tokens)?;
                Type::Map(Box::new(key), Box::new(Type::parse(tokens)?))
            }
            other => bail!("unknown type {other}"),
        })
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Struct(n) => write!(f, "Struct {n}"),
            Type::Enum(n, inner) => write!(f, "Enum {n} {inner}"),
            Type::Array(t) => write!(f, "Array {t}"),
            Type::Set(t) => write!(f, "Set {t}"),
            Type::Optional(t) => write!(f, "Optional {t}"),
            Type::Map(k, v) => write!(f, "Map {k} {v}"),
            other => write!(f, "{other:?}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Property {
    pub index: usize,
    pub name: String,
    pub dim: usize,
    pub ty: Type,
}

#[derive(Debug, Clone)]
pub struct Struct {
    pub super_: Option<String>,
    /// properties of this struct alone (static arrays count once per element)
    pub count: usize,
    pub props: Vec<Property>,
}

#[derive(Default)]
pub struct Schema {
    pub structs: HashMap<String, Struct>,
    pub enums: HashMap<String, Vec<(String, i64)>>,
}

const TEXT: &str = include_str!("schema.txt");

impl Schema {
    pub fn load() -> Result<Schema> {
        Schema::parse(TEXT)
    }

    pub fn parse(text: &str) -> Result<Schema> {
        let mut schema = Schema::default();
        let mut current: Option<String> = None;
        for (n, line) in text.lines().enumerate() {
            let line = line.trim_end();
            let parsed: Result<()> = (|| {
                if line.is_empty() || line.starts_with('#') {
                    return Ok(());
                }
                let mut t = line.split_whitespace();
                if !line.starts_with(' ') {
                    match t.next() {
                        Some("struct") => {
                            let name = t.next().context("struct name")?.to_string();
                            let mut next = t.next().context("struct count")?;
                            let mut super_ = None;
                            if next == ":" {
                                super_ = Some(t.next().context("super name")?.to_string());
                                next = t.next().context("struct count")?;
                            }
                            schema.structs.insert(name.clone(), Struct { super_, count: next.parse()?, props: Vec::new() });
                            current = Some(name);
                        }
                        Some("enum") => {
                            let name = t.next().context("enum name")?.to_string();
                            let values = t
                                .map(|v| {
                                    let (k, n) = v.rsplit_once('=').context("enum value")?;
                                    Ok((k.to_string(), n.parse()?))
                                })
                                .collect::<Result<_>>()?;
                            schema.enums.insert(name, values);
                            current = None;
                        }
                        _ => bail!("expected struct or enum"),
                    }
                    return Ok(());
                }
                let owner = current.as_ref().context("property outside a struct")?;
                let index = t.next().context("index")?.parse()?;
                let decl = t.next().context("name")?;
                let (name, dim) = match decl.split_once('[') {
                    Some((name, dim)) => (name, dim.trim_end_matches(']').parse()?),
                    None => (decl, 1),
                };
                let ty = Type::parse(&mut t)?;
                schema.structs.get_mut(owner).unwrap().props.push(Property { index, name: name.to_string(), dim, ty });
                Ok(())
            })();
            parsed.with_context(|| format!("schema.txt line {}: {line}", n + 1))?;
        }
        Ok(schema)
    }

    pub fn get(&self, name: &str) -> Result<&Struct> {
        self.structs.get(name).with_context(|| format!("type {name} is not in schema.txt"))
    }

    /// Number of properties in `name` and all its parents.
    pub fn total(&self, name: &str) -> Result<usize> {
        let mut total = 0;
        let mut cur = Some(name);
        while let Some(n) = cur {
            let s = self.get(n)?;
            total += s.count;
            cur = s.super_.as_deref();
        }
        Ok(total)
    }

    /// The property at serialization index `index` of `name` (own properties first, then
    /// the parent's), with the array element it stands for.
    pub fn property(&self, name: &str, mut index: usize) -> Result<(&Property, usize)> {
        let mut cur = name;
        loop {
            let s = self.get(cur)?;
            if index < s.count {
                for p in &s.props {
                    if index >= p.index && index < p.index + p.dim {
                        return Ok((p, index - p.index));
                    }
                }
                bail!("{name}: no property at index {index}");
            }
            index -= s.count;
            cur = s.super_.as_deref().with_context(|| format!("{name}: property index out of range"))?;
        }
    }

    /// Serialization index of property `prop` (element `element`) of `name`.
    pub fn index_of(&self, name: &str, prop: &str, element: usize) -> Result<usize> {
        let mut base = 0;
        let mut cur = name;
        loop {
            let s = self.get(cur)?;
            if let Some(p) = s.props.iter().find(|p| p.name == prop) {
                if element >= p.dim {
                    bail!("{name}.{prop}[{element}] out of range");
                }
                return Ok(base + p.index + element);
            }
            base += s.count;
            cur = s.super_.as_deref().with_context(|| format!("{name} has no property {prop}"))?;
        }
    }

    pub fn enum_value(&self, name: &str, value: &str) -> Result<i64> {
        let values = self.enums.get(name).with_context(|| format!("enum {name} is not in schema.txt"))?;
        let full = |v: &str| v.rsplit("::").next().unwrap_or(v) == value;
        values.iter().find(|(v, _)| full(v)).map(|(_, n)| *n).with_context(|| format!("{name} has no value {value}"))
    }
}
