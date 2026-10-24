//! Links the DirectSound export table.

use std::path::Path;

fn main() {
    let def = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsound.def");
    println!("cargo:rustc-cdylib-link-arg=/DEF:{}", def.display());
    println!("cargo:rerun-if-changed=dsound.def");
}
