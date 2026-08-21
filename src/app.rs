//! Application: owns the world state and drives update/draw each frame.

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::game::camera::Camera;
use crate::game::map::{self, Map, Tile};
use crate::game::player::Player;
use crate::game::TILE_SIZE;
use crate::input;

/// Default camera zoom: a 16 px tile renders at 32 px on screen.
const DEFAULT_ZOOM: f32 = 2.0;

/// Clear color (dark blue-grey) behind the map.
const BG_COLOR: Color = Color::new(24.0 / 255.0, 24.0 / 255.0, 34.0 / 255.0, 1.0);

pub struct App {
    assets: Assets,
    map: Map,
    player: Player,
    camera: Camera,
}

impl App {
    pub async fn new() -> App {
        let map = map::placeholder_map();
        let start = map.start_pos().expect("placeholder map must have a start door");
        let player = Player::new(tile_center(start.0 as f32, start.1 as f32));
        let camera = Camera::new(DEFAULT_ZOOM);
        App {
            assets: Assets::load().await,
            map,
            player,
            camera,
        }
    }

    /// Advance the simulation by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        let intent = input::move_intent();
        self.player.update(intent, &self.map, dt);

        let map_w = self.map.width as f32 * TILE_SIZE;
        let map_h = self.map.height as f32 * TILE_SIZE;
        self.camera
            .follow(self.player.pos, map_w, map_h, screen_width(), screen_height());
    }

    /// Render one frame to the screen.
    pub fn draw(&mut self) {
        self.draw_scene(screen_width(), screen_height(), None);
    }

    /// Render the scene into `fb` and leave it there for readback. Used by the
    /// `--screenshot` flag to capture a reliable image of the frame (desktop).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render_to(&mut self, fb: &RenderTarget, w: u32, h: u32) {
        self.draw_scene(w as f32, h as f32, Some(fb.clone()));
    }

    /// Clear the camera and draw the whole scene (tiles then player).
    fn draw_scene(&mut self, view_w: f32, view_h: f32, render_target: Option<RenderTarget>) {
        set_camera(&self.scene_camera(view_w, view_h, render_target));
        clear_background(BG_COLOR);
        self.draw_tiles();
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
                let tile: Tile = self.map.tile(x as i32, y as i32);
                let tex = self.assets.tile(tile.tile_id());
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

    fn draw_player(&self) {
        let tex = self.assets.player(self.player.anim);
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

/// World-pixel center of the tile at grid coords `(tx, ty)`.
fn tile_center(tx: f32, ty: f32) -> Vec2 {
    Vec2::new(tx * TILE_SIZE + TILE_SIZE / 2.0, ty * TILE_SIZE + TILE_SIZE / 2.0)
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
        // macroquad zoom.x = 2 / world_w = 2 / (1280/2) = 2/640.
        assert!((z.x - 2.0 / 640.0).abs() < 1e-5);
        // screen (no render target): zoom.y positive.
        assert!((z.y - 2.0 / 360.0).abs() < 1e-5);
        // render target flips the Y sign.
        let zrt = mq_zoom(2.0, 1280.0, 720.0, true);
        assert_eq!(zrt.x, z.x);
        assert!((zrt.y - (-2.0 / 360.0)).abs() < 1e-5);
    }

    #[test]
    fn world_pixel_scale_equals_magnification() {
        // The effective screen scale equals the magnification: a 16px world
        // span must map to 32 screen px (macroquad NDC->screen is
        // screen_delta = zoom * world_delta * screen_w / 2).
        let mag = 2.0;
        let vw = 1280.0;
        let z = mq_zoom(mag, vw, 720.0, false);
        let world_delta = 16.0;
        let screen_delta = (z.x * world_delta / 2.0) * vw;
        assert!((screen_delta - world_delta * mag).abs() < 1e-4);
    }
}
