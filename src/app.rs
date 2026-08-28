//! Application: owns the world state and drives update/draw each frame.

use macroquad::prelude::*;

use crate::assets::ids::{BeastAnim, Direction, PlayerAnim};
use crate::assets::Assets;
use crate::config::{game::GameConfig, map::MapConfig};
use crate::game::camera::Camera;
use crate::game::generation;
use crate::game::level::{Level, LevelEvent};
use crate::game::terrain;
use crate::game::TILE_SIZE;
use crate::input;

/// Default camera zoom: a 32 px tile renders at 32 px on screen (native).
const DEFAULT_ZOOM: f32 = 1.0;

/// Clear color (dark blue-grey) behind the map.
const BG_COLOR: Color = Color::new(24.0 / 255.0, 24.0 / 255.0, 34.0 / 255.0, 1.0);

/// Top-level game-state machine. M3 has Playing, LevelComplete, and GameOver
/// (all simple: single level, placeholder overlays; the real screens are M5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    Playing,
    LevelComplete,
    GameOver,
}

pub struct App {
    assets: Assets,
    level: Level,
    camera: Camera,
    state: GameState,
}

impl App {
    pub async fn new() -> App {
        // Load config + map, then build the level (which spawns the player,
        // beasts, and lives).
        let game_cfg = load_game_config().await;
        let map_cfg = load_map_config("assets/maps/level01.toml").await;
        let seed = generation::resolve_seed(&map_cfg);
        let level = Level::new(&game_cfg, &map_cfg, seed).expect("level must build a valid world");
        let camera = Camera::new(DEFAULT_ZOOM);

        App {
            assets: Assets::load().await,
            level,
            camera,
            state: GameState::Playing,
        }
    }

    /// Advance the simulation by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        // Freeze gameplay on the complete/game-over screens (M5 builds the real
        // transitions).
        if self.state != GameState::Playing {
            return;
        }

        let input = input::collect();
        let event = self.level.update(input, dt);
        match event {
            LevelEvent::Completed => self.state = GameState::LevelComplete,
            LevelEvent::GameOver => self.state = GameState::GameOver,
            // A catch auto-restarted the level inside `Level`; keep playing.
            LevelEvent::Caught | LevelEvent::None => {}
        }

        let map_w = self.level.map.width as f32 * TILE_SIZE;
        let map_h = self.level.map.height as f32 * TILE_SIZE;
        self.camera
            .follow(self.level.player.pos, map_w, map_h, screen_width(), screen_height());
    }

    /// Render one frame to the screen.
    pub fn draw(&mut self) {
        self.draw_scene(screen_width(), screen_height(), None);
        self.draw_lives();
        match self.state {
            GameState::LevelComplete => draw_level_complete_overlay(screen_width(), screen_height()),
            GameState::GameOver => draw_game_over_overlay(screen_width(), screen_height()),
            GameState::Playing => {}
        }
    }

    /// Render the scene into `fb` and leave it there for readback. Used by the
    /// `--screenshot` flag to capture a reliable image of the frame (desktop).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render_to(&mut self, fb: &RenderTarget, w: u32, h: u32) {
        self.draw_scene(w as f32, h as f32, Some(fb.clone()));
    }

    /// Clear the camera and draw the whole scene (tiles then entities).
    fn draw_scene(&mut self, view_w: f32, view_h: f32, render_target: Option<RenderTarget>) {
        set_camera(&self.scene_camera(view_w, view_h, render_target));
        clear_background(BG_COLOR);
        self.draw_tiles();
        self.draw_mining_effect();
        self.draw_beasts();
        self.draw_player();
        set_default_camera();
    }

    fn scene_camera(&self, view_w: f32, view_h: f32, render_target: Option<RenderTarget>) -> Camera2D {
        // Our Camera.zoom is a *magnification* (2.0 -> a 16px tile renders at
        // 32px). macroquad's Camera2D.zoom is instead `2 / visible_world_size`
        // in clip space, so convert. We center the visible world rect on the
        // screen/rt via `target`.
        let mag = self.camera.zoom;
        let world_w = view_w / mag;
        let world_h = view_h / mag;
        let center = self.camera.pos + Vec2::new(world_w, world_h) / 2.0;
        Camera2D {
            target: center,
            zoom: mq_zoom(mag, view_w, view_h, render_target.is_some()),
            render_target,
            ..Default::default()
        }
    }

    fn draw_tiles(&self) {
        for y in 0..self.level.map.height {
            for x in 0..self.level.map.width {
                let tile = self.level.map.tile(x as i32, y as i32);
                // Autotile: pick the terrain family + Wang tile from the cell and
                // its cardinal neighbours so rock edges blend into differing
                // materials. Dirt is always flat.
                let n = self.level.map.tile(x as i32, y as i32 - 1);
                let e = self.level.map.tile(x as i32 + 1, y as i32);
                let s = self.level.map.tile(x as i32, y as i32 + 1);
                let w = self.level.map.tile(x as i32 - 1, y as i32);
                let sel = terrain::tile_atlas(tile, n, e, s, w);
                // The Wang tile has transparent border-bevel strips that reveal
                // whatever sits beneath (e.g. dirt the rock borders). Draw that
                // underlay first so the edges blend into the ground instead of
                // showing the clear-colour background.
                if let Some(under) = terrain::underlay(tile, n, e, s, w) {
                    self.draw_tile_at(self.assets.tile(under), x as f32, y as f32);
                }
                self.draw_tile_at(self.assets.tile(sel), x as f32, y as f32);
            }
        }
    }

    fn draw_tile_at(&self, tex: &Texture2D, x: f32, y: f32) {
        draw_texture_ex(
            tex,
            x * TILE_SIZE,
            y * TILE_SIZE,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(TILE_SIZE, TILE_SIZE)),
                ..Default::default()
            },
        );
    }

    /// Draw the rock-breaking burst over the cell currently being mined.
    ///
    /// The animation is coupled to mining speed: `progress` runs 0 → 1 over
    /// `mining_time`, and the burst advances one atlas frame per step so frame 0
    /// plays at the start and the last frame plays right as the rock breaks.
    fn draw_mining_effect(&self) {
        let Some(mine) = &self.level.player.mining else {
            return;
        };
        let frames = self.assets.burst_frames();
        let progress = (mine.progress / self.level.mining_time()).clamp(0.0, 1.0);
        let frame = burst_frame(progress, frames);
        let tex = self.assets.burst(frame);
        // The burst sprite is the same size as a cell, so it covers the tile
        // exactly (origin at the tile's top-left).
        draw_texture_ex(
            tex,
            mine.target.0 as f32 * TILE_SIZE,
            mine.target.1 as f32 * TILE_SIZE,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(TILE_SIZE, TILE_SIZE)),
                ..Default::default()
            },
        );
    }

    fn draw_beasts(&self) {
        for beast in &self.level.beasts {
            let anim = BeastAnim {
                dir: beast.dir(),
                motion: beast.motion,
            };
            let tex = self.assets.beast_anim(anim);
            let offset = TILE_SIZE / 2.0;
            draw_texture_ex(
                tex,
                beast.pos.x - offset,
                beast.pos.y - offset,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(Vec2::new(TILE_SIZE, TILE_SIZE)),
                    ..Default::default()
                },
            );
        }
    }

    fn draw_player(&self) {
        let anim = PlayerAnim {
            dir: Direction::from_vec2(self.level.player.facing),
            motion: self.level.player.motion,
        };
        let tex = self.assets.player_anim(anim);
        // Center the sprite on the player's hitbox center.
        let offset = TILE_SIZE / 2.0;
        draw_texture_ex(
            tex,
            self.level.player.pos.x - offset,
            self.level.player.pos.y - offset,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(TILE_SIZE, TILE_SIZE)),
                ..Default::default()
            },
        );
    }

    /// Draw the lives counter as a small text label (the real HUD is M5).
    fn draw_lives(&self) {
        draw_text(
            &format!("Lives: {}", self.level.lives),
            14.0,
            28.0,
            24.0,
            WHITE,
        );
    }
}

/// Load and parse `assets/game.toml`.
async fn load_game_config() -> GameConfig {
    let toml = load_toml("assets/game.toml").await;
    GameConfig::from_toml(&toml).expect("assets/game.toml must be valid")
}

/// Load and parse a map TOML at `path`.
async fn load_map_config(path: &str) -> MapConfig {
    let toml = load_toml(path).await;
    MapConfig::from_toml(&toml).expect("map TOML must be valid")
}

async fn load_toml(path: &str) -> String {
    let bytes = load_file(path).await.expect("config file should load");
    String::from_utf8(bytes).expect("config should be valid UTF-8")
}

/// Draw a simple placeholder "LEVEL COMPLETE" overlay (M5 builds the real one).
fn draw_level_complete_overlay(w: f32, h: f32) {
    draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.6));
    let font: f32 = 48.0;
    let text = "LEVEL COMPLETE";
    let m = measure_text(text, None, font as u16, 1.0);
    let x = (w - m.width) / 2.0;
    let y = (h + m.height) / 2.0 - 10.0;
    draw_text(text, x, y, font, WHITE);
}

/// Draw a simple placeholder "GAME OVER" overlay (M5 builds the real one).
fn draw_game_over_overlay(w: f32, h: f32) {
    draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.6));
    let font: f32 = 48.0;
    let text = "GAME OVER";
    let m = measure_text(text, None, font as u16, 1.0);
    let x = (w - m.width) / 2.0;
    let y = (h + m.height) / 2.0 - 10.0;
    draw_text(text, x, y, font, WHITE);
}

/// Convert our camera magnification (`mag` = screen px per world px) into the
/// clip-space scale components macroquad's `Camera2D` expects.
///
/// macroquad's `Camera2D::zoom` is `2 / visible_world_size`, not a
/// magnification — using the wrong value collapses the visible region to a
/// single tile. `to_render_target` flips the Y sign because macroquad inverts
/// the Y scale for render targets internally but not for the default screen.
fn mq_zoom(mag: f32, view_w: f32, view_h: f32, to_render_target: bool) -> Vec2 {
    let world_w = view_w / mag;
    let world_h = view_h / mag;
    let zoom_y = if to_render_target { -2.0 / world_h } else { 2.0 / world_h };
    Vec2::new(2.0 / world_w, zoom_y)
}

/// The burst-frame index for a mine at `progress` (0 = start, 1 = rock breaks).
///
/// Coupled to mining speed: the frame advances linearly with progress so frame 0
/// plays at the start and the last frame (`frames - 1`) plays as the rock breaks.
fn burst_frame(progress: f32, frames: usize) -> usize {
    let p = progress.clamp(0.0, 1.0);
    ((p * frames as f32).floor() as usize).min(frames - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mq_zoom_converts_magnification_to_macroquad_scale() {
        // mag 2.0 on a 1280x720 view -> a 16px tile renders at 32px.
        let z = mq_zoom(2.0, 1280.0, 720.0, false);
        assert!((z.x - 2.0 / 640.0).abs() < 1e-5);
        assert!((z.y - 2.0 / 360.0).abs() < 1e-5);
        let zrt = mq_zoom(2.0, 1280.0, 720.0, true);
        assert_eq!(zrt.x, z.x);
        assert!((zrt.y - (-2.0 / 360.0)).abs() < 1e-5);
    }

    #[test]
    fn world_pixel_scale_equals_magnification() {
        let mag = 2.0;
        let vw = 1280.0;
        let z = mq_zoom(mag, vw, 720.0, false);
        let world_delta = 16.0;
        let screen_delta = (z.x * world_delta / 2.0) * vw;
        assert!((screen_delta - world_delta * mag).abs() < 1e-4);
    }

    #[test]
    fn burst_frame_advances_with_progress_and_holds_last() {
        let frames = 6;
        // At progress 0 the first frame plays.
        assert_eq!(burst_frame(0.0, frames), 0);
        // Advancing hits each frame in order, and the last frame is held until
        // the rock breaks (never wraps past `frames - 1`).
        assert_eq!(burst_frame(0.2, frames), 1);
        assert_eq!(burst_frame(0.5, frames), 3);
        assert_eq!(burst_frame(0.9, frames), 5);
        assert_eq!(burst_frame(1.0, frames), 5);
        // Out-of-range progress clamps.
        assert_eq!(burst_frame(-0.1, frames), 0);
        assert_eq!(burst_frame(1.5, frames), 5);
    }
}
