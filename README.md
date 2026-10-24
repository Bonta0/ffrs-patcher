# FFRS Patcher

A mod for FINAL FANTASY RESONANCE (Steam, v1.0.1.0), installed with `FFRS-Patcher.exe`:
full analog movement, an Octopath Traveler II-style battle screen and menus, and
Octopath Traveler II's menu sounds read from the player's own copy. No game file is
shipped: the patcher builds the UI theme from the player's game files at install time.
Player-facing notes are in `dist/release/README.txt` after packaging.

## Layout

| Folder | What |
| --- | --- |
| `native/` | `dsound.dll` (Rust): analog movement patch and Octopath sound replacement, loaded by the game |
| `installer/` | `FFRS-Patcher.exe` (Rust): the setup window and the theme build |
| `installer/oodle_loader/` | replaces repak's `oodle_loader` (which downloads Oodle's DLL) for retoc and repak: Oodle decoding with `oozextract` |

The game's containers are read and the theme's written with [retoc](https://github.com/trumank/retoc) (MIT), fetched by Cargo.

The theme, in `installer/src/`:

| File | What |
| --- | --- |
| `theme.rs` | extracts the UI files it changes from the game's containers, edits them, packs the mod container |
| `edits.rs` | the edits: colours (blue chrome to charcoal) and the battle layout |
| `art.rs` | the menu textures, drawn from scratch |
| `fonts.rs` | Alegreya Sans (`installer/fonts/`, OFL) as the UI typeface, and the battle banners |
| `package.rs`, `props.rs` | reading and writing the game's assets (unversioned properties) |
| `schema.rs`, `schema.txt` | layout of the game types the edits touch |

## Building

```
cd native && cargo build --release
cd ../installer && cargo build --release --bin FFRS-Patcher
```

The patcher embeds `native/target/release/dsound.dll`, so build `native/` first.

`installer/src/bin/theme-dev.rs` is a development tool:

- `theme-dev preview "<game>\FFRS\Content\Paks" <out dir>` builds the theme as the
  patcher does and writes the container, the edited files and PNGs of the textures.
- `theme-dev schema <FFRS.usmap> installer/src/schema.txt` regenerates `schema.txt`
  (needed only if a game update changes those types, or to edit new types) from a type
  dump of the game made with UE4SS (`DumpUSMAP`).
