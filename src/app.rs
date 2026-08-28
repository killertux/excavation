//! Application: owns the run state (via [`Run`]), the menu screen state, settings,
//! and drives update/draw each frame.
//!
//! ## M5 state machine
//!
//! ```text
//! MainMenu ─┬─ Play         → Playing   (fresh run; clears the save)
//!           ├─ Continue     → Playing   (resume the saved run)
//!           ├─ LevelSelect  → pick      → Playing
//!           ├─ Settings     → Settings
//!           └─ Quit         → exit
//! Playing ──┬─ Esc          → Paused
//!            ├─ LevelComplete → LevelComplete → Shop → next / Victory
//!            ├─ GameOver    → GameOver  → MainMenu
//!            └─ Victory     → Victory   → MainMenu
//! Paused ───┬─ Resume       → Playing
//!            ├─ Restart Level → Playing
//!            ├─ Save         → (persist) → Paused
//!            ├─ Settings     → Settings
//!            └─ Quit to Menu → (persist) → MainMenu
//! ```

use macroquad::prelude::*;

use crate::assets::ids::{BeastAnim, Direction, IconId, PickupId, PlayerAnim};
use crate::assets::Assets;
use crate::config::game::GameConfig;
use crate::config::map::MapConfig;
use crate::game::camera::Camera;
use crate::game::run::{Run, RunEvent, RunSnapshot};
use crate::game::shop::ShopItem;
use crate::game::terrain;
use crate::game::TILE_SIZE;
use crate::hud;
use crate::input;
use crate::menu::{self, Menu, MenuAction, MenuInput, MenuSource};
use crate::save::{self, SaveData};
use crate::settings::{self as settings_mod, Settings};
use crate::ui::{self, ButtonState};

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

/// The index of the selectable "Continue → next level" row in the shop (one past
/// the last item).
const SHOP_CONTINUE: usize = SHOP_ITEMS.len();

/// Top-level game-state machine (menus + gameplay).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    MainMenu,
    LevelSelect,
    Settings,
    Paused,
    Playing,
    /// Just finished a level; shows the score/gold before the shop.
    LevelComplete,
    /// Between-level shop (buy upgrades/consumables, then continue).
    Shop,
    GameOver,
    Victory,
}

/// A gameplay screen's drawing mode (drives the HUD gold figure and overlays).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameplayDraw {
    Playing,
    Paused,
    LevelComplete,
    Shop,
    GameOver,
    Victory,
}

pub struct App {
    assets: Assets,
    game_config: GameConfig,
    map_configs: Vec<MapConfig>,
    run: Run,
    camera: Camera,
    state: GameState,
    menu: Menu,
    settings: Settings,
    /// The last run loaded/saved to disk (drives the "Continue" button).
    saved_run: Option<RunSnapshot>,
    shop_index: usize,
    last_level_score: u64,
    last_level_gold: u32,
}

impl App {
    pub async fn new() -> App {
        let game_config = load_game_config().await;
        let map_configs = load_map_configs(&game_config.map_order.files).await;
        let assets = Assets::load().await;

        // Load a save, if any, into settings + a resumed run; otherwise default.
        let (settings, saved_run, run) = match save::load() {
            Some(save) => {
                let mut settings = save.settings;
                settings.clamp();
                let run = Run::resume(game_config.clone(), map_configs.clone(), save.run)
                    .expect("saved run must resume");
                (settings, Some(save.run), run)
            }
            None => (
                Settings::default(),
                None,
                Run::new(game_config.clone(), map_configs.clone())
                    .expect("run must build a valid first level"),
            ),
        };

        let camera = Camera::new(DEFAULT_ZOOM);
        // Apply the persisted fullscreen setting at startup.
        macroquad::window::set_fullscreen(settings.fullscreen);

        let mut app = App {
            assets,
            game_config,
            map_configs,
            run,
            camera,
            state: GameState::MainMenu,
            menu: Menu::Main(menu::MainMenu::new(saved_run.is_some())),
            settings,
            saved_run,
            shop_index: 0,
            last_level_score: 0,
            last_level_gold: 0,
        };
        #[cfg(not(target_arch = "wasm32"))]
        app.apply_debug_screen();
        app
    }

    /// Advance the simulation by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        match self.state {
            GameState::MainMenu | GameState::LevelSelect | GameState::Settings | GameState::Paused => {
                self.update_menu(dt)
            }
            GameState::Playing => self.update_playing(dt),
            GameState::LevelComplete => self.update_level_complete(),
            GameState::Shop => self.update_shop(),
            GameState::GameOver | GameState::Victory => self.update_result(),
        }
    }

    // ---- Menu layer ------------------------------------------------------

    /// Collect menu input and run it through the active screen.
    fn update_menu(&mut self, _dt: f32) {
        let action = self.menu.update(&menu_input());
        self.apply_action(action);
    }

    /// Execute a menu action.
    fn apply_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::None => {}
            MenuAction::NewGame => {
                self.run =
                    Run::new(self.game_config.clone(), self.map_configs.clone()).expect("new run builds");
                self.saved_run = None;
                save::clear();
                self.state = GameState::Playing;
                self.follow_camera();
            }
            MenuAction::Continue => {
                if let Some(snap) = self.saved_run {
                    self.run = Run::resume(self.game_config.clone(), self.map_configs.clone(), snap)
                        .expect("saved run resumes");
                    self.state = GameState::Playing;
                    self.follow_camera();
                }
            }
            MenuAction::OpenLevelSelect => {
                self.menu = Menu::LevelSelect(menu::LevelSelect::new(self.map_configs.len(), self.run.unlocked()));
                self.state = GameState::LevelSelect;
            }
            MenuAction::OpenSettings => {
                let source = if self.state == GameState::Paused { MenuSource::Pause } else { MenuSource::Main };
                self.menu = Menu::Settings(menu::SettingsScreen::new(source));
                self.state = GameState::Settings;
            }
            MenuAction::Back => self.handle_back(),
            MenuAction::StartLevel(i) => {
                self.run.start_level(i).expect("selected level builds");
                self.state = GameState::Playing;
                self.follow_camera();
            }
            MenuAction::ToggleFullscreen => {
                self.settings.fullscreen = !self.settings.fullscreen;
                macroquad::window::set_fullscreen(self.settings.fullscreen);
                self.maybe_persist_settings();
            }
            MenuAction::VolumeUpMusic => {
                self.settings.music_volume = settings_mod::volume_step(self.settings.music_volume, 0.1);
                self.maybe_persist_settings();
            }
            MenuAction::VolumeDownMusic => {
                self.settings.music_volume = settings_mod::volume_step(self.settings.music_volume, -0.1);
                self.maybe_persist_settings();
            }
            MenuAction::VolumeUpSfx => {
                self.settings.sfx_volume = settings_mod::volume_step(self.settings.sfx_volume, 0.1);
                self.maybe_persist_settings();
            }
            MenuAction::VolumeDownSfx => {
                self.settings.sfx_volume = settings_mod::volume_step(self.settings.sfx_volume, -0.1);
                self.maybe_persist_settings();
            }
            MenuAction::Resume => self.state = GameState::Playing,
            MenuAction::RestartLevel => {
                self.run.restart_current_level();
                self.state = GameState::Playing;
                self.follow_camera();
            }
            MenuAction::Save => {
                self.persist();
                self.state = GameState::Paused;
            }
            MenuAction::SaveAndQuitToMenu => {
                self.persist();
                self.state = GameState::MainMenu;
                self.menu = Menu::Main(menu::MainMenu::new(self.saved_run.is_some()));
            }
            MenuAction::Quit => quit(),
        }
    }

    /// Leave the current menu screen, returning to where it was opened from.
    fn handle_back(&mut self) {
        match &self.menu {
            Menu::Settings(s) if s.source == MenuSource::Pause => {
                self.state = GameState::Paused;
                self.menu = Menu::Pause(menu::Pause::new());
            }
            Menu::Settings(_) => self.to_main_menu(),
            Menu::LevelSelect(_) => self.to_main_menu(),
            _ => {}
        }
    }

    fn to_main_menu(&mut self) {
        self.state = GameState::MainMenu;
        self.menu = Menu::Main(menu::MainMenu::new(self.saved_run.is_some()));
    }

    /// Persist the current run snapshot + settings to disk.
    fn persist(&mut self) {
        let snap = self.run.snapshot();
        let save = SaveData { version: save::SAVE_VERSION, run: snap, settings: self.settings };
        save::save(&save);
        self.saved_run = Some(snap);
    }

    /// Persist a settings change, but only when a save already exists. This avoids
    /// fabricating a phantom "Continue" from a stale run (e.g. the fresh-boot
    /// placeholder run, or a finished Victory/GameOver run) when the player merely
    /// tweaks volume/fullscreen from the main menu. Settings are still applied
    /// live and will be written at the next explicit Save / Quit to Menu.
    fn maybe_persist_settings(&mut self) {
        if self.saved_run.is_some() {
            self.persist();
        }
    }

    // ---- Gameplay --------------------------------------------------------

    fn update_playing(&mut self, dt: f32) {
        if is_key_pressed(KeyCode::Escape) {
            self.state = GameState::Paused;
            self.menu = Menu::Pause(menu::Pause::new());
            return;
        }
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
        self.follow_camera();
    }

    fn update_level_complete(&mut self) {
        // Acknowledge the score, then head to the shop.
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::Escape) {
            self.shop_index = 0;
            self.state = GameState::Shop;
        }
    }

    fn update_shop(&mut self) {
        // The shop is the items plus a selectable "Continue" row (SHOP_CONTINUE).
        let n = SHOP_ITEMS.len() + 1;
        if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) {
            self.shop_index = (self.shop_index + n - 1) % n;
        }
        if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
            self.shop_index = (self.shop_index + 1) % n;
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
            if self.shop_index == SHOP_CONTINUE {
                self.advance_from_shop();
            } else {
                let item = SHOP_ITEMS[self.shop_index];
                let _ = self.run.buy(item);
            }
        }
        // Esc is always a shortcut to leave the shop.
        if is_key_pressed(KeyCode::Escape) {
            self.advance_from_shop();
        }
    }

    fn update_result(&mut self) {
        // Leaving the result screen returns to the main menu (run is kept for
        // level-select replay; nothing is auto-saved here).
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::Escape) {
            self.to_main_menu();
        }
    }

    fn advance_from_shop(&mut self) {
        if self.run.is_last_level() {
            self.state = GameState::Victory;
        } else {
            self.run.begin_next_level().expect("next level must build");
            self.state = GameState::Playing;
            self.follow_camera();
        }
    }

    fn follow_camera(&mut self) {
        let map_w = self.run.level.map.width as f32 * TILE_SIZE;
        let map_h = self.run.level.map.height as f32 * TILE_SIZE;
        self.camera
            .follow(self.run.level.player.pos, map_w, map_h, screen_width(), screen_height());
    }

    // ---- Rendering -------------------------------------------------------

    /// Draw the current frame to the (live) screen.
    pub fn draw(&mut self) {
        self.draw_frame(screen_width(), screen_height(), None);
    }

    /// Render one frame into `fb` for the `--screenshot` flag (desktop).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render_to(&mut self, fb: &RenderTarget, w: u32, h: u32) {
        self.draw_frame(w as f32, h as f32, Some(fb.clone()));
    }

    fn draw_frame(&mut self, view_w: f32, view_h: f32, rt: Option<RenderTarget>) {
        match self.state {
            GameState::MainMenu => self.draw_main_menu(view_w, view_h, rt),
            GameState::LevelSelect => self.draw_level_select(view_w, view_h, rt),
            GameState::Settings => self.draw_settings(view_w, view_h, rt),
            GameState::Paused => self.draw_gameplay(view_w, view_h, rt, GameplayDraw::Paused),
            GameState::Playing => self.draw_gameplay(view_w, view_h, rt, GameplayDraw::Playing),
            GameState::LevelComplete => self.draw_gameplay(view_w, view_h, rt, GameplayDraw::LevelComplete),
            GameState::Shop => self.draw_gameplay(view_w, view_h, rt, GameplayDraw::Shop),
            GameState::GameOver => self.draw_gameplay(view_w, view_h, rt, GameplayDraw::GameOver),
            GameState::Victory => self.draw_gameplay(view_w, view_h, rt, GameplayDraw::Victory),
        }
    }

    /// A screen-space camera that can target `rt` (for overlays/HUD/menus).
    ///
    /// For a render target, uses the "display rect" camera (top-left origin,
    /// matching the proven `mq_zoom` convention) so the readback is upright; for
    /// the live screen, resets to the default camera (as before M5).
    fn set_screen_camera(rt: Option<RenderTarget>, view_w: f32, view_h: f32) {
        if let Some(rt) = rt {
            set_camera(&Camera2D {
                target: Vec2::new(view_w / 2.0, view_h / 2.0),
                zoom: Vec2::new(2.0 / view_w, -2.0 / view_h),
                render_target: Some(rt),
                ..Default::default()
            });
        } else {
            set_default_camera();
        }
    }

    /// Draw a gameplay state: the world scene, then the HUD (+ any overlay).
    fn draw_gameplay(&mut self, view_w: f32, view_h: f32, rt: Option<RenderTarget>, mode: GameplayDraw) {
        self.draw_scene(view_w, view_h, rt.clone());
        Self::set_screen_camera(rt.clone(), view_w, view_h);

        // Gold figure: live in-attempt gold while playing or paused; banked total
        // on the outcome screens (the attempt is over).
        let gold_display = if matches!(mode, GameplayDraw::Playing | GameplayDraw::Paused) {
            self.run.level.gold_collected
        } else {
            self.run.gold
        };
        let show_timer = mode == GameplayDraw::Playing;
        hud::draw_hud(&self.assets, &self.run, gold_display, show_timer, view_w, view_h);

        match mode {
            GameplayDraw::Playing | GameplayDraw::Paused => {}
            GameplayDraw::LevelComplete => draw_level_complete_overlay(
                view_w,
                view_h,
                self.last_level_score,
                self.last_level_gold,
                self.run.score_total,
            ),
            GameplayDraw::Shop => draw_shop_overlay(&self.assets, view_w, view_h, &self.run, self.shop_index),
            GameplayDraw::GameOver => draw_game_over_overlay(view_w, view_h),
            GameplayDraw::Victory => draw_victory_overlay(view_w, view_h, self.run.score_total),
        }
        if mode == GameplayDraw::Paused {
            self.draw_pause_overlay(view_w, view_h);
        }
        set_default_camera();
    }

    // ---- Menu screens ----------------------------------------------------

    fn draw_main_menu(&self, w: f32, h: f32, rt: Option<RenderTarget>) {
        Self::set_screen_camera(rt, w, h);
        clear_background(BG_COLOR);
        self.draw_texture_full(self.assets.menu_background(), w, h);
        self.draw_title(w, 70.0);

        let (items, selection) = match &self.menu {
            Menu::Main(m) => (m.items(), m.selection),
            _ => (vec![], 0),
        };
        let labels: Vec<String> = items.iter().map(|it| main_menu_label(*it)).collect();
        self.draw_button_column(&labels, selection, w, 250.0, true);

        centered_text("Up/Down: select    Enter: activate", w, h - 40.0, 18.0, LIGHTGRAY);
        set_default_camera();
    }

    fn draw_level_select(&self, w: f32, h: f32, rt: Option<RenderTarget>) {
        Self::set_screen_camera(rt, w, h);
        clear_background(BG_COLOR);
        self.draw_texture_full(self.assets.menu_background(), w, h);
        centered_text("SELECT LEVEL", w, 140.0, 48.0, WHITE);

        let (selection, count, unlocked) = match &self.menu {
            Menu::LevelSelect(ls) => (ls.selection, ls.level_count, ls.unlocked),
            _ => (0, 0, 0),
        };
        let start_y = 220.0;
        for i in 0..count {
            let y = start_y + i as f32 * 56.0;
            let locked = i + 1 > unlocked;
            let state = if locked {
                ButtonState::Disabled
            } else if i == selection {
                ButtonState::Hover
            } else {
                ButtonState::Normal
            };
            let btn_w = 340.0;
            let btn_h = 46.0;
            ui::draw_button(&self.assets, Rect::new((w - btn_w) / 2.0, y, btn_w, btn_h), state);
            let label = if locked {
                format!("Level {}  (locked)", i + 1)
            } else {
                format!("Level {}", i + 1)
            };
            let color = if locked {
                GRAY
            } else if i == selection {
                YELLOW
            } else {
                WHITE
            };
            centered_text(&label, w, y + btn_h / 2.0 + 8.0, 22.0, color);
        }

        centered_text("Enter: play    Esc: back", w, h - 40.0, 18.0, LIGHTGRAY);
        set_default_camera();
    }

    fn draw_settings(&self, w: f32, h: f32, rt: Option<RenderTarget>) {
        Self::set_screen_camera(rt, w, h);
        clear_background(BG_COLOR);
        self.draw_texture_full(self.assets.menu_background(), w, h);
        centered_text("SETTINGS", w, 140.0, 48.0, WHITE);

        let selection = match &self.menu {
            Menu::Settings(s) => s.selection,
            _ => 0,
        };
        let start_y = 220.0;
        let line_h = 84.0;

        let music_state = if selection == 0 { ButtonState::Hover } else { ButtonState::Normal };
        draw_text("Music Volume", w * 0.22, start_y + 34.0, 24.0, WHITE);
        ui::draw_slider(&self.assets, Rect::new(w * 0.55, start_y + 12.0, w * 0.32, 36.0), self.settings.music_volume, music_state);

        let sfx_state = if selection == 1 { ButtonState::Hover } else { ButtonState::Normal };
        draw_text("SFX Volume", w * 0.22, start_y + line_h + 34.0, 24.0, WHITE);
        ui::draw_slider(&self.assets, Rect::new(w * 0.55, start_y + line_h + 12.0, w * 0.32, 36.0), self.settings.sfx_volume, sfx_state);

        // Fullscreen toggle row.
        let fs_state = if selection == 2 { ButtonState::Hover } else { ButtonState::Normal };
        draw_text("Fullscreen", w * 0.22, start_y + 2.0 * line_h + 34.0, 24.0, WHITE);
        let btn_w = 200.0;
        let btn_x = w * 0.55;
        ui::draw_button(&self.assets, Rect::new(btn_x, start_y + 2.0 * line_h + 12.0, btn_w, 40.0), fs_state);
        let label = if self.settings.fullscreen { "ON" } else { "OFF" };
        let color = if self.settings.fullscreen { GREEN } else { LIGHTGRAY };
        centered_text(label, btn_x + btn_w, start_y + 2.0 * line_h + 12.0 + 32.0, 24.0, color);

        // Back row.
        let back_state = if selection == 3 { ButtonState::Hover } else { ButtonState::Normal };
        ui::draw_button(&self.assets, Rect::new(w / 2.0 - 170.0, start_y + 3.0 * line_h, 340.0, 46.0), back_state);
        centered_text("Back", w, start_y + 3.0 * line_h + 31.0, 24.0, if selection == 3 { YELLOW } else { WHITE });

        centered_text("Left/Right: volume   Enter: toggle   Esc: back", w, h - 40.0, 18.0, LIGHTGRAY);
        set_default_camera();
    }

    /// Draw a centered column of buttons (used by the main menu & pause overlay).
    fn draw_button_column(&self, labels: &[String], selected: usize, w: f32, start_y: f32, panel: bool) {
        let btn_w = 340.0;
        let btn_h = 46.0;
        let line_h = 58.0;
        let total = labels.len() as f32 * line_h;
        let cx = (w - btn_w) / 2.0;
        if panel {
            let panel = Rect::new(cx - 24.0, start_y - 28.0, btn_w + 48.0, total + 40.0);
            ui::draw_panel(&self.assets, panel);
        }
        for (i, label) in labels.iter().enumerate() {
            let y = start_y + i as f32 * line_h;
            let state = if i == selected { ButtonState::Hover } else { ButtonState::Normal };
            ui::draw_button(&self.assets, Rect::new(cx, y, btn_w, btn_h), state);
            let color = if i == selected { YELLOW } else { WHITE };
            centered_text(label, w, y + btn_h / 2.0 + 8.0, 24.0, color);
        }
    }

    fn draw_pause_overlay(&self, w: f32, h: f32) {
        draw_overlay(w, h);
        centered_text("PAUSED", w, 110.0, 44.0, WHITE);
        let selection = match &self.menu {
            Menu::Pause(p) => p.selection,
            _ => 0,
        };
        let labels = ["Resume", "Restart Level", "Save", "Settings", "Quit to Menu"];
        let label_refs: Vec<String> = labels.iter().map(|s| s.to_string()).collect();
        self.draw_button_column(&label_refs, selection, w, 170.0, true);
        centered_text("Esc: resume", w, h - 40.0, 18.0, LIGHTGRAY);
    }

    /// Draw a backdrop image scaled to the whole window.
    fn draw_texture_full(&self, tex: &Texture2D, w: f32, h: f32) {
        draw_texture_ex(
            tex,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(w, h)),
                ..Default::default()
            },
        );
    }

    /// Draw the title logo as a banner centered horizontally at `start_y`.
    fn draw_title(&self, w: f32, start_y: f32) {
        let logo = self.assets.title_logo();
        let title_w = (w * 0.7).min(880.0);
        let title_h = title_w * (logo.height() / logo.width());
        draw_texture_ex(
            logo,
            (w - title_w) / 2.0,
            start_y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(title_w, title_h)),
                ..Default::default()
            },
        );
    }

    // ---- Scene (unchanged from M4) ---------------------------------------

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
            let anim = BeastAnim { dir: beast.dir(), motion: beast.motion };
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

    // ---- Debug screenshot helper ----------------------------------------

    /// Force an initial state for `DSH_SCREEN=<name>` (desktop visual checks).
    #[cfg(not(target_arch = "wasm32"))]
    fn apply_debug_screen(&mut self) {
        let Ok(name) = std::env::var("DSH_SCREEN") else { return };
        match name.as_str() {
            "mainmenu" | "menu" => {
                self.state = GameState::MainMenu;
                self.menu = Menu::Main(menu::MainMenu::new(self.saved_run.is_some()));
            }
            "levelselect" => {
                self.state = GameState::LevelSelect;
                self.menu = Menu::LevelSelect(menu::LevelSelect::new(self.map_configs.len(), self.run.unlocked()));
            }
            "settings" => {
                self.state = GameState::Settings;
                self.menu = Menu::Settings(menu::SettingsScreen::new(MenuSource::Main));
            }
            "pause" | "paused" => {
                self.state = GameState::Paused;
                self.menu = Menu::Pause(menu::Pause::new());
            }
            "shop" => {
                self.state = GameState::Shop;
                self.shop_index = 0;
            }
            "playing" => self.state = GameState::Playing,
            "levelcomplete" => self.state = GameState::LevelComplete,
            "gameover" => self.state = GameState::GameOver,
            "victory" => self.state = GameState::Victory,
            _ => {}
        }
    }
}

// ---- Free helpers (loading, overlays, text) ------------------------------

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

/// Collect the current frame's edge-triggered menu input.
fn menu_input() -> MenuInput {
    MenuInput {
        up: is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W),
        down: is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S),
        left: is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::A),
        right: is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::D),
        enter: is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space),
        escape: is_key_pressed(KeyCode::Escape),
    }
}

/// Exit the process (desktop) or request the window to quit (wasm best-effort).
fn quit() {
    #[cfg(not(target_arch = "wasm32"))]
    std::process::exit(0);
    #[cfg(target_arch = "wasm32")]
    macroquad::miniquad::window::quit();
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

/// A short display label for a main-menu entry.
fn main_menu_label(item: menu::MainMenuItem) -> String {
    match item {
        menu::MainMenuItem::Play => "Play".into(),
        menu::MainMenuItem::Continue => "Continue".into(),
        menu::MainMenuItem::LevelSelect => "Level Select".into(),
        menu::MainMenuItem::Settings => "Settings".into(),
        menu::MainMenuItem::Quit => "Quit".into(),
    }
}

/// The score/gold overlay shown right after a level completes.
fn draw_level_complete_overlay(w: f32, h: f32, score: u64, gold: u32, score_total: u64) {
    draw_overlay(w, h);
    centered_text("LEVEL COMPLETE", w, 120.0, 48.0, WHITE);
    centered_text(&format!("Gold this level: {gold}"), w, 200.0, 28.0, GOLD);
    centered_text(&format!("Score this level: {score}"), w, 240.0, 28.0, WHITE);
    centered_text(&format!("Total score: {score_total}"), w, 276.0, 28.0, LIGHTGRAY);
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

    // The final, selectable "Continue → next level" row.
    let cy = start_y + SHOP_ITEMS.len() as f32 * line_h;
    let cursor = if selected == SHOP_CONTINUE { "> " } else { "  " };
    let color = if selected == SHOP_CONTINUE { YELLOW } else { WHITE };
    draw_text(&format!("{cursor}Continue  ->  next level"), left, cy, 22.0, color);

    centered_text("Enter/Space: buy/continue    Esc: continue", w, h - 70.0, 20.0, LIGHTGRAY);
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

/// Draw the "GAME OVER" overlay.
fn draw_game_over_overlay(w: f32, h: f32) {
    draw_overlay(w, h);
    centered_text("GAME OVER", w, (h + 60.0) / 2.0 - 10.0, 48.0, WHITE);
    centered_text("Enter: menu", w, (h + 60.0) / 2.0 + 40.0, 20.0, LIGHTGRAY);
}

/// Draw the "VICTORY" overlay.
fn draw_victory_overlay(w: f32, h: f32, score_total: u64) {
    draw_overlay(w, h);
    centered_text("VICTORY", w, (h + 60.0) / 2.0 - 40.0, 48.0, WHITE);
    centered_text(&format!("Final score: {score_total}"), w, (h + 60.0) / 2.0 + 10.0, 24.0, GOLD);
    centered_text("Enter: menu", w, (h + 60.0) / 2.0 + 50.0, 20.0, LIGHTGRAY);
}

/// Convert our camera magnification (`mag` = screen px per world px) into the
/// clip-space scale components macroquad's `Camera2D` expects.
fn mq_zoom(mag: f32, view_w: f32, view_h: f32, to_render_target: bool) -> Vec2 {
    let world_w = view_w / mag;
    let world_h = view_h / mag;
    let zoom_y = if to_render_target { -2.0 / world_h } else { 2.0 / world_h };
    Vec2::new(2.0 / world_w, zoom_y)
}

/// The burst-frame index for a mine at `progress` (0 = start, 1 = rock breaks).
fn burst_frame(progress: f32, frames: usize) -> usize {
    let p = progress.clamp(0.0, 1.0);
    ((p * frames as f32).floor() as usize).min(frames - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mq_zoom_converts_magnification_to_macroquad_scale() {
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
        assert_eq!(burst_frame(0.0, frames), 0);
        assert_eq!(burst_frame(0.2, frames), 1);
        assert_eq!(burst_frame(0.5, frames), 3);
        assert_eq!(burst_frame(0.9, frames), 5);
        assert_eq!(burst_frame(1.0, frames), 5);
        assert_eq!(burst_frame(-0.1, frames), 0);
        assert_eq!(burst_frame(1.5, frames), 5);
    }
}
