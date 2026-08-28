//! Cross-level run state: gold, upgrades, consumables, lives, score, and level
//! progression. A `Run` owns a single [`Level`] and drives it; it is pure and
//! testable (no GPU, no input polling — the caller feeds it an [`Input`]).

use serde::{Deserialize, Serialize};

use crate::audio::Sfx;
use crate::config::game::GameConfig;
use crate::config::map::MapConfig;
use crate::game::consumables::{self, ConsumableKind, Consumables};
use crate::game::generation;
use crate::game::level::{Level, LevelEvent, LevelParams};
use crate::game::score;
use crate::game::shop::{self, ShopError, ShopItem};
use crate::game::upgrades::{self, Upgrades};
use crate::input::Input;

/// The outcome of one run-simulation step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEvent {
    /// Nothing notable; keep playing.
    Playing,
    /// The player was caught; a life was spent and the level restarted.
    Caught,
    /// The level was completed; `score` is the freshly earned level score.
    LevelCompleted { score: u64 },
    /// The player ran out of lives.
    GameOver,
    /// Every level in `map_order` was completed.
    Victory,
}

/// The cross-level state a save/load preserves (the persistent part of a run).
/// It deliberately excludes per-level simulation state (map, positions, pickups,
/// elapsed, active effect): on load the level at `level_index` is rebuilt fresh.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RunSnapshot {
    pub gold: u32,
    pub upgrades: Upgrades,
    pub consumables: Consumables,
    pub lives: u32,
    pub score_total: u64,
    pub level_index: usize,
    /// The highest selectable level (1-based); a level is selectable when its
    /// 1-based index is `<= unlocked`.
    pub unlocked: usize,
}

/// The whole run: persistent cross-level state plus the current level.
pub struct Run {
    pub gold: u32,
    pub upgrades: Upgrades,
    pub consumables: Consumables,
    pub lives: u32,
    pub score_total: u64,
    level_index: usize,
    /// Highest selectable level (1-based), unlocked by advancing past it.
    pub unlocked: usize,
    map_cfgs: Vec<MapConfig>,
    cfg: GameConfig,
    pub level: Level,
    /// One-shot sound effects reported since the last drain: this run's own
    /// (consumable activations) plus the level's (rock breaks, gold pickups).
    sound_events: Vec<Sfx>,
}

impl Run {
    /// Start a fresh run on the first map, with starting lives and no upgrades.
    pub fn new(cfg: GameConfig, map_cfgs: Vec<MapConfig>) -> Result<Run, generation::GenError> {
        debug_assert!(!map_cfgs.is_empty(), "map_order must list at least one level");
        let map_cfg = map_cfgs[0].clone();
        let level = build_level(&cfg, &map_cfg, &Upgrades::default())?;
        Ok(Run {
            gold: 0,
            upgrades: Upgrades::default(),
            consumables: Consumables::default(),
            lives: cfg.player.starting_lives,
            score_total: 0,
            level_index: 0,
            unlocked: 1,
            map_cfgs,
            cfg,
            level,
            sound_events: Vec::new(),
        })
    }

    /// The 0-based index of the level being played.
    pub fn level_index(&self) -> usize {
        self.level_index
    }

    /// The number of levels in the run.
    pub fn level_count(&self) -> usize {
        self.map_cfgs.len()
    }

    /// Whether the current level is the last one in `map_order`.
    pub fn is_last_level(&self) -> bool {
        self.level_index + 1 >= self.map_cfgs.len()
    }

    /// The highest selectable level (1-based). Levels `1..=unlocked` are unlocked.
    pub fn unlocked(&self) -> usize {
        self.unlocked
    }

    /// Snapshot the run-level state for saving (excludes per-level sim state).
    pub fn snapshot(&self) -> RunSnapshot {
        RunSnapshot {
            gold: self.gold,
            upgrades: self.upgrades,
            consumables: self.consumables,
            lives: self.lives,
            score_total: self.score_total,
            level_index: self.level_index,
            unlocked: self.unlocked,
        }
    }

    /// Resume a run from a snapshot, building the level at `snapshot.level_index`
    /// with the saved upgrades. The level index is clamped to the available maps
    /// so an invalid (e.g. truncated) save never panics.
    pub fn resume(cfg: GameConfig, map_cfgs: Vec<MapConfig>, snap: RunSnapshot) -> Result<Run, generation::GenError> {
        if map_cfgs.is_empty() {
            return Err(generation::GenError::NoPath);
        }
        let level_index = snap.level_index.min(map_cfgs.len() - 1);
        let map_cfg = map_cfgs[level_index].clone();
        let level = build_level(&cfg, &map_cfg, &snap.upgrades)?;
        Ok(Run {
            gold: snap.gold,
            upgrades: snap.upgrades,
            consumables: snap.consumables,
            lives: snap.lives,
            score_total: snap.score_total,
            level_index,
            unlocked: snap.unlocked.max(level_index + 1).min(map_cfgs.len()),
            map_cfgs,
            cfg,
            level,
            sound_events: Vec::new(),
        })
    }

    /// Build a fresh level at `index` with the current run state (gold/upgrades/
    /// lives/score carry over). Used by level select. If the run has ended
    /// (0 lives), a replay restarts with a fresh life budget so re-entering a
    /// level is playable. Returns an error if `index` is out of range.
    pub fn start_level(&mut self, index: usize) -> Result<(), generation::GenError> {
        if index >= self.map_cfgs.len() {
            return Err(generation::GenError::NoPath);
        }
        self.level_index = index;
        self.unlocked = self.unlocked.max(index + 1);
        if self.lives == 0 {
            self.lives = self.cfg.player.starting_lives;
        }
        self.sound_events.clear();
        let map_cfg = self.map_cfgs[index].clone();
        self.level = build_level(&self.cfg, &map_cfg, &self.upgrades)?;
        Ok(())
    }

    /// Restart the current level with a fresh random map (no life cost). Used by
    /// the pause menu's "Restart Level".
    pub fn restart_current_level(&mut self) {
        self.level.restart(generation::fresh_random_seed());
    }

    /// Advance the simulation by `dt`. Handles consumable activation and the
    /// life/score transitions that live outside a single `Level`.
    pub fn update(&mut self, input: Input, dt: f32) -> RunEvent {
        if input.use_super_pick {
            self.try_use_consumable(ConsumableKind::SuperPick);
        }
        if input.use_sticky_smell {
            self.try_use_consumable(ConsumableKind::StickySmell);
        }

        match self.level.update(input.move_, dt) {
            LevelEvent::None => RunEvent::Playing,
            LevelEvent::Caught => self.on_caught(),
            LevelEvent::Completed => self.on_completed(),
        }
    }

    /// Spend a consumable (if any owned) and activate its effect.
    fn try_use_consumable(&mut self, kind: ConsumableKind) {
        if self.consumables.use_one(kind) {
            let d = consumables::duration(kind, &self.cfg.consumables);
            self.level.start_effect(kind, d);
            let sfx = match kind {
                ConsumableKind::SuperPick => Sfx::SuperPick,
                ConsumableKind::StickySmell => Sfx::StickySmell,
            };
            self.sound_events.push(sfx);
        }
    }

    /// Take all one-shot sound effects reported since the last call: this run's
    /// own (consumable activations) plus the current level's (rock breaks, gold
    /// pickups). The caller plays them through the audio layer.
    pub fn drain_sounds(&mut self) -> Vec<Sfx> {
        let mut out = std::mem::take(&mut self.sound_events);
        out.extend(self.level.drain_sounds());
        out
    }

    /// A catch costs a life; 0 lives is game over, else restart the level.
    fn on_caught(&mut self) -> RunEvent {
        // `saturating_sub` guards against a resumed 0-lives save: a catch must
        // never wrap `u32` to a huge value (which would silently skip game over).
        self.lives = self.lives.saturating_sub(1);
        if self.lives == 0 {
            RunEvent::GameOver
        } else {
            // Preserve this frame's level sounds (e.g. a rock the player broke
            // the same frame they were caught) before the restart clears the
            // freshly-regenerated level's queue.
            self.sound_events.extend(self.level.drain_sounds());
            self.level.restart(generation::fresh_random_seed());
            RunEvent::Caught
        }
    }

    /// Bank gold, add the level score, and advance or win.
    fn on_completed(&mut self) -> RunEvent {
        self.gold += self.level.gold_collected;
        let score = score::level_score(self.level.elapsed(), self.level.gold_collected, &self.cfg.score);
        self.score_total += score;
        // Beating a level unlocks the next one for the level select.
        self.unlocked = self.unlocked.max((self.level_index + 2).min(self.map_cfgs.len()));
        if self.is_last_level() {
            RunEvent::Victory
        } else {
            RunEvent::LevelCompleted { score }
        }
    }

    /// Advance to the next level, carrying over gold/lives/upgrades/consumables/
    /// score. The consumable effect resets (owned counts persist).
    pub fn begin_next_level(&mut self) -> Result<RunEvent, generation::GenError> {
        self.level_index += 1;
        self.sound_events.clear();
        let map_cfg = self.map_cfgs.get(self.level_index).cloned().ok_or(generation::GenError::NoPath)?;
        self.level = build_level(&self.cfg, &map_cfg, &self.upgrades)?;
        Ok(RunEvent::Playing)
    }

    /// A reference to the game config (for shop/HUD display).
    pub fn config(&self) -> &GameConfig {
        &self.cfg
    }

    /// The current cost of a shop item, given this run's state.
    pub fn item_cost(&self, item: ShopItem) -> u32 {
        shop::cost(item, self, &self.cfg)
    }

    /// Purchase a shop item, deducting gold and applying its effect.
    pub fn buy(&mut self, item: ShopItem) -> Result<(), ShopError> {
        let cfg = self.cfg.clone();
        shop::buy(item, self, &cfg)
    }
}

/// Build a level from a config + map config + current upgrades.
fn build_level(
    cfg: &GameConfig,
    map_cfg: &MapConfig,
    upgrades: &Upgrades,
) -> Result<Level, generation::GenError> {
    let seed = generation::resolve_seed(map_cfg);
    let params = LevelParams {
        player_speed: upgrades::walk_speed(cfg.player.base_speed, upgrades, &cfg.upgrades.walk_speed),
        player_mining_time: upgrades::mining_time(cfg.player.base_mining_time, upgrades, &cfg.upgrades.mining_speed),
        beast_speed: cfg.beast.base_speed * map_cfg.beast_speed_multiplier,
        beast_mining_time: cfg.beast.base_mining_time * map_cfg.beast_mining_time_multiplier,
        replan_interval: cfg.beast.replan_interval,
    };
    Level::new(map_cfg, seed, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::game::GameConfig;
    use crate::game::consumables::ConsumableKind;

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
        files = ["a.toml", "b.toml"]
    "#;

    fn game() -> GameConfig {
        GameConfig::from_toml(GAME_TOML).expect("valid game config")
    }

    fn map_cfg(beast_count: u32, seed: u64) -> MapConfig {
        let mut c = MapConfig::from_toml(&format!(
            r#"
                width = 30
                height = 20
                unmineable_count = 20
                beast_count = {beast_count}
                start = {{ x = 15, y = 19 }}
                exit  = {{ x = 5,  y = 0 }}
                seed = {seed}
            "#
        ))
        .expect("valid map");
        c.seed = Some(seed);
        c
    }

    fn run(seed: u64) -> Run {
        let cfgs = vec![map_cfg(1, seed), map_cfg(0, seed)];
        Run::new(game(), cfgs).expect("run builds")
    }

    fn no_input() -> Input {
        Input { move_: Default::default(), use_super_pick: false, use_sticky_smell: false }
    }

    fn center_of(x: usize, y: usize) -> macroquad::prelude::Vec2 {
        let t = crate::game::TILE_SIZE;
        macroquad::prelude::Vec2::new(x as f32 * t + t / 2.0, y as f32 * t + t / 2.0)
    }

    #[test]
    fn new_run_starts_level_zero_with_starting_lives() {
        let r = run(12345);
        assert_eq!(r.lives, 3);
        assert_eq!(r.gold, 0);
        assert_eq!(r.score_total, 0);
        assert_eq!(r.level_index(), 0);
        assert_eq!(r.level_count(), 2);
    }

    #[test]
    fn catching_reduces_lives_and_restarts_level() {
        let mut r = run(12345);
        // Put the player on the beast so the next update catches.
        r.level.player.pos = r.level.beasts[0].pos;
        let ev = r.update(no_input(), 1.0 / 60.0);
        assert_eq!(ev, RunEvent::Caught);
        assert_eq!(r.lives, 2, "a catch costs a life");
        // The level regenerated (player re-spawned at the start gap).
        let (sx, sy) = r.level.map.start_pos();
        let pc = (
            (r.level.player.pos.x / crate::game::TILE_SIZE).floor() as usize,
            (r.level.player.pos.y / crate::game::TILE_SIZE).floor() as usize,
        );
        assert_eq!(pc, (sx, sy), "player re-spawned at start after a catch");
    }

    #[test]
    fn zero_lives_is_game_over() {
        let mut r = run(12345);
        r.lives = 1;
        r.level.player.pos = r.level.beasts[0].pos;
        let ev = r.update(no_input(), 1.0 / 60.0);
        assert_eq!(ev, RunEvent::GameOver, "0 lives ends the run");
        assert_eq!(r.lives, 0);
    }

    #[test]
    fn zero_lives_catch_is_game_over_not_underflow() {
        // A resumed save carrying 0 lives must not wrap `u32` (which would strand
        // the player on infinite lives); a catch must simply be game over.
        let mut r = run(12345);
        r.lives = 0;
        r.level.player.pos = r.level.beasts[0].pos;
        let ev = r.update(no_input(), 1.0 / 60.0);
        assert_eq!(ev, RunEvent::GameOver, "0 lives -> game over on the first catch");
        assert_eq!(r.lives, 0, "lives must not underflow to a huge value");
    }

    #[test]
    fn completing_the_first_level_banks_gold_and_score() {
        let mut r = run(12345);
        // Drop the exit-guarding beast so the player can reach the exit.
        r.level.beasts.clear();
        r.level.gold_collected = 8;
        // Move the player onto the exit to complete the level.
        let (ex, ey) = r.level.map.exit_pos();
        r.level.player.pos = center_of(ex, ey);
        let ev = r.update(no_input(), 1.0 / 60.0);
        let expected = score::level_score(r.level.elapsed(), 8, &game().score);
        assert_eq!(ev, RunEvent::LevelCompleted { score: expected }, "first level completed");
        assert_eq!(r.gold, 8, "gold banks on completion");
        assert_eq!(r.score_total, expected, "score added to the running total");
    }

    #[test]
    fn completing_the_last_level_is_victory() {
        let mut r = run(12345);
        // Jump to the last level by simulating: advance once then complete.
        r.begin_next_level().expect("advance to level 1");
        assert!(r.is_last_level(), "level 1 of 2 is the last");
        r.level.gold_collected = 3;
        let (ex, ey) = r.level.map.exit_pos();
        r.level.player.pos = center_of(ex, ey);
        let ev = r.update(no_input(), 1.0 / 60.0);
        assert_eq!(ev, RunEvent::Victory, "last level completion is victory");
    }

    #[test]
    fn consumable_use_decrements_count_and_sets_effect() {
        let mut r = run(12345);
        r.consumables.add(ConsumableKind::SuperPick);
        r.consumables.add(ConsumableKind::StickySmell);
        let input = Input { move_: Default::default(), use_super_pick: true, use_sticky_smell: false };
        let ev = r.update(input, 1.0 / 60.0);
        assert_eq!(ev, RunEvent::Playing);
        assert_eq!(r.consumables.count(ConsumableKind::SuperPick), 0);
        assert!(r.level.active_effect.is_some());
        assert_eq!(r.level.active_effect.map(|e| e.kind), Some(ConsumableKind::SuperPick));

        // Using the same consumable again with none owned does nothing.
        let input2 = Input { move_: Default::default(), use_super_pick: true, use_sticky_smell: false };
        let _ = r.update(input2, 1.0 / 60.0);
        assert!(r.level.active_effect.is_some(), "effect still active (nothing to re-use)");
    }

    #[test]
    fn use_consumables_reports_sound_events() {
        let mut r = run(12345);
        r.consumables.add(ConsumableKind::SuperPick);
        r.consumables.add(ConsumableKind::StickySmell);
        let input = Input { move_: Default::default(), use_super_pick: true, use_sticky_smell: true };
        let ev = r.update(input, 1.0 / 60.0);
        assert_eq!(ev, RunEvent::Playing);
        let sounds = r.drain_sounds();
        assert!(sounds.contains(&Sfx::SuperPick), "using a Super Pick reports it, got {sounds:?}");
        assert!(sounds.contains(&Sfx::StickySmell), "using Sticky Smell reports it, got {sounds:?}");
    }

    #[test]
    fn use_consumable_with_none_owned_reports_nothing() {
        let mut r = run(12345);
        // No consumables owned; pressing the use key must not emit a sound.
        let input = Input { move_: Default::default(), use_super_pick: true, use_sticky_smell: true };
        let _ = r.update(input, 1.0 / 60.0);
        let sounds = r.drain_sounds();
        assert!(!sounds.contains(&Sfx::SuperPick), "no owned pick -> no super-pick sound");
        assert!(!sounds.contains(&Sfx::StickySmell), "no owned smell -> no sticky sound");
    }

    #[test]
    fn caught_frame_sounds_survive_the_restart() {
        let mut r = run(12345);
        // A rock directly below the player, and the player flush against it so a
        // Super Pick breaks it on the exact frame a beast (teleported onto the
        // player) catches them. The rock-break must not be wiped by the restart.
        let t = crate::game::TILE_SIZE;
        r.level.map.set_tile(2, 2, crate::game::map::Tile::Dirt);
        r.level.map.set_tile(2, 3, crate::game::map::Tile::Mineable);
        r.level.player.pos = macroquad::prelude::Vec2::new(
            2.0 * t + t / 2.0,
            3.0 * t - crate::game::movement::HITBOX_HALF,
        );
        r.level.player.facing = macroquad::prelude::Vec2::new(0.0, 1.0);
        r.level.start_effect(ConsumableKind::SuperPick, 3.0);
        r.level.beasts[0].pos = r.level.player.pos;

        let input = Input {
            move_: macroquad::prelude::Vec2::new(0.0, 1.0),
            use_super_pick: false,
            use_sticky_smell: false,
        };
        let ev = r.update(input, 1.0 / 60.0);
        assert_eq!(ev, RunEvent::Caught, "caught on the same frame as the break");
        assert_eq!(r.lives, 2, "a fresh run starts with 3 lives; one catch spends one");

        let sounds = r.drain_sounds();
        assert!(
            sounds.contains(&Sfx::RockBreak),
            "rock-break emitted on the catch frame survives the restart, got {sounds:?}"
        );
    }

    #[test]
    fn re_update_after_completion_does_not_rebank() {
        let mut r = run(12345);
        r.level.beasts.clear();
        r.level.gold_collected = 7;
        let (ex, ey) = r.level.map.exit_pos();
        r.level.player.pos = center_of(ex, ey);
        let first = r.update(no_input(), 1.0 / 60.0);
        assert!(matches!(first, RunEvent::LevelCompleted { .. }));
        let (gold_after, score_after) = (r.gold, r.score_total);
        // A spurious second update on the finished level must not re-bank.
        let second = r.update(no_input(), 1.0 / 60.0);
        assert_eq!(second, RunEvent::Playing, "finished level reports nothing on re-update");
        assert_eq!(r.gold, gold_after, "gold is not double-banked");
        assert_eq!(r.score_total, score_after, "score is not double-counted");
    }

    #[test]
    fn begin_next_level_persists_gold_lives_and_upgrades() {
        let mut r = run(12345);
        r.gold = 500;
        r.lives = 2;
        r.upgrades.walk_speed = 2;
        r.level.gold_collected = 100; // should NOT carry over
        r.begin_next_level().expect("advance");
        assert_eq!(r.gold, 500, "gold persists across levels");
        assert_eq!(r.lives, 2, "lives persist across levels");
        assert_eq!(r.upgrades.walk_speed, 2, "upgrades persist across levels");
        assert_eq!(r.level_index(), 1);
        assert_eq!(r.level.gold_collected, 0, "per-level gold does not carry over");
    }

    #[test]
    fn snapshot_captures_run_state_and_resume_restores_it() {
        let mut r = run(12345);
        r.gold = 321;
        r.upgrades.mining_speed = 2;
        r.consumables.add(ConsumableKind::SuperPick);
        r.lives = 4;
        r.score_total = 777;
        r.unlocked = 2;
        let snap = r.snapshot();

        assert_eq!(snap.gold, 321);
        assert_eq!(snap.upgrades.mining_speed, 2);
        assert_eq!(snap.lives, 4);
        assert_eq!(snap.score_total, 777);
        assert_eq!(snap.level_index, 0);
        assert_eq!(snap.unlocked, 2);

        // Resume from the snapshot into a fresh Run; the state is preserved.
        let r2 = Run::resume(game(), vec![map_cfg(1, 1), map_cfg(0, 1)], snap).expect("resume builds");
        assert_eq!(r2.snapshot(), snap, "resume reproduces the saved run state");
        assert_eq!(r2.level_index(), 0);
        assert_eq!(r2.gold, 321);
        assert_eq!(r2.upgrades.mining_speed, 2);
        assert_eq!(r2.consumables.count(ConsumableKind::SuperPick), 1);
        assert_eq!(r2.lives, 4);
    }

    #[test]
    fn resume_clamps_out_of_range_level_index() {
        let snap = RunSnapshot {
            gold: 10,
            upgrades: Upgrades::default(),
            consumables: Consumables::default(),
            lives: 3,
            score_total: 0,
            level_index: 99, // only 2 maps exist
            unlocked: 1,
        };
        let r = Run::resume(game(), vec![map_cfg(1, 1), map_cfg(0, 1)], snap).expect("resume clamps");
        assert_eq!(r.level_index(), 1, "level index clamped to the last available map");
    }

    #[test]
    fn resume_clamps_unlocked_to_level_count() {
        // A hand-edited save claiming everything is unlocked must be capped.
        let snap = RunSnapshot {
            gold: 0,
            upgrades: Upgrades::default(),
            consumables: Consumables::default(),
            lives: 3,
            score_total: 0,
            level_index: 0,
            unlocked: 999,
        };
        let r = Run::resume(game(), vec![map_cfg(1, 1), map_cfg(0, 1)], snap).expect("resume clamps");
        assert_eq!(r.unlocked(), 2, "unlocked clamped to the available level count");
    }

    #[test]
    fn start_level_builds_requested_level_and_preserves_cross_level_state() {
        let mut r = run(12345);
        r.gold = 250;
        r.lives = 2;
        r.upgrades.walk_speed = 3;
        r.unlocked = 2;
        r.start_level(1).expect("start level 2");
        assert_eq!(r.level_index(), 1, "the selected level becomes current");
        assert_eq!(r.gold, 250, "run gold persists into the replayed level");
        assert_eq!(r.lives, 2, "run lives persist");
        assert_eq!(r.upgrades.walk_speed, 3, "upgrades persist");
        assert_eq!(r.level.gold_collected, 0, "per-level gold is fresh");
        assert_eq!(r.unlocked, 2, "unlock progress is not lost");
    }

    #[test]
    fn start_level_out_of_range_errors() {
        let mut r = run(12345);
        assert!(matches!(r.start_level(5), Err(generation::GenError::NoPath)));
    }

    #[test]
    fn restart_current_level_regenerates_map_without_spending_a_life() {
        let mut r = run(12345);
        let before = r.level.map.tiles.clone();
        let lives_before = r.lives;
        r.restart_current_level();
        assert_ne!(r.level.map.tiles, before, "the map regenerates (fresh seed)");
        assert_eq!(r.lives, lives_before, "restart costs no life");
        assert_eq!(r.level_index(), 0, "still on the same level");
    }

    #[test]
    fn unlocking_increments_when_advancing_past_the_furthest_level() {
        let mut r = run(12345);
        assert_eq!(r.unlocked(), 1, "level 1 unlocked at the start");
        // Complete level 0 (clear the exit-guarding beast so the player reaches
        // the exit) and assert level 2 becomes unlocked.
        r.level.beasts.clear();
        r.level.gold_collected = 1;
        let (ex, ey) = r.level.map.exit_pos();
        r.level.player.pos = center_of(ex, ey);
        let ev = r.update(no_input(), 1.0 / 60.0);
        assert!(matches!(ev, RunEvent::LevelCompleted { .. }));
        assert_eq!(r.unlocked(), 2, "beating level 1 unlocks level 2");
    }
}
