//! Runtime asset loading: slice the PNG sheets and upload them to GPU textures.
//!
//! Each sheet is a uniform `rows × cols` grid; we crop every cell and upload it
//! as a 16×16 [`Texture2D`], kept in row-major order so a `(row, col)` index
//! linearly maps to `row * cols + col`.

use macroquad::prelude::*;

use crate::assets::ids::{BeastAnim, PlayerAnim};

pub mod ids;
pub mod layout;

/// Grid dimensions of each committed sheet.
const TERRAIN_ROWS: usize = 7;
const TERRAIN_COLS: usize = 6;
const PLAYER_ROWS: usize = 4;
const PLAYER_COLS: usize = 21; // idle + walk x10 + mine x10
const BEAST_ROWS: usize = 4;
const BEAST_COLS: usize = 11; // idle + walk x10

/// All textures needed by the game, indexed by their (row, col) in each sheet.
pub struct Assets {
    terrain: Vec<Texture2D>,
    player: Vec<Texture2D>,
    beast: Vec<Texture2D>,
}

impl Assets {
    /// Load and slice every sheet.
    ///
    /// Terrain tiles tile seamlessly so they are stretched to 16×16. Character
    /// frames are centered poses, so they are aspect-fitted to avoid squashing.
    pub async fn load() -> Assets {
        let terrain = load_sheet(
            "assets/images/tiles/terrain_atlas.png",
            layout::SheetSpec::new(TERRAIN_ROWS, TERRAIN_COLS, layout::ScaleMode::Stretch),
        )
        .await;
        let player = load_sheet(
            "assets/images/characters/player_sheet.png",
            layout::SheetSpec::new(PLAYER_ROWS, PLAYER_COLS, layout::ScaleMode::Fit),
        )
        .await;
        let beast = load_sheet(
            "assets/images/characters/beast_sheet.png",
            layout::SheetSpec::new(BEAST_ROWS, BEAST_COLS, layout::ScaleMode::Fit),
        )
        .await;
        Assets { terrain, player, beast }
    }

    /// The terrain tile texture at atlas `(row, col)`.
    pub fn tile(&self, row: usize, col: usize) -> &Texture2D {
        &self.terrain[row * TERRAIN_COLS + col]
    }

    /// The player frame for a directional pose.
    pub fn player_anim(&self, anim: PlayerAnim) -> &Texture2D {
        &self.player[anim.row() * PLAYER_COLS + anim.col()]
    }

    /// The beast frame for a directional pose.
    pub fn beast_anim(&self, anim: BeastAnim) -> &Texture2D {
        &self.beast[anim.row() * BEAST_COLS + anim.col()]
    }
}

async fn load_sheet(path: &str, spec: layout::SheetSpec) -> Vec<Texture2D> {
    let bytes = load_file(path)
        .await
        .expect("asset file should load");
    let img = image::load_from_memory(&bytes)
        .expect("asset should be a valid image")
        .into_rgba8();
    let frames = layout::detect_and_resize(&img, &spec).expect("sheet should slice correctly");
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
