//! Runtime asset loading: slice the combined atlas and upload frames to GPU.
//!
//! The committed atlas (`assets/images/My project atlas.png`) packs 32 px cells
//! on a 33 px pitch (1 px gutter). We crop each needed cell and downscale it to
//! a 16×16 [`Texture2D`] for the game's 16 px tiles.
//!
//! Layout (row origin in source px, then columns left→right):
//! - Miner: rows Down=0, Up=33, Right=66, Left=99 — each with 11 columns
//!   (`base`, `idle`×2, `walk`×4, `mine`×4).
//! - Dino (beast): rows Down=132, Up=165, Right=198, Left=231 — 5 columns
//!   (`base`, `walk`×4).
//! - Terrain: Unbreakable at y=264, Mineable at y=297 (each a `base` + 16 wang
//!   tiles at columns 1..16), Dirt at y=330 (single `base`).
//! - Burst effect: Rock Breaking at y=363 (6 burst frames at columns 1..6).

use macroquad::prelude::*;

use crate::assets::ids::{BeastAnim, IconId, PickupId, PlayerAnim};
use crate::game::terrain::TerrainTile;

pub mod ids;
pub mod layout;

/// Atlas source cell size and the pitch between cell origins (1 px gutter).
const CELL: u32 = 32;
const PITCH: u32 = 33;

/// Column count of the miner sheet and beast sheet.
const PLAYER_ROWS: usize = 4;
const PLAYER_COLS: usize = 11; // base + idle x2 + walk x4 + mine x4
const BEAST_ROWS: usize = 4;
const BEAST_COLS: usize = 5; // base + walk x4

/// Number of Wang transition tiles in a rock family (mask 0..15).
const WANG_COLS: usize = 16;
/// Number of burst frames in the rock-breaking effect.
const BURST_FRAMES: usize = 6;

/// Source row origins (y px) for each family.
const ROCK_UNBREAKABLE_Y: u32 = 264;
const ROCK_MINEABLE_Y: u32 = 297;
const DIRT_Y: u32 = 330;
const BURST_Y: u32 = 363;
const MINER_ROW_Y: [u32; 4] = [0, 33, 66, 99]; // Down, Up, Right, Left
const DINO_ROW_Y: [u32; 4] = [132, 165, 198, 231]; // Down, Up, Right, Left

/// M4 pickup/icon cells: 24×24 each, all at x=0 (the atlas `.json` — see
/// `assets/ids.rs`). The ordering here must match the `PickupId`/`IconId`
/// enum ordering.
const ICON_CELL: u32 = 24;
const GOLD_Y: u32 = 396;
const SUPER_PICK_Y: u32 = 421;
const STICKY_SMELL_Y: u32 = 446;
const HEART_Y: u32 = 471;
const BUY_LIVES_Y: u32 = 496;
const WALK_SPEED_Y: u32 = 521;
const MINING_SPEED_Y: u32 = 546;

// M5 UI cells, all from the same atlas at their native sizes (not 32×32). The
// source rects come from the committed `My project atlas.json` (§10 of the M5
// plan). Ordering: normal / hover / pressed / disabled where applicable.
const UI_BUTTON_Y: u32 = 571;
const UI_BUTTON_W: u32 = 48;
const UI_BUTTON_H: u32 = 16;
const UI_BUTTON_XS: [u32; 4] = [0, 49, 98, 147];
const UI_PANEL_Y: u32 = 588;
const UI_PANEL_W: u32 = 64;
const UI_PANEL_H: u32 = 48;
const UI_SLIDER_Y: u32 = 637;
const UI_SLIDER_W: u32 = 96;
const UI_SLIDER_H: u32 = 24;
const UI_SLIDER_XS: [u32; 4] = [0, 97, 194, 291];
const UI_SCROLL_Y: u32 = 662;
const UI_SCROLL_W: u32 = 32;
const UI_SCROLL_H: u32 = 32;

/// Maximum texture dimension for the large standalone PNGs, to keep GPU memory
/// reasonable while staying crisp when scaled to the window.
const UI_MAX_W: u32 = 1400;
const UI_MAX_H: u32 = 800;

/// The rock terrain families (each a 16-tile Wang set, indexed by mask).
struct RockAtlas {
    /// Wang tiles for unbreakable rock, indexed by mask (0..15).
    unbreakable: Vec<Texture2D>,
    /// Wang tiles for mineable rock, indexed by mask (0..15).
    mineable: Vec<Texture2D>,
    /// The flat dirt fill.
    dirt: Texture2D,
}

impl RockAtlas {
    /// The texture for a `TerrainTile`.
    fn select(&self, sel: TerrainTile) -> &Texture2D {
        match sel {
            TerrainTile::Dirt => &self.dirt,
            TerrainTile::Mineable(m) => &self.mineable[m as usize],
            TerrainTile::Unbreakable(m) => &self.unbreakable[m as usize],
        }
    }
}

/// All textures needed by the game.
pub struct Assets {
    terrain: RockAtlas,
    player: Vec<Texture2D>,
    beast: Vec<Texture2D>,
    burst: Vec<Texture2D>,
    /// M4 pickup sprites, indexed by [`PickupId`].
    pickup: Vec<Texture2D>,
    /// M4 HUD/shop icon sprites, indexed by [`IconId`].
    icon: Vec<Texture2D>,
    /// M5 UI button states (normal / hover / pressed / disabled).
    ui_button: [Texture2D; 4],
    /// M5 base GUI panel.
    ui_panel: Texture2D,
    /// M5 adjustable-bar states (normal / hover / pressed / disabled).
    ui_slider: [Texture2D; 4],
    /// M5 scrollbar fill (slider knob/fill).
    ui_scroll: Texture2D,
    /// The scaled title logo banner.
    title_logo: Texture2D,
    /// The scaled menu backdrop.
    menu_background: Texture2D,
}

impl Assets {
    /// Load and slice the combined atlas.
    ///
    /// Terrain tiles tile seamlessly (Wang edges butt against neighbours) so are
    /// stretched to 16×16. Character frames are centered poses, so are
    /// aspect-fitted to avoid squashing.
    pub async fn load() -> Assets {
        let bytes = load_file("assets/images/My project atlas.png")
            .await
            .expect("atlas should load");
        let img = image::load_from_memory(&bytes)
            .expect("atlas should be a valid image")
            .into_rgba8();

        // Terrain: crop the 16 wang tiles (columns 1..16) + dirt base.
        let unbreakable = load_row(
            &img,
            ROCK_UNBREAKABLE_Y,
            WANG_COLS,
            1,
            layout::ScaleMode::Stretch,
        );
        let mineable = load_row(
            &img,
            ROCK_MINEABLE_Y,
            WANG_COLS,
            1,
            layout::ScaleMode::Stretch,
        );
        let dirt = load_row(&img, DIRT_Y, 1, 0, layout::ScaleMode::Stretch)
            .into_iter()
            .next()
            .expect("dirt base should slice");

        let player = load_character(&img, &MINER_ROW_Y, PLAYER_ROWS, PLAYER_COLS);
        let beast = load_character(&img, &DINO_ROW_Y, BEAST_ROWS, BEAST_COLS);
        let burst = load_row(&img, BURST_Y, BURST_FRAMES, 1, layout::ScaleMode::Fit);

        let pickup = vec![load_cell_icon(&img, GOLD_Y, layout::ScaleMode::Stretch)];
        let icon = vec![
            load_cell_icon(&img, SUPER_PICK_Y, layout::ScaleMode::Stretch),
            load_cell_icon(&img, STICKY_SMELL_Y, layout::ScaleMode::Stretch),
            load_cell_icon(&img, HEART_Y, layout::ScaleMode::Stretch),
            load_cell_icon(&img, BUY_LIVES_Y, layout::ScaleMode::Stretch),
            load_cell_icon(&img, WALK_SPEED_Y, layout::ScaleMode::Stretch),
            load_cell_icon(&img, MINING_SPEED_Y, layout::ScaleMode::Stretch),
        ];

        // M5 UI cells are sliced at native size (buttons/bars/panel have aspect
        // ratios other than 1:1, so they are not forced to the 32 px tile).
        let ui_button = load_native_rects(
            &img,
            &UI_BUTTON_XS.map(|x| layout::Rect::new(x, UI_BUTTON_Y, UI_BUTTON_W, UI_BUTTON_H)),
        )
        .try_into()
        .expect("button has 4 states");
        let ui_panel = load_native_rects(
            &img,
            &[layout::Rect::new(0, UI_PANEL_Y, UI_PANEL_W, UI_PANEL_H)],
        )
        .into_iter()
        .next()
        .expect("panel is a single cell");
        let ui_slider = load_native_rects(
            &img,
            &UI_SLIDER_XS.map(|x| layout::Rect::new(x, UI_SLIDER_Y, UI_SLIDER_W, UI_SLIDER_H)),
        )
        .try_into()
        .expect("slider has 4 states");
        let ui_scroll = load_native_rects(
            &img,
            &[layout::Rect::new(0, UI_SCROLL_Y, UI_SCROLL_W, UI_SCROLL_H)],
        )
        .into_iter()
        .next()
        .expect("scroll fill is a single cell");

        // Large standalone UI PNGs, scaled down at load.
        let title_logo =
            load_scaled_png("assets/images/ui/title_logo.png", UI_MAX_W, UI_MAX_H).await;
        let menu_background = load_scaled_png(
            "assets/images/backgrounds/menu_background.png",
            UI_MAX_W,
            UI_MAX_H,
        )
        .await;

        Assets {
            terrain: RockAtlas {
                unbreakable,
                mineable,
                dirt,
            },
            player,
            beast,
            burst,
            pickup,
            icon,
            ui_button,
            ui_panel,
            ui_slider,
            ui_scroll,
            title_logo,
            menu_background,
        }
    }

    /// The terrain texture for a cell selected by autotiling.
    pub fn tile(&self, sel: TerrainTile) -> &Texture2D {
        self.terrain.select(sel)
    }

    /// The player frame for a directional pose.
    pub fn player_anim(&self, anim: PlayerAnim) -> &Texture2D {
        &self.player[anim.row() * PLAYER_COLS + anim.col()]
    }

    /// The beast frame for a directional pose.
    pub fn beast_anim(&self, anim: BeastAnim) -> &Texture2D {
        &self.beast[anim.row() * BEAST_COLS + anim.col()]
    }

    /// The `i`-th burst frame of the rock-breaking effect.
    pub fn burst(&self, i: usize) -> &Texture2D {
        &self.burst[i % BURST_FRAMES]
    }

    /// Number of burst frames (for timing the effect).
    pub fn burst_frames(&self) -> usize {
        BURST_FRAMES
    }

    /// A pickup sprite (e.g. gold).
    pub fn pickup(&self, id: PickupId) -> &Texture2D {
        &self.pickup[id as usize]
    }

    /// A HUD/shop icon sprite.
    pub fn icon(&self, id: IconId) -> &Texture2D {
        &self.icon[id as usize]
    }

    /// A UI button frame: 0 = normal, 1 = hover, 2 = pressed, 3 = disabled.
    pub fn ui_button(&self, state: usize) -> &Texture2D {
        &self.ui_button[state % self.ui_button.len()]
    }

    /// The base GUI panel sprite.
    pub fn ui_panel(&self) -> &Texture2D {
        &self.ui_panel
    }

    /// An adjustable-bar (slider track) frame: 0 = normal, 1 = hover, 2 = pressed,
    /// 3 = disabled.
    pub fn ui_slider(&self, state: usize) -> &Texture2D {
        &self.ui_slider[state % self.ui_slider.len()]
    }

    /// The scrollbar fill (slider knob/fill) sprite.
    pub fn ui_scroll(&self) -> &Texture2D {
        &self.ui_scroll
    }

    /// The scaled title logo banner.
    pub fn title_logo(&self) -> &Texture2D {
        &self.title_logo
    }

    /// The scaled menu backdrop image.
    pub fn menu_background(&self) -> &Texture2D {
        &self.menu_background
    }
}

/// Slice a single 24×24 icon cell and resize it to the game's 32 px tile. The
/// icon source cells are 24×24 (unlike the 32 px character/terrain cells) and
/// sit in their own row of the atlas.
fn load_cell_icon(img: &image::RgbaImage, y: u32, scale_mode: layout::ScaleMode) -> Texture2D {
    let rect = layout::Rect::new(0, y, ICON_CELL, ICON_CELL);
    slice_rects(img, vec![rect], scale_mode)
        .into_iter()
        .next()
        .expect("icon cell should slice")
}

/// Crop `rects` from `img` at their **native** pixel size and upload each as a
/// texture (no 32×32 resize). Used for the M5 UI cells, which have non-square
/// aspect ratios (buttons, bars, panel).
fn load_native_rects(img: &image::RgbaImage, rects: &[layout::Rect]) -> Vec<Texture2D> {
    rects
        .iter()
        .map(|r| {
            let crop = image::imageops::crop_imm(img, r.x, r.y, r.w, r.h).to_image();
            let image = Image {
                width: r.w as u16,
                height: r.h as u16,
                bytes: crop.into_raw(),
            };
            let tex = Texture2D::from_image(&image);
            tex.set_filter(FilterMode::Nearest);
            tex
        })
        .collect()
}

/// Load a standalone PNG and downscale it to fit within `max_w`×`max_h`
/// (preserving aspect), so the large title/backdrop stay crisp without an
/// oversized GPU texture.
async fn load_scaled_png(path: &str, max_w: u32, max_h: u32) -> Texture2D {
    let bytes = load_file(path).await.expect("png should load");
    let img = image::load_from_memory(&bytes)
        .expect("png should be a valid image")
        .into_rgba8();
    let (w, h) = img.dimensions();
    let scale = (max_w as f32 / w as f32)
        .min(max_h as f32 / h as f32)
        .min(1.0);
    let nw = ((w as f32 * scale).round() as u32).max(1);
    let nh = ((h as f32 * scale).round() as u32).max(1);
    let resized = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Lanczos3);
    let image = Image {
        width: nw as u16,
        height: nh as u16,
        bytes: resized.into_raw(),
    };
    let tex = Texture2D::from_image(&image);
    tex.set_filter(FilterMode::Linear);
    tex
}

/// Slice `cols` cells from a single atlas row starting at `row_y`, optionally
/// skipping `col_offset` leading columns. Cells sit at `x = col * PITCH`.
fn load_row(
    img: &image::RgbaImage,
    row_y: u32,
    cols: usize,
    col_offset: usize,
    scale_mode: layout::ScaleMode,
) -> Vec<Texture2D> {
    let rects: Vec<layout::Rect> = (0..cols)
        .map(|i| {
            let col = col_offset + i;
            layout::Rect::new(col as u32 * PITCH, row_y, CELL, CELL)
        })
        .collect();
    slice_rects(img, rects, scale_mode)
}

/// Slice a full character sheet: one grid row per facing row origin.
fn load_character(
    img: &image::RgbaImage,
    row_ys: &[u32; 4],
    _rows: usize,
    cols: usize,
) -> Vec<Texture2D> {
    let mut out = Vec::with_capacity(row_ys.len() * cols);
    for &row_y in row_ys {
        out.extend(load_row(img, row_y, cols, 0, layout::ScaleMode::Fit));
    }
    out
}

/// Crop each rect to a 16×16 RGBA buffer and upload as a GPU texture.
fn slice_rects(
    img: &image::RgbaImage,
    rects: Vec<layout::Rect>,
    scale_mode: layout::ScaleMode,
) -> Vec<Texture2D> {
    let spec = layout::SheetSpec {
        rows: 1,
        cols: rects.len(),
        scale_mode,
        explicit_rects: Some(rects),
    };
    let frames = layout::detect_and_resize(img, &spec).expect("atlas frames should slice");
    frames.iter().map(texture_from_frame).collect()
}

fn texture_from_frame(frame: &layout::Frame) -> Texture2D {
    let image = Image {
        width: layout::TILE_SIZE as u16,
        height: layout::TILE_SIZE as u16,
        bytes: frame.rgba.clone(),
    };
    let tex = Texture2D::from_image(&image);
    // Keep pixel-art crisp when scaled by the camera zoom.
    tex.set_filter(FilterMode::Nearest);
    tex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_coords_stay_inside_sheet() {
        // The last wang column of the widest row must stay within the atlas
        // width (560). Cell origin is `col * PITCH`, cell is `CELL` wide.
        let (w, _h) = (560u32, 694u32);
        let col = 16;
        let x = col as u32 * PITCH;
        assert!(
            x + CELL <= w,
            "wang col {col} runs off sheet: x={x}+{CELL}>={w}"
        );
        assert_eq!(x, 528);
    }

    #[test]
    fn miner_row_origin_maps_expected_facing() {
        assert_eq!(MINER_ROW_Y[0], 0); // Down
        assert_eq!(MINER_ROW_Y[1], 33); // Up
        assert_eq!(MINER_ROW_Y[2], 66); // Right
        assert_eq!(MINER_ROW_Y[3], 99); // Left
    }
}
