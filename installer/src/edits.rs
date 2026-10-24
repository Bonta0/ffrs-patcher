//! The theme's edits to the game's UI assets, toward Octopath Traveler II's look:
//!
//! * colours: neutral charcoal panels, white hairlines and white text. UI chrome that is
//!   blue/cyan becomes grey at the same brightness; colour stays where it carries meaning
//!   (HP/MP, friend/foe, map icons, limit/negative/mastery highlights).
//! * battle layout: the party status as a compact column at the top right (game: a row
//!   along the bottom), the command list beside it above the characters (game: top-right
//!   corner), and no backing band behind the turn order.
//!
//! Layout values are UMG units (1080p reference; the game scales them).

use anyhow::{Context, Result};

use crate::package::{Object, Package};
use crate::props::{Color, Struct, Value};
use crate::schema::Schema;

/// Asset path under FFRS/Content -> edit.
pub type Edit = fn(&Schema, &mut Package) -> Result<()>;

pub const EDITS: &[(&str, Edit)] = &[
    // data tables
    ("Datatable/UI/Common/Window/DT_CmnWindowBorderStyle", border_style),
    ("Datatable/UI/Common/Window/DT_LeftLineWindowStyle", |s, p| table_colors(s, p, &[], Hues { lo: 175.0, ..Hues::TABLE })),
    ("Datatable/UI/Common/HorizontalGradient/DA_HorizontalGradient", horizontal_gradient),
    ("Datatable/UI/Common/Line/DT_CommonLine", |s, p| table_colors(s, p, &[], Hues::TABLE)),
    ("Datatable/Color/DT_MainMenuColor", |s, p| table_colors(s, p, &[], Hues::TABLE)),
    ("Datatable/Color/DT_CommonColor", |s, p| table_colors(s, p, GAMEPLAY_ROWS, Hues::TABLE)),
    ("Datatable/Color/CDT_CommonColor", |s, p| table_colors(s, p, &[GAMEPLAY_ROWS, STATUS_ROWS].concat(), Hues::TABLE)),
    // decorative blues in widgets and materials
    ("UI/Option/Entry/Widget/Parts/WBP_OP_RadioButtonParts", |s, p| chrome(s, p, &["Image_Select"])),
    ("UI/Option/Entry/Widget/Parts/WBP_OP_CommandBG", |s, p| chrome(s, p, &["Image_Select"])),
    ("UI/Option/Entry/Widget/Parts/WBP_OptionList_EntryBackGround", |s, p| chrome(s, p, &["Image", "Image_1"])),
    ("UI/Option/Entry/Widget/Parts/WBP_OP_InputSetting_Category", |s, p| chrome(s, p, &["Image_72"])),
    ("UI/Option/Entry/Widget/Parts/WBP_OP_KeySelect", |s, p| chrome(s, p, &["img_select"])),
    ("UI/Option/Entry/Widget/Parts/WBP_OP_KeySelectList", |s, p| chrome(s, p, &["Image", "Image_166"])),
    ("UI/Option/Entry/Widget/Parts/WBP_OP_BrightnessWindow", |s, p| chrome(s, p, &["Image_79", "Slider"])),
    ("UI/Option/Entry/Widget/WBP_OP_Slider", |s, p| chrome(s, p, &["Image_79", "Slider"])),
    ("UI/Option/Entry/Widget/WBP_OP_Brightness", |s, p| chrome(s, p, &["Button_67"])),
    ("UI/Common/Scroll/Widget/WBP_CommonScrollBox", |s, p| chrome(s, p, &["CPP_MuchaScrollBox"])),
    ("UI/Common/Cursor/Tab/WBP_TabCursorLight", |s, p| chrome(s, p, &["Image_Select"])),
    ("UI/Common/Cursor/Finger/WBP_FingerCursor", |s, p| chrome(s, p, &["Image_Image_Glow"])),
    ("UI/Common/Layout/BG/WBP_LeftLineBG", |s, p| chrome(s, p, &["LineImage"])),
    ("UI/Common/menu/ListView/WBP_ListViewMenu", |s, p| chrome(s, p, &["CmnListView"])),
    ("UI/Common/menu/ListView/WBP_TileViewMenu", |s, p| chrome(s, p, &["TileView"])),
    ("UI/Battle/Command/Finish/Widget/WBP_FinishCmdTitle", |s, p| chrome(s, p, &["Image_124", "Image_BG", "IMG_addEffect", "TextBlock_79"])),
    ("UI/Battle/SkillMessage/WBP_Btl_SkillMsg", |s, p| chrome(s, p, &["Image_BG", "Image_Line_L", "Image_Line_U"])),
    ("UI/MainMenu/Common/Material/MI_CharaListBg", asset_chrome),
    ("UI/Textures/menu/MI_MenuList_BG", asset_chrome),
    ("UI/Textures/menu/MI_EvButtonEff", asset_chrome),
    ("UI/Title/List/WBP_Title_ListEntry", |s, p| chrome(s, p, &["CommonTextBlock_47"])),
    // battle layout
    ("UI/Battle/Widget/WBP_BattleLayout", battle_layout),
    ("UI/Battle/Unit/Statuses/WBP_Btl_UnitStatusList", party_list),
    ("UI/Battle/Unit/Statuses/WBP_Btl_UnitStatus", party_entry),
    ("UI/Battle/Command/List/Widget/WBP_BtlCmd_ListFrame", command_frame),
    ("UI/Battle/Command/List/Widget/WBP_BtlCmd_PrimaryList", command_list),
    ("UI/Battle/TimeLine/Widget/WBP_Btl_Turn", turn_band),
];

// ---------------------------------------------------------------- colour maths

fn lin_to_srgb(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_to_lin(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    if max == min {
        return (0.0, 0.0, max);
    }
    let range = max - min;
    let (rc, gc, bc) = ((max - r) / range, (max - g) / range, (max - b) / range);
    let h = if r == max {
        bc - gc
    } else if g == max {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };
    ((h / 6.0).rem_euclid(1.0), range / max, max)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (v, v, v);
    }
    let i = (h * 6.0) as i64;
    let f = h * 6.0 - i as f64;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// Which colours count as cool UI chrome: hue range (degrees) and minimum saturation.
#[derive(Clone, Copy)]
struct Hues {
    lo: f64,
    min_sat: f64,
}

impl Hues {
    const TABLE: Hues = Hues { lo: 175.0, min_sat: 0.12 };
    const WIDGET: Hues = Hues { lo: 180.0, min_sat: 0.20 };
    const HI: f64 = 262.0;
    const SAT_MUL: f64 = 0.08;
}

/// Drain a cool colour (linear RGB) to near-grey, keeping its brightness; None for
/// colours outside the cool range (left alone).
fn neutralize(rgb: [f64; 3], hues: Hues) -> Option<[f64; 3]> {
    let [r, g, b] = rgb.map(lin_to_srgb);
    let (h, s, v) = rgb_to_hsv(r, g, b);
    if !(hues.lo <= h * 360.0 && h * 360.0 <= Hues::HI && s >= hues.min_sat) {
        return None;
    }
    let (r, g, b) = hsv_to_rgb(h, (s * Hues::SAT_MUL).min(1.0), v.min(1.0));
    Some([r, g, b].map(srgb_to_lin))
}

fn le_f32(b: &[u8], i: usize) -> f64 {
    f32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap()) as f64
}

fn set_f32(b: &mut [u8], i: usize, v: f64) {
    b[i * 4..i * 4 + 4].copy_from_slice(&(v as f32).to_le_bytes());
}

/// Neutralize every cool colour reached by `visit`.
fn neutralize_all(hues: Hues, visit: impl FnOnce(&mut dyn FnMut(Color))) {
    visit(&mut |c| match c {
        Color::Linear(b) => {
            if let Some(new) = neutralize([0, 1, 2].map(|i| le_f32(b, i)), hues) {
                for (i, v) in new.into_iter().enumerate() {
                    set_f32(b, i, v);
                }
            }
        }
        Color::Srgb(b) => {
            // FColor is stored B, G, R, A
            let rgb = [2, 1, 0].map(|i| srgb_to_lin(b[i] as f64 / 255.0));
            if let Some(new) = neutralize(rgb, hues) {
                for (i, v) in [2, 1, 0].into_iter().zip(new) {
                    b[i] = (lin_to_srgb(v) * 255.0).round() as u8;
                }
            }
        }
    });
}

/// Neutral linear grey with the faintest cool cast.
const fn grey(v: f64, a: f64) -> [f64; 4] {
    [v, v, v * 1.08, a]
}

fn set_color(c: &mut Value, rgba: [f64; 4]) -> Result<()> {
    let Value::Native(ty, b) = c else { anyhow::bail!("not a colour") };
    anyhow::ensure!(ty == "LinearColor", "not a LinearColor");
    for (i, v) in rgba.into_iter().enumerate() {
        set_f32(b, i, v);
    }
    Ok(())
}

// ---------------------------------------------------------------- colour edits

/// DT_CommonColor / CDT_CommonColor rows whose colours carry meaning: factions, map landmarks.
const GAMEPLAY_ROWS: &[&str] = &["陣営_味方", "陣営_敵", "Map_Landmark"];
/// CDT_CommonColor status colours.
const STATUS_ROWS: &[&str] = &["HP", "MP", "MP_減少", "HP_Title", "MP_Title", "ステータス表示_増加", "青"];

fn table(schema: &Schema, p: &mut Package, f: impl FnOnce(&Package, &mut Vec<((i32, i32), Struct)>) -> Result<()>) -> Result<()> {
    let export = p.asset()?;
    p.edit(schema, export, |p, o: &mut Object| f(p, o.rows.as_mut().context("not a data table")?))
}

/// Every cool colour of a data table, except rows named in `skip`.
fn table_colors(schema: &Schema, p: &mut Package, skip: &[&str], hues: Hues) -> Result<()> {
    table(schema, p, |p, rows| {
        for (name, row) in rows {
            if !skip.contains(&p.name(*name)?.as_str()) {
                neutralize_all(hues, |f| row.colors(f));
            }
        }
        Ok(())
    })
}

fn border_style(schema: &Schema, p: &mut Package) -> Result<()> {
    table(schema, p, |p, rows| {
        for (name, row) in rows {
            match p.name(*name)?.as_str() {
                // system windows / dialogs: charcoal, slightly lighter on top
                "dialog2" => {
                    let dc = row.field(schema, "DirectionColor")?.as_struct()?;
                    for (side, rgba) in [("TopColor", grey(0.045, 0.90)), ("BottomColor", grey(0.012, 0.90))] {
                        set_color(dc.field(schema, side)?.as_struct()?.field(schema, "LinearColor")?, rgba)?;
                    }
                }
                "ToolTips" => neutralize_all(Hues { lo: 150.0, min_sat: 0.03 }, |f| row.colors(f)),
                _ => {}
            }
        }
        Ok(())
    })
}

/// DA_HorizontalGradient styles (base, head) in linear RGBA. These back most list rows,
/// command menus and panel headers: Octopath II keeps them dark charcoal so only the
/// selection stands out. Unlisted styles (Limit, Negative, Libra, Ability.Unmastered...)
/// keep their colours.
const GRADIENT_STYLES: &[(&str, [f64; 4], [f64; 4])] = &[
    ("UI.Common.BG.HorizontalGradient.Color.Default", grey(0.016, 1.0), grey(0.016, 1.0)),
    ("UI.Common.BG.HorizontalGradient.Color.ListTitle", grey(0.018, 1.0), grey(0.065, 1.0)),
    ("UI.Common.BG.HorizontalGradient.Color.statusDetail", grey(0.010, 1.0), grey(0.045, 1.0)),
    ("UI.Common.BG.HorizontalGradient.Color.ATCBG", grey(0.010, 1.0), grey(0.045, 1.0)),
    ("UI.Common.BG.HorizontalGradient.Color.ListBG.Extra", grey(0.010, 1.0), grey(0.050, 1.0)),
    // the acting unit's name strip and the command header: clearly lighter than the rest
    ("UI.Common.BG.HorizontalGradient.Color.ListTitle.Extra", grey(0.07, 1.0), grey(0.13, 1.0)),
];

fn horizontal_gradient(schema: &Schema, p: &mut Package) -> Result<()> {
    let export = p.asset()?;
    p.edit(schema, export, |p, o| {
        let Value::Map { entries, .. } = o.props.field(schema, "ColorStyleMap")? else { anyhow::bail!("ColorStyleMap is not a map") };
        for (key, value) in entries {
            let tag = p.name(key.as_struct()?.field(schema, "TagName")?.name()?)?;
            let Some(&(_, base, head)) = GRADIENT_STYLES.iter().find(|(t, ..)| *t == tag) else { continue };
            // BaseColor, then HeadColor
            let mut colors = [base, head].into_iter();
            value.colors(&mut |c| {
                if let Color::Linear(b) = c {
                    if let Some(rgba) = colors.next() {
                        for (i, v) in rgba.into_iter().enumerate() {
                            set_f32(b, i, v);
                        }
                    }
                }
            });
        }
        Ok(())
    })
}

/// Widget and material objects whose cool colours are only decoration.
fn chrome(schema: &Schema, p: &mut Package, objects: &[&str]) -> Result<()> {
    for name in objects {
        let exports = p.exports(name);
        anyhow::ensure!(!exports.is_empty(), "no object {name}");
        for export in exports {
            p.edit(schema, export, |_, o| {
                neutralize_all(Hues::WIDGET, |f| o.props.colors(f));
                Ok(())
            })?;
        }
    }
    Ok(())
}

/// A material instance: all its colours.
fn asset_chrome(schema: &Schema, p: &mut Package) -> Result<()> {
    let export = p.asset()?;
    p.edit(schema, export, |_, o| {
        neutralize_all(Hues::WIDGET, |f| o.props.colors(f));
        Ok(())
    })
}

// ---------------------------------------------------------------- battle layout

const PARTY_TOP: f32 = 36.0; // game: anchored to the bottom edge
const PARTY_RIGHT: f32 = 16.0;
const PARTY_ENTRY_WIDTH: f32 = 212.0; // game: 240
const PARTY_ENTRY_HEIGHT: f32 = 124.0;
const PARTY_SPACING: f32 = 4.0;
const HP_SIZE: f32 = 22.0; // game: 18 (bold, shared style)
const MP_SIZE: f32 = 17.0; // game: 16
const NAME_SIZE: f32 = 16.0; // game: 18
const CMD_RIGHT: f32 = 290.0; // clears the party column + gap
const CMD_TOP: f32 = 95.0; // above the character sprites, below the turn order (game: top-right corner)
const CMD_WIDTH: f32 = 420.0; // game: 500

/// Set sides of a Margin; sides at zero are left unsaved (zero is the default).
fn set_margin(schema: &Schema, slot: &mut Struct, sides: &[(&str, f32)]) -> Result<()> {
    let pad = slot.field(schema, "Padding")?.as_struct()?;
    for &(side, v) in sides {
        if v == 0.0 {
            pad.remove(schema, side)?;
        } else {
            pad.set(schema, side, 0, Value::float(v))?;
        }
    }
    Ok(())
}

/// The slot (named `slot`) that holds the object named `content`.
fn slot_of(schema: &Schema, p: &Package, slot: &str, content: &str) -> Result<usize> {
    let content = p.export(content)? as i32 + 1;
    for export in p.exports(slot) {
        let mut o = p.read_object(schema, export)?;
        if o.props.get(schema, "Content", 0)?.is_some_and(|v| v.object().ok() == Some(content)) {
            return Ok(export);
        }
    }
    anyhow::bail!("no {slot} holding the object")
}

fn set_font_size(schema: &Schema, o: &mut Object, size: f32) -> Result<()> {
    o.props.field(schema, "Font")?.as_struct()?.set(schema, "Size", 0, Value::float(size))
}

/// Party list: bottom-right corner -> top-right corner.
fn battle_layout(schema: &Schema, p: &mut Package) -> Result<()> {
    let slot = slot_of(schema, p, "OverlaySlot_0", "WBP_Btl_CharaState")?;
    p.edit(schema, slot, |_, o| {
        o.props.set_enum(schema, "VerticalAlignment", "VAlign_Top")?;
        set_margin(schema, &mut o.props, &[("Top", PARTY_TOP), ("Right", PARTY_RIGHT), ("Bottom", 0.0)])
    })
}

/// Stack the party entries vertically.
fn party_list(schema: &Schema, p: &mut Package) -> Result<()> {
    let export = p.export("WBP_DirectionLayoutMenu")?;
    p.edit(schema, export, |_, o| {
        let grid = o.props.field(schema, "GridProperty")?.as_struct()?;
        grid.set_enum(schema, "GridDirection", "Vertical")?;
        grid.field(schema, "SpaceBetweenEntryInGridDirection")?;
        grid.set(schema, "SpaceBetweenEntryInGridDirection", 0, Value::float(PARTY_SPACING))
    })
}

/// Compact entry for the column; large Octopath-style HP/MP numerals.
fn party_entry(schema: &Schema, p: &mut Package) -> Result<()> {
    let size_box = p.export("SizeBox_0")?;
    p.edit(schema, size_box, |_, o| {
        for (prop, v) in [("WidthOverride", PARTY_ENTRY_WIDTH), ("HeightOverride", PARTY_ENTRY_HEIGHT)] {
            o.props.field(schema, prop)?;
            o.props.set(schema, prop, 0, Value::float(v))?;
        }
        Ok(())
    })?;
    let name = p.export("Text_Name")?;
    p.edit(schema, name, |_, o| set_font_size(schema, o, NAME_SIZE))?;
    let mp = p.export("Text_MP_Value")?;
    p.edit(schema, mp, |_, o| set_font_size(schema, o, MP_SIZE))?;
    // HP normally takes its font from a shared text style (also used by other menus):
    // give this one text block its own font instead, so the change stays in battle.
    let mut mp_font = p.read_object(schema, mp)?.props.field(schema, "Font")?.clone();
    mp_font.as_struct()?.set(schema, "Size", 0, Value::float(HP_SIZE))?;
    let hp = p.export("Text_HP_Value")?;
    p.edit(schema, hp, |_, o| {
        o.props.remove(schema, "Style")?;
        o.props.field(schema, "Font")?;
        o.props.set(schema, "Font", 0, mp_font)
    })?;
    // The acting member's entry pops out of the row (animation A_Action: 16 units up);
    // in a column it pops out to the left instead.
    let section = p.export("MovieScene2DTransformSection_0")?;
    p.edit(schema, section, |_, o| {
        let mut x = o.props.get(schema, "Translation", 0)?.context("no X translation")?.clone();
        let y = o.props.get(schema, "Translation", 1)?.context("no Y translation")?;
        swap_channel_values(&mut x, y)?;
        o.props.set(schema, "Translation", 0, x)
    })
}

/// Swap the key values of two float curves (MovieSceneFloatChannel), keeping their times.
fn swap_channel_values(a: &mut Value, b: &mut Value) -> Result<()> {
    // extrapolation (2), times (size, count, data), values (size, count, data)
    fn values(v: &mut Value) -> Result<(&mut [u8], usize)> {
        let Value::Native(ty, data) = v else { anyhow::bail!("not a curve") };
        anyhow::ensure!(ty == "MovieSceneFloatChannel", "not a float curve");
        let le = |d: &[u8], at: usize| i32::from_le_bytes(d[at..at + 4].try_into().unwrap()) as usize;
        let times = 2 + 8 + le(data, 2) * le(data, 6);
        let (size, count) = (le(data, times), le(data, times + 4));
        anyhow::ensure!(size >= 4, "unexpected curve key size");
        Ok((&mut data[times + 8..times + 8 + size * count], size))
    }
    let (a, size) = values(a)?;
    let (b, _) = values(b)?;
    for (ka, kb) in a.chunks_mut(size).zip(b.chunks_mut(size)) {
        // FMovieSceneFloatValue starts with the value
        let tmp: [u8; 4] = ka[..4].try_into()?;
        ka[..4].copy_from_slice(&kb[..4]);
        kb[..4].copy_from_slice(&tmp);
    }
    Ok(())
}

/// Move the command lists down and left, beside the party column.
fn command_frame(schema: &Schema, p: &mut Package) -> Result<()> {
    let primary = slot_of(schema, p, "OverlaySlot_1", "WBP_BtlCmd_PrimaryList")?;
    let secondary = slot_of(schema, p, "OverlaySlot_0", "WBP_BtlCmd_SecondaryList")?;
    p.edit(schema, primary, |_, o| set_margin(schema, &mut o.props, &[("Top", CMD_TOP), ("Right", CMD_RIGHT)]))?;
    p.edit(schema, secondary, |_, o| {
        set_margin(schema, &mut o.props, &[("Top", CMD_TOP + 42.0), ("Right", CMD_RIGHT - 16.0)])
    })
}

/// Narrower command window, closer to Octopath's compact list.
fn command_list(schema: &Schema, p: &mut Package) -> Result<()> {
    let export = p.export("SizeBox_0")?;
    p.edit(schema, export, |_, o| {
        o.props.field(schema, "WidthOverride")?;
        o.props.set(schema, "WidthOverride", 0, Value::float(CMD_WIDTH))
    })
}

/// Turn-order bar: hide the band behind it (Octopath II shows a bare line).
fn turn_band(schema: &Schema, p: &mut Package) -> Result<()> {
    let export = p.export("Image_BG")?;
    p.edit(schema, export, |_, o| o.props.set_enum(schema, "Visibility", "Collapsed"))
}
