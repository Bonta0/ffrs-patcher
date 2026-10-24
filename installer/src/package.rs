//! A legacy package (.uasset header + .uexp export data) whose objects can be read,
//! edited and written back (props.rs for the objects' properties).

use std::io::Cursor;

use anyhow::{bail, Context, Result};
use retoc::legacy_asset::{FLegacyPackageHeader, FMinimalName};
use retoc::logging::Log;
use retoc::version::EngineVersion;

use crate::props::{self, Reader, Struct, Value};
use crate::schema::Schema;

const CLASS_DEFAULT_OBJECT: u32 = 0x10;
/// Classes that save data of their own after their properties (kept as is).
const CLASS_DATA: &[&str] = &["MaterialInstanceConstant"];

pub struct Package {
    header: FLegacyPackageHeader,
    /// each export's data, in export order
    data: Vec<Vec<u8>>,
    /// .uexp bytes after the last export (the package tag)
    tail: Vec<u8>,
}

/// An object's saved data: its properties, then what its class saves after them.
pub struct Object {
    pub props: Struct,
    /// the object's GUID flag (+ GUID)
    guid: Vec<u8>,
    /// a data table's rows: (row name, row)
    pub rows: Option<Vec<((i32, i32), Struct)>>,
    rest: Vec<u8>,
}

impl Package {
    pub fn read(uasset: &[u8], uexp: &[u8], engine: EngineVersion) -> Result<Package> {
        let header = FLegacyPackageHeader::deserialize(&mut Cursor::new(uasset), Some(engine.package_file_version()))?;
        let base = header.summary.versioning_info.total_header_size as i64;
        let mut order: Vec<usize> = (0..header.exports.len()).collect();
        order.sort_by_key(|&i| header.exports[i].serial_offset);
        let mut data = vec![Vec::new(); header.exports.len()];
        let mut end = 0;
        for i in order {
            let e = &header.exports[i];
            let start = (e.serial_offset - base) as usize;
            if start != end {
                bail!("export data is not contiguous");
            }
            end = start + e.serial_size as usize;
            data[i] = uexp.get(start..end).context("export outside the .uexp")?.to_vec();
        }
        Ok(Package { tail: uexp[end..].to_vec(), header, data })
    }

    pub fn write(mut self) -> Result<(Vec<u8>, Vec<u8>)> {
        let mut order: Vec<usize> = (0..self.data.len()).collect();
        order.sort_by_key(|&i| self.header.exports[i].serial_offset);
        let mut uexp = Vec::new();
        for i in order {
            // offsets relative to the end of the header; serialize() adds its size
            let e = &mut self.header.exports[i];
            e.serial_offset = uexp.len() as i64;
            e.serial_size = self.data[i].len() as i64;
            e.script_serialization_end_offset = e.serial_size;
            uexp.extend(&self.data[i]);
        }
        uexp.extend(&self.tail);
        let mut uasset = Cursor::new(Vec::new());
        let size = self.header.summary.versioning_info.total_header_size as usize;
        self.header.serialize(&mut uasset, Some(size), &Log::new_stdout(false, false))?;
        Ok((uasset.into_inner(), uexp))
    }

    pub fn name(&self, (index, number): (i32, i32)) -> Result<String> {
        Ok(self.header.name_map.get(FMinimalName { index, number })?.into_owned())
    }

    /// Name of the object a package index points to (import or export).
    pub fn object_name(&self, index: i32) -> Result<String> {
        let name = match index {
            0 => bail!("null object reference"),
            i if i < 0 => self.header.imports.get((-i - 1) as usize).context("bad import")?.object_name,
            i => self.header.exports.get((i - 1) as usize).context("bad export")?.object_name,
        };
        Ok(self.header.name_map.get(name)?.into_owned())
    }

    /// The export named after the package (its main object).
    pub fn asset(&self) -> Result<usize> {
        let package = &self.header.summary.package_name;
        self.export(package.rsplit('/').next().unwrap_or(package))
    }

    /// Index of the first export called `name`.
    pub fn export(&self, name: &str) -> Result<usize> {
        self.exports(name).into_iter().next().with_context(|| format!("no object {name}"))
    }

    pub fn exports(&self, name: &str) -> Vec<usize> {
        (0..self.header.exports.len())
            .filter(|&i| self.header.name_map.get(self.header.exports[i].object_name).is_ok_and(|n| n == name))
            .collect()
    }

    pub fn read_object(&self, schema: &Schema, export: usize) -> Result<Object> {
        let e = &self.header.exports[export];
        let class = self.object_name(e.class_index.index)?;
        let data = &self.data[export];
        let mut r = Reader::new(data);
        let props = props::read_struct(&mut r, schema, &class)?;
        let at = r.pos();
        if e.object_flags & CLASS_DEFAULT_OBJECT == 0 && r.i32()? != 0 {
            r.take(16)?;
        }
        let guid = data[at..r.pos()].to_vec();
        let mut object = Object { props, guid, rows: None, rest: Vec::new() };
        if schema.index_of(&class, "RowStruct", 0).is_ok() {
            let row_struct = object.props.get(schema, "RowStruct", 0)?.context("data table without a row type")?.object()?;
            let row_struct = self.object_name(row_struct)?;
            let mut rows = Vec::new();
            for _ in 0..r.i32()? {
                let name = Value::Raw(r.take(8)?.to_vec()).name()?;
                rows.push((name, props::read_struct(&mut r, schema, &row_struct)?));
            }
            object.rows = Some(rows);
        }
        object.rest = r.rest().to_vec();
        // anything else must be read to the end, or its layout was not what schema.txt says
        if !object.rest.is_empty() && !CLASS_DATA.contains(&class.as_str()) {
            bail!("{} unexpected bytes after the properties", object.rest.len());
        }
        Ok(object)
    }

    pub fn write_object(&mut self, schema: &Schema, export: usize, object: &Object) -> Result<()> {
        let mut out = Vec::new();
        props::write_struct(&mut out, schema, &object.props)?;
        out.extend(&object.guid);
        if let Some(rows) = &object.rows {
            out.extend((rows.len() as i32).to_le_bytes());
            for ((index, number), row) in rows {
                out.extend(index.to_le_bytes());
                out.extend(number.to_le_bytes());
                props::write_struct(&mut out, schema, row)?;
            }
        }
        out.extend(&object.rest);
        self.data[export] = out;
        Ok(())
    }

    /// Read an object, check it writes back unchanged, edit it and store the result.
    pub fn edit(&mut self, schema: &Schema, export: usize, f: impl FnOnce(&Package, &mut Object) -> Result<()>) -> Result<()> {
        let name = self.header.name_map.get(self.header.exports[export].object_name)?.into_owned();
        let original = self.data[export].clone();
        let mut object = self.read_object(schema, export).with_context(|| format!("reading {name}"))?;
        self.write_object(schema, export, &object)?;
        if self.data[export] != original {
            bail!("{name}: not understood exactly (reading and writing it back changes it)");
        }
        f(self, &mut object).with_context(|| format!("editing {name}"))?;
        self.write_object(schema, export, &object)
    }
}
