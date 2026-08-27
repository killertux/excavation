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

use crate::assets::ids::{BeastAnim, PlayerAnim};
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
        let unbreakable = load_row(&img, ROCK_UNBREAKABLE_Y, WANG_COLS, 1, layout::ScaleMode::Stretch);
        let mineable = load_row(&img, ROCK_MINEABLE_Y, WANG_COLS, 1, layout::ScaleMode::Stretch);
        let dirt = load_row(&img, DIRT_Y, 1, 0, layout::ScaleMode::Stretch)
            .into_iter()
            .next()
            .expect("dirt base should slice");

        let player = load_character(&img, &MINER_ROW_Y, PLAYER_ROWS, PLAYER_COLS);
        let beast = load_character(&img, &DINO_ROW_Y, BEAST_ROWS, BEAST_COLS);
        let burst = load_row(&img, BURST_Y, BURST_FRAMES, 1, layout::ScaleMode::Fit);

        Assets {
            terrain: RockAtlas { unbreakable, mineable, dirt },
            player,
            beast,
            burst,
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
fn slice_rects(img: &image::RgbaImage, rects: Vec<layout::Rect>, scale_mode: layout::ScaleMode) -> Vec<Texture2D> {
    let spec = layout::SheetSpec { rows: 1, cols: rects.len(), scale_mode, explicit_rects: Some(rects) };
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
        assert!(x + CELL <= w, "wang col {col} runs off sheet: x={x}+{CELL}>={w}");
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
