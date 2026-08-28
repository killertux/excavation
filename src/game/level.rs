//! Level container: owns the whole play state (map, player, beasts, gold
//! pickups, the active consumable effect) so the simulation is testable
//! **without a GPU** (pure — only `Vec2`/map/game types). Lives and cross-level
//! progress live in [`super::run::Run`]; a `Level` is a single attempt.

use macroquad::prelude::Vec2;

use super::beast::Beast;
use super::consumables::{ActiveEffect, ConsumableKind};
use super::generation;
use super::map::Map;
use super::movement;
use super::pickup::{Pickup, PickupKind};
use super::player::Player;
use super::TILE_SIZE;
use crate::audio::Sfx;
use crate::config::map::MapConfig;

/// The outcome of one level-simulation step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelEvent {
    /// Nothing notable happened; keep playing.
    None,
    /// The player reached the exit gap.
    Completed,
    /// The player was caught. The caller (`Run`) handles lives; the level must
    /// be restarted (via [`Level::restart`]) to continue if lives remain.
    Caught,
}

/// The effective tuning a level needs, computed from `game.toml` plus upgrades
/// by the caller (`Run`), so a `Level` never reads base config values directly.
#[derive(Debug, Clone, Copy)]
pub struct LevelParams {
    pub player_speed: f32,
    pub player_mining_time: f32,
    pub beast_speed: f32,
    pub beast_mining_time: f32,
    pub replan_interval: f32,
}

/// The full play state of a single level.
pub struct Level {
    pub map: Map,
    map_cfg: MapConfig,
    pub player: Player,
    pub beasts: Vec<Beast>,
    // Config-derived values, kept so a restart can re-spawn/mine correctly.
    player_speed: f32,
    player_mining_time: f32,
    beast_speed: f32,
    beast_mining_time: f32,
    replan_interval: f32,
    /// Gold gathered in the current attempt (banked by `Run` on completion).
    pub gold_collected: u32,
    /// Gold pickups waiting to be collected.
    pub pickups: Vec<Pickup>,
    /// Seconds since this attempt started (for scoring; reset on restart).
    elapsed: f32,
    /// The active consumable effect, if any (per-level; resets on restart).
    pub active_effect: Option<ActiveEffect>,
    /// Set once the level reports `Completed`. Guards against a caller
    /// re-running `update` and re-banking gold/score (the level is over).
    completed: bool,
    /// One-shot sound effects reported this frame (cleared each `update`). The
    /// caller (`Run`) drains them to the audio layer.
    sound_events: Vec<Sfx>,
}

impl Level {
    /// Build a level from a map config + generation `seed` + effective tuning.
    pub fn new(map_cfg: &MapConfig, seed: u64, params: LevelParams) -> Result<Level, generation::GenError> {
        let map = generation::generate(map_cfg, seed)?;
        let player = spawn_player(&map, params.player_speed);
        let beasts = spawn_beasts(
            &map,
            map_cfg.beast_count,
            params.beast_speed,
            params.beast_mining_time,
            params.replan_interval,
        );

        Ok(Level {
            map,
            map_cfg: map_cfg.clone(),
            player,
            beasts,
            player_speed: params.player_speed,
            player_mining_time: params.player_mining_time,
            beast_speed: params.beast_speed,
            beast_mining_time: params.beast_mining_time,
            replan_interval: params.replan_interval,
            gold_collected: 0,
            pickups: Vec::new(),
            elapsed: 0.0,
            active_effect: None,
            completed: false,
            sound_events: Vec::new(),
        })
    }

    /// The player's per-rock mining time (seconds), used to time the burst.
    pub fn mining_time(&self) -> f32 {
        self.player_mining_time
    }

    /// Seconds elapsed since this attempt started.
    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    /// Activate a consumable effect, replacing any current one. `duration` comes
    /// from `game.toml` (the caller computes it so `Level` stays pure).
    pub fn start_effect(&mut self, kind: ConsumableKind, duration: f32) {
        self.active_effect = Some(ActiveEffect { kind, remaining: duration });
    }

    /// Advance the simulation by `dt`. Returns the first significant event.
    ///
    /// Runs the player, then every beast, drops gold from any excavated cells,
    /// collects pickups, then checks for a catch (hitbox overlap), then whether
    /// the player reached the exit gap.
    pub fn update(&mut self, move_: Vec2, dt: f32) -> LevelEvent {
        // The level is over; a caller that keeps calling `update` gets nothing
        // further (prevents re-banking gold/score on a finished level).
        if self.completed {
            return LevelEvent::None;
        }
        self.elapsed += dt;
        self.sound_events.clear();
        self.tick_effect(dt);

        let super_pick = self.active_effect
            .map(|e| e.kind == ConsumableKind::SuperPick)
            .unwrap_or(false);
        self.player.super_pick = super_pick;
        self.player.update(move_, &mut self.map, self.player_mining_time, dt);
        if let Some(cell) = self.player.take_excavated() {
            self.drop_gold(cell);
        }

        let player_pos = self.player.pos;
        let sticky = self.active_effect
            .map(|e| e.kind == ConsumableKind::StickySmell)
            .unwrap_or(false);
        // Collect excavated cells, then drop gold after the loop (the beast loop
        // holds `&mut self.beasts`, so we can't also borrow `self` mutably).
        let mut excavated = Vec::new();
        for beast in &mut self.beasts {
            beast.sticky = sticky;
            beast.update(player_pos, &mut self.map, dt);
            if let Some(cell) = beast.take_excavated() {
                excavated.push(cell);
            }
        }
        for cell in excavated {
            self.drop_gold(cell);
        }

        self.collect_pickups();

        // Catch = hitbox overlap with any beast.
        for beast in &self.beasts {
            if movement::hits(self.player.pos, beast.pos) {
                return LevelEvent::Caught;
            }
        }

        // Reaching the exit gap completes the level.
        if player_on_exit(self.player.pos, self.map.exit_pos()) {
            self.completed = true;
            return LevelEvent::Completed;
        }

        LevelEvent::None
    }

    /// Decrement the active effect's remaining time; clear it when it runs out.
    fn tick_effect(&mut self, dt: f32) {
        if let Some(e) = &mut self.active_effect {
            e.remaining -= dt;
            if e.remaining <= 0.0 {
                self.active_effect = None;
            }
        }
    }

    /// If the cell at `(x, y)` hides gold, spawn a gold pickup at its centre and
    /// mark the gold as consumed (it cannot drop twice).
    fn drop_gold(&mut self, cell: (i32, i32)) {
        if cell.0 < 0 || cell.1 < 0 {
            return;
        }
        // Every rock that becomes dirt breaks (player or beast mined it).
        self.sound_events.push(Sfx::RockBreak);
        let (x, y) = (cell.0 as usize, cell.1 as usize);
        if self.map.take_gold(x, y) {
            self.pickups.push(Pickup::gold(tile_center(cell.0 as f32, cell.1 as f32)));
        }
    }

    /// Remove pickups the player's hitbox overlaps, banking their gold.
    fn collect_pickups(&mut self) {
        let player_pos = self.player.pos;
        let mut kept = Vec::with_capacity(self.pickups.len());
        let mut collected = 0;
        for p in self.pickups.drain(..) {
            if movement::hits(player_pos, p.pos) {
                if p.kind == PickupKind::Gold {
                    collected += 1;
                    self.sound_events.push(Sfx::GoldPickup);
                }
            } else {
                kept.push(p);
            }
        }
        self.gold_collected += collected;
        self.pickups = kept;
    }

    /// Take the one-shot sound effects reported since the last call.
    pub fn drain_sounds(&mut self) -> Vec<Sfx> {
        std::mem::take(&mut self.sound_events)
    }

    /// Regenerate the map with `seed` and re-spawn the player + beasts. The
    /// attempt's gold is discarded and the elapsed timer/effect reset (the
    /// caller manages lives and banked gold across catches).
    pub fn restart(&mut self, seed: u64) {
        self.map = generation::generate(&self.map_cfg, seed).expect("restart must generate a valid map");
        self.player = spawn_player(&self.map, self.player_speed);
        self.beasts = spawn_beasts(
            &self.map,
            self.map_cfg.beast_count,
            self.beast_speed,
            self.beast_mining_time,
            self.replan_interval,
        );
        self.gold_collected = 0;
        self.pickups.clear();
        self.elapsed = 0.0;
        self.active_effect = None;
        self.sound_events.clear();
        self.completed = false;
    }
}

/// World-pixel center of the tile at grid coords `(tx, ty)`.
fn tile_center(tx: f32, ty: f32) -> Vec2 {
    Vec2::new(tx * TILE_SIZE + TILE_SIZE / 2.0, ty * TILE_SIZE + TILE_SIZE / 2.0)
}

/// Spawn the player at the start gap.
fn spawn_player(map: &Map, speed: f32) -> Player {
    let (sx, sy) = map.start_pos();
    Player::new(tile_center(sx as f32, sy as f32), speed)
}

/// Spawn `count` beasts, all at the exit gap (beast 0 exactly on it, extras
/// stacked with a small per-beast offset so they are visible). `count == 0`
/// yields no beasts. "More beasts at other places" is deferred to a later
/// milestone via a `beast_spawns` list in the map TOML.
fn spawn_beasts(map: &Map, count: u32, speed: f32, mining_time: f32, replan_interval: f32) -> Vec<Beast> {
    let (ex, ey) = map.exit_pos();
    let base = tile_center(ex as f32, ey as f32);
    (0..count)
        .map(|i| {
            let offset = if i == 0 {
                Vec2::ZERO
            } else {
                Vec2::new(i as f32 * 4.0, i as f32 * 4.0)
            };
            Beast::new(base + offset, speed, mining_time, replan_interval)
        })
        .collect()
}

/// True when the player (a world-pixel position) is standing on the exit cell.
fn player_on_exit(player_pos: Vec2, exit: (usize, usize)) -> bool {
    let cell = (
        (player_pos.x / TILE_SIZE).floor() as i32,
        (player_pos.y / TILE_SIZE).floor() as i32,
    );
    cell == (exit.0 as i32, exit.1 as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::map::Tile;
    use crate::config::game::GameConfig;
    use crate::config::map::MapConfig;

    const GAME_TOML: &str = r#"
        [player]
        base_speed = 240.0
        base_mining_time = 0.8
        starting_lives = 3
        max_lives = 9

        [beast]
        base_speed = 140.0
        base_mining_time = 1.6
        replan_interval = 0.25

        [upgrades.walk_speed]
        max_level = 5
        cost_per_level = [50, 100, 200, 400, 800]
        speed_increase_per_level = 15.0

        [upgrades.mining_speed]
        max_level = 5
        cost_per_level = [50, 100, 200, 400, 800]
        mining_time_multiplier_per_level = 0.85

        [lives]
        cost = 100

        [consumables.super_pick]
        cost = 60
        duration = 3.0

        [consumables.sticky_smell]
        cost = 40
        duration = 5.0

        [score]
        par_time = 60.0
        time_multiplier = 10.0
        gold_multiplier = 5.0

        [map_order]
        files = ["assets/maps/level01.toml", "assets/maps/level02.toml"]
    "#;

    const MAP_TOML: &str = r#"
        width = 30
        height = 20
        unmineable_count = 20
        beast_count = 2
        start = { x = 15, y = 19 }
        exit  = { x = 5,  y = 0 }
        structures = [[8, 5], [9, 5], [10, 5]]
    "#;

    fn game() -> GameConfig {
        GameConfig::from_toml(GAME_TOML).expect("valid game config")
    }

    fn map_cfg(toml: &str) -> MapConfig {
        MapConfig::from_toml(toml).expect("valid map config")
    }

    /// Effective tuning for a level with no upgrades, derived from the configs.
    fn params(game: &GameConfig, map_cfg: &MapConfig) -> LevelParams {
        LevelParams {
            player_speed: game.player.base_speed,
            player_mining_time: game.player.base_mining_time,
            beast_speed: game.beast.base_speed * map_cfg.beast_speed_multiplier,
            beast_mining_time: game.beast.base_mining_time * map_cfg.beast_mining_time_multiplier,
            replan_interval: game.beast.replan_interval,
        }
    }

    /// A level seeded with a fixed seed is deterministic.
    fn level(seed: u64) -> Level {
        let game = game();
        let mc = map_cfg(MAP_TOML);
        Level::new(&mc, seed, params(&game, &mc)).expect("level builds")
    }

    #[test]
    fn spawns_expected_player_and_beasts() {
        let lv = level(12345);
        assert_eq!(lv.beasts.len(), 2, "beast_count beasts spawn");
        // Player at the start gap.
        let (sx, sy) = lv.map.start_pos();
        let pc = ((lv.player.pos.x / TILE_SIZE).floor() as i32, (lv.player.pos.y / TILE_SIZE).floor() as i32);
        assert_eq!(pc, (sx as i32, sy as i32));
        // Beasts at the exit gap.
        let (ex, ey) = lv.map.exit_pos();
        for b in &lv.beasts {
            let bc = ((b.pos.x / TILE_SIZE).floor() as i32, (b.pos.y / TILE_SIZE).floor() as i32);
            assert_eq!(bc, (ex as i32, ey as i32), "beasts spawn at the exit gap");
        }
    }

    #[test]
    fn zero_beasts_spawns_none() {
        let no_beast = map_cfg("
            width = 30
            height = 20
            unmineable_count = 20
            beast_count = 0
            start = { x = 15, y = 19 }
            exit  = { x = 5,  y = 0 }
        ");
        let game = game();
        let lv = Level::new(&no_beast, 1, params(&game, &no_beast)).expect("builds");
        assert!(lv.beasts.is_empty());
    }

    #[test]
    fn all_beasts_update_and_advance() {
        let mut lv = level(7);
        // A few frames idle: beasts re-plan and stay in bounds. The point is
        // they all run `update` without panicking and remain inside the map.
        for _ in 0..30 {
            let ev = lv.update(Vec2::ZERO, 1.0 / 60.0);
            assert!(ev == LevelEvent::None, "no event yet, got {ev:?}");
        }
        let map_w = lv.map.width as f32 * TILE_SIZE;
        let map_h = lv.map.height as f32 * TILE_SIZE;
        for b in &lv.beasts {
            assert!(b.pos.x.is_finite() && b.pos.y.is_finite());
            assert!(b.pos.x >= 0.0 && b.pos.x <= map_w);
            assert!(b.pos.y >= 0.0 && b.pos.y <= map_h);
        }
    }

    #[test]
    fn catch_returns_event_without_regenerating_map() {
        let mut lv = level(12345);
        let original_tiles = lv.map.tiles.clone();

        // Teleport (test-only) the player onto the first beast so `hits` fires.
        let b_pos = lv.beasts[0].pos;
        lv.player.pos = b_pos;

        let ev = lv.update(Vec2::ZERO, 1.0 / 60.0);
        assert_eq!(ev, LevelEvent::Caught, "caught returns a Caught event");
        assert_eq!(
            lv.map.tiles, original_tiles,
            "the level defers the restart to the caller (Run); it does not regenerate on catch"
        );
    }

    #[test]
    fn restart_respawns_and_resets_attempt_state() {
        let mut lv = level(12345);
        lv.gold_collected = 5;
        lv.pickups.push(Pickup::gold(Vec2::new(1.0, 1.0)));
        lv.start_effect(ConsumableKind::SuperPick, 3.0);
        let original_tiles = lv.map.tiles.clone();

        lv.restart(999);

        assert_ne!(lv.map.tiles, original_tiles, "a fresh map is generated");
        assert_eq!(lv.gold_collected, 0, "the attempt's gold is discarded");
        assert!(lv.pickups.is_empty(), "old pickups are cleared");
        assert!(lv.active_effect.is_none(), "the active effect resets");
        let (sx, sy) = lv.map.start_pos();
        let pc = (
            (lv.player.pos.x / TILE_SIZE).floor() as usize,
            (lv.player.pos.y / TILE_SIZE).floor() as usize,
        );
        assert_eq!(pc, (sx, sy), "player re-spawned at start");
    }

    #[test]
    fn reaching_exit_completes_the_level() {
        let no_beast = map_cfg("
            width = 30
            height = 20
            unmineable_count = 20
            beast_count = 0
            start = { x = 15, y = 19 }
            exit  = { x = 5,  y = 0 }
        ");
        let game = game();
        let mut lv = Level::new(&no_beast, 12345, params(&game, &no_beast)).expect("builds");
        let (ex, ey) = lv.map.exit_pos();
        lv.player.pos = tile_center(ex as f32, ey as f32);
        let ev = lv.update(Vec2::ZERO, 1.0 / 60.0);
        assert_eq!(ev, LevelEvent::Completed, "standing on the exit completes the level");
    }

    #[test]
    fn beast_carves_toward_player_over_time() {
        // One beast, a real generated map, and a player that stays put. The
        // beast must perceive the exit-adjacent rock, dig it, and keep carving
        // toward the player — proving the AI digs through known mineable rock.
        let cfg = map_cfg("
            width = 30
            height = 20
            unmineable_count = 20
            beast_count = 1
            start = { x = 15, y = 19 }
            exit  = { x = 5,  y = 0 }
        ");
        let game = game();
        let mut lv = Level::new(&cfg, 12345, params(&game, &cfg)).expect("builds");
        let (ex, ey) = lv.map.exit_pos();
        let rock_below_exit = (ex as i32, ey as i32 + 1);
        let dirt_before = lv.map.count(crate::game::map::Tile::Dirt);
        let player_pos = lv.player.pos;
        let spawn_dist = (lv.beasts[0].pos - player_pos).length();

        // ~20 s of a static player.
        for _ in 0..1200 {
            lv.update(Vec2::ZERO, 1.0 / 60.0);
        }

        // The rock directly below the (protected-corridor) exit was mined by the
        // beast.
        assert_eq!(
            lv.map.tile(rock_below_exit.0, rock_below_exit.1),
            crate::game::map::Tile::Dirt,
            "beast dug the rock it perceived below the exit"
        );
        assert!(
            lv.map.count(crate::game::map::Tile::Dirt) > dirt_before,
            "beast dug cells, growing the open floor"
        );
        // The beast carved closer to the player (it steps into each freed cell to
        // perceive the next diggable rock).
        let final_dist = (lv.beasts[0].pos - player_pos).length();
        assert!(
            final_dist < spawn_dist,
            "beast carved closer to the player: {final_dist} vs spawn {spawn_dist}"
        );
    }

    #[test]
    fn beast_guarding_exit_catches_before_completion() {
        // A beast physically on the exit cell catches the arriving player before
        // the level completes (the beast is the exit guard). Documents the
        // catch-before-exit ordering in `update`.
        let mut lv = level(12345);
        let (ex, ey) = lv.map.exit_pos();
        let pos = tile_center(ex as f32, ey as f32);
        lv.player.pos = pos;
        lv.beasts[0].pos = pos; // a beast sits exactly on the exit cell
        let ev = lv.update(Vec2::ZERO, 1.0 / 60.0);
        assert_eq!(ev, LevelEvent::Caught, "a guarding beast catches the player first");
    }

    #[test]
    fn mining_a_gold_rock_drops_a_pickup_and_overlap_collects_it() {
        let mut lv = level(12345);
        // Build a small clear region: player at (2,2), a gold rock below (2,3).
        lv.map.set_tile(2, 2, crate::game::map::Tile::Dirt);
        lv.map.set_tile(2, 3, crate::game::map::Tile::Mineable);
        lv.map.gold.insert((2, 3));
        lv.player.pos = tile_center(2.0, 2.0);
        lv.player.facing = Vec2::new(0.0, 1.0);

        // Super Pick: the gold rock is broken, dropping a pickup.
        lv.start_effect(ConsumableKind::SuperPick, 3.0);
        for _ in 0..30 {
            lv.update(Vec2::new(0.0, 1.0), 1.0 / 60.0);
            if !lv.pickups.is_empty() {
                break;
            }
        }
        assert_eq!(lv.pickups.len(), 1, "a gold rock drops a pickup");
        assert!(!lv.map.has_gold(2, 3), "the gold was consumed when the rock broke");

        // Collect it by moving the player onto the pickup.
        let pickup_pos = lv.pickups[0].pos;
        lv.player.pos = pickup_pos;
        lv.update(Vec2::ZERO, 1.0 / 60.0);
        assert!(lv.pickups.is_empty(), "overlap collects the pickup");
        assert_eq!(lv.gold_collected, 1, "collected gold is banked in the attempt");
    }

    #[test]
    fn super_pick_can_drop_gold_from_an_unmineable_rock() {
        let mut lv = level(12345);
        lv.map.set_tile(2, 2, crate::game::map::Tile::Dirt);
        lv.map.set_tile(2, 3, crate::game::map::Tile::Unmineable);
        lv.map.gold.insert((2, 3));
        lv.player.pos = tile_center(2.0, 2.0);
        lv.player.facing = Vec2::new(0.0, 1.0);

        lv.start_effect(ConsumableKind::SuperPick, 3.0);
        for _ in 0..30 {
            lv.update(Vec2::new(0.0, 1.0), 1.0 / 60.0);
            if !lv.pickups.is_empty() {
                break;
            }
        }
        assert_eq!(lv.map.tile(2, 3), crate::game::map::Tile::Dirt, "super pick dug the unmineable rock");
        assert_eq!(lv.pickups.len(), 1, "gold dropped from the dug unmineable rock");
    }

    #[test]
    fn breaking_a_rock_emits_rock_break_sound() {
        let mut lv = level(12345);
        lv.map.set_tile(2, 2, Tile::Dirt);
        lv.map.set_tile(2, 3, Tile::Mineable);
        lv.player.pos = tile_center(2.0, 2.0);
        lv.player.facing = Vec2::new(0.0, 1.0);
        // Super Pick: the rock breaks the first frame the player (walking down)
        // is flush against it. Take a few frames to reach the rock.
        lv.start_effect(ConsumableKind::SuperPick, 3.0);

        let mut dug = false;
        for _ in 0..30 {
            lv.update(Vec2::new(0.0, 1.0), 1.0 / 60.0);
            if lv.map.tile(2, 3) == Tile::Dirt {
                dug = true;
                break;
            }
        }
        assert!(dug, "the rock was mined");

        let sounds = lv.drain_sounds();
        assert!(sounds.contains(&Sfx::RockBreak), "a break must report RockBreak, got {sounds:?}");
        // A single break reports it exactly once.
        assert_eq!(sounds.iter().filter(|s| **s == Sfx::RockBreak).count(), 1);
    }

    #[test]
    fn collecting_gold_emits_gold_pickup_sound() {
        let mut lv = level(12345);
        lv.map.set_tile(2, 2, Tile::Dirt);
        lv.map.set_tile(2, 3, Tile::Mineable);
        lv.map.gold.insert((2, 3));
        lv.player.pos = tile_center(2.0, 2.0);
        lv.player.facing = Vec2::new(0.0, 1.0);
        lv.start_effect(ConsumableKind::SuperPick, 3.0);

        // Break the gold rock so a pickup drops, then move the player onto it.
        for _ in 0..10 {
            lv.update(Vec2::new(0.0, 1.0), 1.0 / 60.0);
            if !lv.pickups.is_empty() {
                break;
            }
        }
        assert_eq!(lv.pickups.len(), 1, "a gold pickup drops");
        lv.player.pos = lv.pickups[0].pos;
        // Drop the rock-break sound from the break frame; only collect this frame.
        lv.drain_sounds();

        lv.update(Vec2::ZERO, 1.0 / 60.0);

        let sounds = lv.drain_sounds();
        assert!(sounds.contains(&Sfx::GoldPickup), "collecting gold must report GoldPickup, got {sounds:?}");
    }

    #[test]
    fn sound_queue_clears_each_update() {
        let mut lv = level(12345);
        lv.map.set_tile(2, 2, Tile::Dirt);
        lv.map.set_tile(2, 3, Tile::Mineable);
        lv.player.pos = tile_center(2.0, 2.0);
        lv.player.facing = Vec2::new(0.0, 1.0);
        lv.start_effect(ConsumableKind::SuperPick, 3.0);
        // Break the rock, then confirm the break lands in the queue…
        let mut dug = false;
        for _ in 0..30 {
            lv.update(Vec2::new(0.0, 1.0), 1.0 / 60.0);
            if lv.map.tile(2, 3) == Tile::Dirt {
                dug = true;
                break;
            }
        }
        assert!(dug);
        assert!(!lv.drain_sounds().is_empty(), "a break happened this frame");
        // …and that the next idle frame reports nothing (the queue is per-frame).
        lv.update(Vec2::ZERO, 1.0 / 60.0);
        assert!(lv.drain_sounds().is_empty(), "a fresh frame reports no events");
    }
}
