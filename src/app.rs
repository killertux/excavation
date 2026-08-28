//! Application: owns the run state (via [`Run`]) and drives update/draw each
//! frame.

use macroquad::prelude::*;

use crate::assets::ids::{BeastAnim, Direction, IconId, PickupId, PlayerAnim};
use crate::assets::Assets;
use crate::config::game::GameConfig;
use crate::config::map::MapConfig;
use crate::game::camera::Camera;
use crate::game::consumables::ConsumableKind;
use crate::game::run::{Run, RunEvent};
use crate::game::shop::ShopItem;
use crate::game::terrain;
use crate::game::TILE_SIZE;
use crate::input;

/// Default camera zoom: a 32 px tile renders at 32 px on screen (native).
const DEFAULT_ZOOM: f32 = 1.0;

/// Clear color (dark blue-grey) behind the map.
const BG_COLOR: Color = Color::new(24.0 / 255.0, 24.0 / 255.0, 34.0 / 255.0, 1.0);

/// The shop's purchasable items, in display order.
const SHOP_ITEMS: [ShopItem; 5] = [
    ShopItem::WalkSpeed,
    ShopItem::MiningSpeed,
    ShopItem::Lives,
    ShopItem::SuperPick,
    ShopItem::StickySmell,
];

/// Top-level game-state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    Playing,
    /// Just finished a level; shows the score/gold before the shop.
    LevelComplete,
    /// Between-level shop (buy upgrades/consumables, then continue).
    Shop,
    GameOver,
    Victory,
}

pub struct App {
    assets: Assets,
    run: Run,
    camera: Camera,
    state: GameState,
    shop_index: usize,
    last_level_score: u64,
    last_level_gold: u32,
}

impl App {
    pub async fn new() -> App {
        let game_cfg = load_game_config().await;
        let map_cfgs = load_map_configs(&game_cfg.map_order.files).await;
        let run = Run::new(game_cfg, map_cfgs).expect("run must build a valid first level");
        let camera = Camera::new(DEFAULT_ZOOM);
        App {
            assets: Assets::load().await,
            run,
            camera,
            state: GameState::Playing,
            shop_index: 0,
            last_level_score: 0,
            last_level_gold: 0,
        }
    }

    /// Advance the simulation by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        match self.state {
            GameState::Playing => self.update_playing(dt),
            GameState::LevelComplete => {
                // Acknowledge the score, then head to the shop.
                if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::Escape) {
                    self.shop_index = 0;
                    self.state = GameState::Shop;
                }
            }
            GameState::Shop => self.update_shop(),
            GameState::GameOver | GameState::Victory => {}
        }
    }

    fn update_playing(&mut self, dt: f32) {
        let input = input::collect();
        let event = self.run.update(input, dt);
        match event {
            RunEvent::Playing | RunEvent::Caught => {}
            RunEvent::LevelCompleted { score } => {
                self.last_level_score = score;
                self.last_level_gold = self.run.level.gold_collected;
                self.state = GameState::LevelComplete;
            }
            RunEvent::GameOver => self.state = GameState::GameOver,
            RunEvent::Victory => self.state = GameState::Victory,
        }

        let map_w = self.run.level.map.width as f32 * TILE_SIZE;
        let map_h = self.run.level.map.height as f32 * TILE_SIZE;
        self.camera
            .follow(self.run.level.player.pos, map_w, map_h, screen_width(), screen_height());
    }

    fn update_shop(&mut self) {
        let n = SHOP_ITEMS.len();
        if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) {
            self.shop_index = (self.shop_index + n - 1) % n;
        }
        if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
            self.shop_index = (self.shop_index + 1) % n;
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
            let item = SHOP_ITEMS[self.shop_index];
            let _ = self.run.buy(item);
        }
        if is_key_pressed(KeyCode::Escape) {
            self.advance_from_shop();
        }
    }

    fn advance_from_shop(&mut self) {
        if self.run.is_last_level() {
            self.state = GameState::Victory;
        } else {
            self.run.begin_next_level().expect("next level must build");
            self.state = GameState::Playing;
        }
    }

    /// Render one frame to the screen.
    pub fn draw(&mut self) {
        self.draw_scene(screen_width(), screen_height(), None);
        self.draw_hud();
        match self.state {
            GameState::Playing => {}
            GameState::LevelComplete => draw_level_complete_overlay(
                screen_width(),
                screen_height(),
                self.last_level_score,
                self.last_level_gold,
            ),
            GameState::Shop => draw_shop_overlay(
                &self.assets,
                screen_width(),
                screen_height(),
                &self.run,
                self.shop_index,
            ),
            GameState::GameOver => draw_game_over_overlay(screen_width(), screen_height()),
            GameState::Victory => draw_victory_overlay(screen_width(), screen_height(), self.run.score_total),
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
        self.draw_pickups();
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
        for y in 0..self.run.level.map.height {
            for x in 0..self.run.level.map.width {
                let tile = self.run.level.map.tile(x as i32, y as i32);
                // Autotile: pick the terrain family + Wang tile from the cell and
                // its cardinal neighbours so rock edges blend into differing
                // materials. Dirt is always flat.
                let n = self.run.level.map.tile(x as i32, y as i32 - 1);
                let e = self.run.level.map.tile(x as i32 + 1, y as i32);
                let s = self.run.level.map.tile(x as i32, y as i32 + 1);
                let w = self.run.level.map.tile(x as i32 - 1, y as i32);
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

    /// Draw the rock-breaking burst over any cell currently being excavated —
    /// the player's active mine, and every beast that is digging.
    ///
    /// The animation is coupled to mining speed: progress runs 0 → 1 over the
    /// dig time, and the burst advances one atlas frame per step so frame 0
    /// plays at the start and the last frame plays right as the rock breaks.
    fn draw_mining_effect(&self) {
        if let Some(mine) = &self.run.level.player.mining {
            let progress = (mine.progress / self.run.level.mining_time()).clamp(0.0, 1.0);
            self.draw_burst_at(mine.target, progress);
        }
        for beast in &self.run.level.beasts {
            if let Some((target, ratio)) = beast.dig_frame() {
                self.draw_burst_at(target, ratio.clamp(0.0, 1.0));
            }
        }
    }

    /// Draw the burst sprite centred on the cell at `target`, at `progress`.
    fn draw_burst_at(&self, target: (i32, i32), progress: f32) {
        let frames = self.assets.burst_frames();
        let frame = burst_frame(progress, frames);
        let tex = self.assets.burst(frame);
        // The burst sprite is the same size as a cell, so it covers the tile
        // exactly (origin at the tile's top-left).
        draw_texture_ex(
            tex,
            target.0 as f32 * TILE_SIZE,
            target.1 as f32 * TILE_SIZE,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(TILE_SIZE, TILE_SIZE)),
                ..Default::default()
            },
        );
    }

    /// Draw dropped pickups (gold).
    fn draw_pickups(&self) {
        for p in &self.run.level.pickups {
            let tex = self.assets.pickup(PickupId::Gold);
            let offset = TILE_SIZE / 2.0;
            draw_texture_ex(
                tex,
                p.pos.x - offset,
                p.pos.y - offset,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(Vec2::new(TILE_SIZE, TILE_SIZE)),
                    ..Default::default()
                },
            );
        }
    }

    fn draw_beasts(&self) {
        for beast in &self.run.level.beasts {
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
            dir: Direction::from_vec2(self.run.level.player.facing),
            motion: self.run.level.player.motion,
        };
        let tex = self.assets.player_anim(anim);
        // Center the sprite on the player's hitbox center.
        let offset = TILE_SIZE / 2.0;
        draw_texture_ex(
            tex,
            self.run.level.player.pos.x - offset,
            self.run.level.player.pos.y - offset,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(TILE_SIZE, TILE_SIZE)),
                ..Default::default()
            },
        );
    }

    /// Draw the simple text HUD (lives as heart icons, gold, level, active
    /// effect). The real HUD is M5.
    fn draw_hud(&self) {
        // Lives as a row of heart icons.
        for i in 0..self.run.lives {
            let tex = self.assets.icon(IconId::Heart);
            draw_texture_ex(
                tex,
                14.0 + i as f32 * 22.0,
                30.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(Vec2::new(20.0, 20.0)),
                    ..Default::default()
                },
            );
        }
        draw_text(&format!("Gold: {}", self.run.gold), 14.0, 66.0, 24.0, GOLD);
        draw_text(
            &format!("Level: {}/{}", self.run.level_index() + 1, self.run.level_count()),
            14.0,
            94.0,
            24.0,
            WHITE,
        );
        if let Some(e) = self.run.level.active_effect {
            let name = match e.kind {
                ConsumableKind::SuperPick => "Super Pick",
                ConsumableKind::StickySmell => "Sticky Smell",
            };
            draw_text(
                &format!("{name}: {:.1}s", e.remaining.max(0.0)),
                14.0,
                122.0,
                20.0,
                YELLOW,
            );
        }
    }
}

/// Load and parse `assets/game.toml`.
async fn load_game_config() -> GameConfig {
    let toml = load_toml("assets/game.toml").await;
    GameConfig::from_toml(&toml).expect("assets/game.toml must be valid")
}

/// Load every map listed in `game.toml`'s `[map_order]`.
async fn load_map_configs(files: &[String]) -> Vec<MapConfig> {
    let mut cfgs = Vec::with_capacity(files.len());
    for path in files {
        let toml = load_toml(path).await;
        let cfg = MapConfig::from_toml(&toml).expect("map TOML must be valid");
        cfgs.push(cfg);
    }
    cfgs
}

async fn load_toml(path: &str) -> String {
    let bytes = load_file(path).await.expect("config file should load");
    String::from_utf8(bytes).expect("config should be valid UTF-8")
}

/// Draw a dim rectangle overlay behind a screen.
fn draw_overlay(w: f32, h: f32) {
    draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.62));
}

/// Draw centered text (using the default macroquad font).
fn centered_text(text: &str, w: f32, y: f32, font: f32, color: Color) {
    let m = measure_text(text, None, font as u16, 1.0);
    let x = (w - m.width) / 2.0;
    draw_text(text, x, y, font, color);
}

/// The score/gold overlay shown right after a level completes.
fn draw_level_complete_overlay(w: f32, h: f32, score: u64, gold: u32) {
    draw_overlay(w, h);
    centered_text("LEVEL COMPLETE", w, 120.0, 48.0, WHITE);
    centered_text(&format!("Gold: {gold}"), w, 200.0, 28.0, GOLD);
    centered_text(&format!("Score: {score}"), w, 240.0, 28.0, WHITE);
    centered_text("Enter: continue to shop", w, h - 90.0, 20.0, LIGHTGRAY);
}

/// The between-level shop screen.
#[allow(clippy::too_many_arguments)]
fn draw_shop_overlay(assets: &Assets, w: f32, h: f32, run: &Run, selected: usize) {
    draw_overlay(w, h);
    centered_text("SHOP", w, 90.0, 44.0, WHITE);
    centered_text(&format!("Gold: {}", run.gold), w, 130.0, 24.0, GOLD);

    let left = w * 0.24;
    let start_y = 190.0;
    let line_h = 52.0;
    for (i, item) in SHOP_ITEMS.iter().enumerate() {
        let y = start_y + i as f32 * line_h;
        let cursor = if i == selected { "> " } else { "  " };
        let color = if i == selected { YELLOW } else { WHITE };
        let label = shop_label(*item, run);
        let cost = shop_cost_str(*item, run);
        draw_text(&format!("{cursor}{label}  {cost}"), left, y, 22.0, color);
        // Icon beside the selected item.
        if i == selected {
            let tex = assets.icon(shop_icon(*item));
            draw_texture_ex(
                tex,
                left + 300.0,
                y - 22.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(Vec2::new(TILE_SIZE, TILE_SIZE)),
                    ..Default::default()
                },
            );
        }
    }

    centered_text("Enter/Space: buy    Esc: continue", w, h - 70.0, 20.0, LIGHTGRAY);
}

/// A short display label for a shop item, including its owned state.
fn shop_label(item: ShopItem, run: &Run) -> String {
    match item {
        ShopItem::WalkSpeed => {
            let cfg = run.config();
            format!("Walk Speed (lv {}/{})", run.upgrades.walk_speed, cfg.upgrades.walk_speed.max_level)
        }
        ShopItem::MiningSpeed => {
            let cfg = run.config();
            format!("Mining Speed (lv {}/{})", run.upgrades.mining_speed, cfg.upgrades.mining_speed.max_level)
        }
        ShopItem::Lives => {
            let cfg = run.config();
            format!("+1 Life ({}/{})", run.lives, cfg.player.max_lives)
        }
        ShopItem::SuperPick => format!("Super Pick (x{})", run.consumables.super_pick),
        ShopItem::StickySmell => format!("Sticky Smell (x{})", run.consumables.sticky_smell),
    }
}

/// The cost string for a shop item: the number, or "MAX" when it can't be bought
/// further (upgrade maxed / lives at the cap).
fn shop_cost_str(item: ShopItem, run: &Run) -> String {
    let cost = run.item_cost(item);
    let maxed = match item {
        ShopItem::WalkSpeed => run.upgrades.walk_speed >= run.config().upgrades.walk_speed.max_level,
        ShopItem::MiningSpeed => run.upgrades.mining_speed >= run.config().upgrades.mining_speed.max_level,
        ShopItem::Lives => run.lives >= run.config().player.max_lives,
        ShopItem::SuperPick | ShopItem::StickySmell => false,
    };
    if maxed {
        "MAX".to_string()
    } else {
        format!("{cost} g")
    }
}

/// The HUD/shop icon for a shop item.
fn shop_icon(item: ShopItem) -> IconId {
    match item {
        ShopItem::WalkSpeed => IconId::WalkSpeed,
        ShopItem::MiningSpeed => IconId::MiningSpeed,
        ShopItem::Lives => IconId::BuyLives,
        ShopItem::SuperPick => IconId::SuperPick,
        ShopItem::StickySmell => IconId::StickySmell,
    }
}

/// Draw a simple placeholder "GAME OVER" overlay (M5 builds the real one).
fn draw_game_over_overlay(w: f32, h: f32) {
    draw_overlay(w, h);
    centered_text("GAME OVER", w, (h + 60.0) / 2.0 - 10.0, 48.0, WHITE);
}

/// Draw a simple placeholder "VICTORY" overlay (M5 builds the real one).
fn draw_victory_overlay(w: f32, h: f32, score_total: u64) {
    draw_overlay(w, h);
    centered_text("VICTORY", w, (h + 60.0) / 2.0 - 40.0, 48.0, WHITE);
    centered_text(&format!("Final score: {score_total}"), w, (h + 60.0) / 2.0 + 10.0, 24.0, GOLD);
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
