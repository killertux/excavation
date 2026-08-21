//! Application: owns the world state and drives update/draw each frame.

use macroquad::prelude::*;

use crate::assets::ids::{BeastAnim, Direction, PlayerAnim};
use crate::assets::Assets;
use crate::config::{game::GameConfig, map::MapConfig};
use crate::game::beast::Beast;
use crate::game::camera::Camera;
use crate::game::generation;
use crate::game::map::Map;
use crate::game::player::Player;
use crate::game::terrain;
use crate::game::TILE_SIZE;
use crate::input;

/// Default camera zoom: a 16 px tile renders at 32 px on screen.
const DEFAULT_ZOOM: f32 = 2.0;

/// Clear color (dark blue-grey) behind the map.
const BG_COLOR: Color = Color::new(24.0 / 255.0, 24.0 / 255.0, 34.0 / 255.0, 1.0);

/// Top-level game-state machine. M2 has just Playing and a placeholder
/// LevelComplete; the real level-complete screen lands in M5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    Playing,
    LevelComplete,
}

pub struct App {
    assets: Assets,
    map: Map,
    player: Player,
    beast: Beast,
    camera: Camera,
    state: GameState,
    /// Seconds to mine one rock (from game.toml).
    mining_time: f32,
}

impl App {
    pub async fn new() -> App {
        // Load config + a map, then generate + spawn.
        let game_cfg = load_game_config().await;
        let map_cfg = load_map_config("assets/maps/level01.toml").await;
        let seed = generation::resolve_seed(&map_cfg);
        let map = generation::generate(&map_cfg, seed).expect("level01 must generate a valid map");

        let start = map.start_pos().expect("map must have a start door");
        let player = Player::new(tile_center(start.0 as f32, start.1 as f32), game_cfg.player.base_speed);
        // The beast guards the exit door until the player digs a path to it.
        let beast_pos = map
            .exit_pos()
            .map(|(x, y)| tile_center(x as f32, y as f32))
            .unwrap_or_else(|| player.pos);
        let beast = Beast::new(beast_pos);
        let camera = Camera::new(DEFAULT_ZOOM);

        App {
            assets: Assets::load().await,
            map,
            player,
            beast,
            camera,
            state: GameState::Playing,
            mining_time: game_cfg.player.base_mining_time,
        }
    }

    /// Advance the simulation by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        // In LevelComplete we freeze gameplay; the real screen is M5.
        if self.state == GameState::LevelComplete {
            return;
        }

        let input = input::collect();
        self.player
            .update(input.move_, input.mine, &mut self.map, self.mining_time, dt);
        self.beast.update(self.player.pos, &self.map, dt);

        // Win when the player's cell is the exit door.
        if let Some(exit) = self.map.exit_pos() {
            if player_on_exit(self.player.pos, exit) {
                self.state = GameState::LevelComplete;
            }
        }

        let map_w = self.map.width as f32 * TILE_SIZE;
        let map_h = self.map.height as f32 * TILE_SIZE;
        self.camera
            .follow(self.player.pos, map_w, map_h, screen_width(), screen_height());
    }

    /// Render one frame to the screen.
    pub fn draw(&mut self) {
        self.draw_scene(screen_width(), screen_height(), None);
        if self.state == GameState::LevelComplete {
            draw_level_complete_overlay(screen_width(), screen_height());
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
        self.draw_beast();
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
        for y in 0..self.map.height {
            for x in 0..self.map.width {
                let tile = self.map.tile(x as i32, y as i32);
                // Autotile: pick the atlas tile from the cell + cardinal
                // neighbours so the floor shows edges where it meets rock/wall.
                let n = self.map.tile(x as i32, y as i32 - 1);
                let e = self.map.tile(x as i32 + 1, y as i32);
                let s = self.map.tile(x as i32, y as i32 + 1);
                let w = self.map.tile(x as i32 - 1, y as i32);
                let (row, col) = terrain::tile_atlas(tile, n, e, s, w);
                let tex = self.assets.tile(row, col);
                draw_texture_ex(
                    tex,
                    x as f32 * TILE_SIZE,
                    y as f32 * TILE_SIZE,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(Vec2::new(TILE_SIZE, TILE_SIZE)),
                        ..Default::default()
                    },
                );
            }
        }
    }

    fn draw_beast(&self) {
        let anim = BeastAnim {
            dir: self.beast.dir(),
            motion: self.beast.motion,
        };
        let tex = self.assets.beast_anim(anim);
        let offset = TILE_SIZE / 2.0;
        draw_texture_ex(
            tex,
            self.beast.pos.x - offset,
            self.beast.pos.y - offset,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(TILE_SIZE, TILE_SIZE)),
                ..Default::default()
            },
        );
    }

    fn draw_player(&self) {
        let anim = PlayerAnim {
            dir: Direction::from_vec2(self.player.facing),
            motion: self.player.motion,
        };
        let tex = self.assets.player_anim(anim);
        // Center the 16×16 sprite on the player's hitbox center.
        let offset = TILE_SIZE / 2.0;
        draw_texture_ex(
            tex,
            self.player.pos.x - offset,
            self.player.pos.y - offset,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(TILE_SIZE, TILE_SIZE)),
                ..Default::default()
            },
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

/// World-pixel center of the tile at grid coords `(tx, ty)`.
fn tile_center(tx: f32, ty: f32) -> Vec2 {
    Vec2::new(tx * TILE_SIZE + TILE_SIZE / 2.0, ty * TILE_SIZE + TILE_SIZE / 2.0)
}

/// True when the player (a world-pixel position) is standing on the exit cell.
fn player_on_exit(player_pos: Vec2, exit: (usize, usize)) -> bool {
    let cell = (
        (player_pos.x / TILE_SIZE).floor() as i32,
        (player_pos.y / TILE_SIZE).floor() as i32,
    );
    cell == (exit.0 as i32, exit.1 as i32)
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
    fn player_reaches_exit_only_on_the_exit_cell() {
        // Standing at the center of the exit cell (5, 0).
        assert!(player_on_exit(tile_center(5.0, 0.0), (5, 0)));
        // Standing in an adjacent cell is not "on" the exit yet.
        assert!(!player_on_exit(tile_center(5.0, 1.0), (5, 0)));
        assert!(!player_on_exit(tile_center(4.0, 0.0), (5, 0)));
        // Slightly off-center within the exit cell still counts (floor cell).
        assert!(player_on_exit(
            Vec2::new(5.0 * TILE_SIZE + 1.0, 0.0 * TILE_SIZE + 2.0),
            (5, 0)
        ));
    }
}
