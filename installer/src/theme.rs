//! The UI theme, generated on the player's machine from their own game files.
//!
//! No game file ships with the patcher: it extracts the UI files the theme changes from
//! the game's IoStore containers (retoc, legacy .uasset/.uexp form), edits them
//! (edits.rs), draws the theme's textures and battle banners into the game's texture
//! files (art.rs, fonts.rs) and packs the results into a mod container in `~mods`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use retoc::asset_conversion::{self, FZenPackageContext};
use retoc::iostore;
use retoc::iostore_writer::IoStoreWriter;
use retoc::legacy_asset::FSerializedAssetBundle;
use retoc::logging::Log;
use retoc::version::EngineVersion;
use retoc::zen_asset_conversion;
use retoc::{build_verse_cell_store, Config, EIoChunkType, FIoChunkId, FileWriterTrait, UEPath, UEPathBuf};

use crate::package::Package;
use crate::schema::Schema;
use crate::{art, edits, fonts};

const ENGINE: EngineVersion = EngineVersion::UE5_6;
const MOUNT_POINT: &str = "../../../";

/// In-memory sink for retoc's legacy conversion.
#[derive(Default)]
struct MemWriter(Mutex<HashMap<String, Vec<u8>>>);

impl FileWriterTrait for MemWriter {
    fn write_file(&self, path: String, _allow_compress: bool, data: Vec<u8>) -> Result<()> {
        self.0.lock().unwrap().insert(path.replace('\\', "/"), data);
        Ok(())
    }
}

/// Legacy files (.uasset, .uexp, ...) of the given packages, from the containers in
/// `paks` (the game's Content/Paks). `packages`: .uasset paths under the game root.
pub fn extract(paks: &Path, packages: &[String]) -> Result<HashMap<String, Vec<u8>>> {
    let store = iostore::open(paks, Arc::new(Config::default())).with_context(|| format!("reading {}", paks.display()))?;
    let log = Log::new_stdout(false, false);
    let context = FZenPackageContext::create(&*store, Some(ENGINE.package_file_version()), &log, Some(build_verse_cell_store(&vec![])));
    let wanted: HashMap<String, &String> = packages.iter().map(|p| (format!("{MOUNT_POINT}{p}").to_lowercase(), p)).collect();
    let writer = MemWriter::default();
    let mut found = 0;
    for package in store.packages() {
        let chunk = FIoChunkId::from_package_id(package.id(), 0, EIoChunkType::ExportBundleData);
        let Some(path) = store.chunk_path(chunk) else { continue };
        if let Some(rel) = wanted.get(&path.to_lowercase()) {
            asset_conversion::build_legacy(&context, package.id(), UEPath::new(rel.as_str()), &writer)
                .with_context(|| format!("extracting {rel}"))?;
            found += 1;
        }
    }
    if found != wanted.len() {
        bail!("{} of {} UI files not found in the game", wanted.len() - found, wanted.len());
    }
    Ok(writer.0.into_inner().unwrap())
}

/// The theme's files, made from the game's: path under the game root -> legacy file.
pub fn build(paks: &Path) -> Result<HashMap<String, Vec<u8>>> {
    let schema = Schema::load()?;
    let asset = |a: &str, ext: &str| format!("FFRS/Content/{a}.{ext}");
    let mut packages: Vec<String> = edits::EDITS.iter().map(|(a, _)| asset(a, "uasset")).collect();
    packages.extend(art::packages());
    packages.extend(fonts::banner_packages());
    let mut files = extract(paks, &packages)?;
    for (a, edit) in edits::EDITS {
        let (uasset, uexp) = (asset(a, "uasset"), asset(a, "uexp"));
        let mut package = Package::read(&files[&uasset], &files[&uexp], ENGINE)?;
        edit(&schema, &mut package).with_context(|| format!("{a} (different game version?)"))?;
        let (header, data) = package.write()?;
        files.insert(uasset, header);
        files.insert(uexp, data);
    }
    art::draw(&mut files)?;
    fonts::draw_banners(&mut files)?;
    Ok(files)
}

/// Pack legacy files into an IoStore mod container at `utoc` (+ .ucas), uncompressed.
pub fn pack(files: &HashMap<String, Vec<u8>>, utoc: &Path) -> Result<()> {
    let mount_point = UEPath::new(MOUNT_POINT);
    let mut writer = IoStoreWriter::new(utoc, ENGINE.toc_version(), Some(ENGINE.container_header_version()), mount_point.into())?;
    let log = Log::new_stdout(false, false);
    let cells = build_verse_cell_store(&vec![]);
    let no_shaders = HashMap::new();
    let mut assets: Vec<&String> = files.keys().filter(|p| p.ends_with(".uasset")).collect();
    assets.sort();
    for path in assets {
        let sibling = |ext: &str| files.get(&Path::new(path).with_extension(ext).to_string_lossy().replace('\\', "/")).cloned();
        let bundle = FSerializedAssetBundle {
            asset_file_buffer: files[path].clone(),
            exports_file_buffer: sibling("uexp").with_context(|| format!("{path} has no .uexp"))?,
            bulk_data_buffer: sibling("ubulk"),
            optional_bulk_data_buffer: sibling("uptnl"),
            memory_mapped_bulk_data_buffer: sibling("m.ubulk"),
        };
        let mut converted = zen_asset_conversion::build_zen_asset(
            bundle,
            &no_shaders,
            &mount_point.join(UEPathBuf::from(path.as_str())),
            Some(ENGINE.package_file_version()),
            writer.container_header_version(),
            false, // UE5.6 containers need no cross-package import fix-up
            None,
            Some(cells.clone()),
            &log,
        )
        .with_context(|| format!("packing {path}"))?;
        converted.write(&mut writer)?;
    }
    writer.finalize()?;
    Ok(())
}
