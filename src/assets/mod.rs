//! Runtime asset loading: slice the PNG sheets and upload them to GPU textures.
//!
//! This is the thin GPU-facing layer. The pure slicing logic lives in
//! [`layout`]; here we fetch the file bytes (portable across desktop and wasm
//! via macroquad), decode them, slice to 16×16, and upload each frame to a
//! [`Texture2D`].

use macroquad::prelude::*;

use crate::assets::ids::{PlayerAnim, TileId};

pub mod ids;
pub mod layout;

/// All textures needed by the game, indexed by their id enums.
pub struct Assets {
    terrain: Vec<Texture2D>,
    player: Vec<Texture2D>,
}

impl Assets {
    /// Load and slice every sheet used in M1.
    ///
    /// Terrain tiles are ~square, so they are stretched to 16×16. The player
    /// frames are portrait, so they are aspect-fitted (see the M1 plan risk #1:
    /// start with `Fit` for character sheets to avoid squashing).
    pub async fn load() -> Assets {
        let terrain = load_sheet(
            "assets/images/tiles/terrain_atlas.png",
            layout::SheetSpec::new(TileId::COUNT, layout::ScaleMode::Stretch),
        )
        .await;
        let player = load_sheet(
            "assets/images/characters/player_sheet.png",
            layout::SheetSpec::new(PlayerAnim::COUNT, layout::ScaleMode::Fit),
        )
        .await;
        Assets { terrain, player }
    }

    /// The terrain tile texture for the given id.
    pub fn tile(&self, id: TileId) -> &Texture2D {
        &self.terrain[id.index()]
    }

    /// The player frame texture for the given animation.
    pub fn player(&self, anim: PlayerAnim) -> &Texture2D {
        &self.player[anim.index()]
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
