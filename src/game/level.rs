//! Level container: owns the whole play state (map, player, beasts, lives) so
//! the simulation is testable **without a GPU** (pure — only `Vec2`/map/game
//! types). `App` holds a `Level` and renders from it; the update loop returns a
//! [`LevelEvent`] describing what happened this frame.

use macroquad::prelude::Vec2;

use super::beast::Beast;
use super::generation;
use super::map::Map;
use super::movement;
use super::player::Player;
use super::TILE_SIZE;
use crate::config::game::GameConfig;
use crate::config::map::MapConfig;
use crate::input::Input;

/// The outcome of one level-simulation step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelEvent {
    /// Nothing notable happened; keep playing.
    None,
    /// The player reached the exit gap.
    Completed,
    /// The player was caught but still has lives; the level auto-restarted.
    Caught,
    /// The player ran out of lives.
    GameOver,
}

/// The full play state of a single level.
pub struct Level {
    pub map: Map,
    map_cfg: MapConfig,
    pub player: Player,
    pub beasts: Vec<Beast>,
    pub lives: u32,
    // Config-derived values, kept so a restart can re-spawn/mine correctly.
    player_speed: f32,
    player_mining_time: f32,
    beast_speed: f32,
    beast_mining_time: f32,
    replan_interval: f32,
}

impl Level {
    /// Build a level from the game + map configs and a generation `seed`.
    pub fn new(game: &GameConfig, map_cfg: &MapConfig, seed: u64) -> Result<Level, generation::GenError> {
        let map = generation::generate(map_cfg, seed)?;
        let player_speed = game.player.base_speed;
        let player_mining_time = game.player.base_mining_time;
        let beast_speed = game.beast.base_speed * map_cfg.beast_speed_multiplier;
        let beast_mining_time = game.beast.base_mining_time * map_cfg.beast_mining_time_multiplier;
        let replan_interval = game.beast.replan_interval;

        let player = spawn_player(&map, player_speed);
        let beasts = spawn_beasts(&map, map_cfg.beast_count, beast_speed, beast_mining_time, replan_interval);

        Ok(Level {
            map,
            map_cfg: map_cfg.clone(),
            player,
            beasts,
            lives: game.player.starting_lives,
            player_speed,
            player_mining_time,
            beast_speed,
            beast_mining_time,
            replan_interval,
        })
    }

    /// The player's per-rock mining time (seconds), used to time the burst.
    pub fn mining_time(&self) -> f32 {
        self.player_mining_time
    }

    /// Advance the simulation by `dt`. Returns the first significant event.
    ///
    /// Runs the player, then every beast, then checks for a catch (hitbox
    /// overlap), then checks whether the player reached the exit gap.
    pub fn update(&mut self, input: Input, dt: f32) -> LevelEvent {
        self.player.update(input.move_, &mut self.map, self.player_mining_time, dt);

        let player_pos = self.player.pos;
        for beast in &mut self.beasts {
            beast.update(player_pos, &mut self.map, dt);
        }

        // Catch = hitbox overlap with any beast.
        for beast in &self.beasts {
            if movement::hits(self.player.pos, beast.pos) {
                return self.on_caught();
            }
        }

        // Reaching the exit gap completes the level.
        if player_on_exit(self.player.pos, self.map.exit_pos()) {
            return LevelEvent::Completed;
        }

        LevelEvent::None
    }

    /// Discard a life; regenerate a fresh map if lives remain, else game over.
    fn on_caught(&mut self) -> LevelEvent {
        self.lives -= 1;
        if self.lives == 0 {
            LevelEvent::GameOver
        } else {
            self.restart(generation::fresh_random_seed());
            LevelEvent::Caught
        }
    }

    /// Regenerate the map with `seed` and re-spawn the player + beasts. Lives
    /// are **not** reset here — the caller manages lives across catches.
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
    use crate::config::game::GameConfig;
    use crate::config::map::MapConfig;

    const GAME_TOML: &str = r#"
        [player]
        base_speed = 240.0
        base_mining_time = 0.8
        starting_lives = 3

        [beast]
        base_speed = 140.0
        base_mining_time = 1.6
        replan_interval = 0.25
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

    /// A level seeded with a fixed seed is deterministic.
    fn level(seed: u64) -> Level {
        Level::new(&game(), &map_cfg(MAP_TOML), seed).expect("level builds")
    }

    fn no_input() -> Input {
        Input { move_: Vec2::ZERO }
    }

    #[test]
    fn spawns_expected_player_lives_and_beasts() {
        let lv = level(12345);
        assert_eq!(lv.lives, 3);
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
        let lv = Level::new(&game(), &no_beast, 1).expect("builds");
        assert!(lv.beasts.is_empty());
    }

    #[test]
    fn all_beasts_update_and_advance() {
        let mut lv = level(7);
        // A few frames idle: beasts re-plan and stay in bounds. The point is
        // they all run `update` without panicking and remain inside the map.
        for _ in 0..30 {
            let ev = lv.update(no_input(), 1.0 / 60.0);
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
    fn catch_decrements_lives_and_regenerates_map() {
        let mut lv = level(12345);
        let original_tiles = lv.map.tiles.clone();
        let lives = lv.lives;

        // Nudge the player onto the first beast so `hits` fires next update.
        let b_pos = lv.beasts[0].pos;
        // Teleport (test-only) the player to overlap the beast.
        lv.player.pos = b_pos;

        let ev = lv.update(no_input(), 1.0 / 60.0);
        assert_eq!(ev, LevelEvent::Caught, "caught with lives remaining");
        assert_eq!(lv.lives, lives - 1, "a life was lost");
        assert_ne!(lv.map.tiles, original_tiles, "the map was regenerated");
        let (sx, sy) = lv.map.start_pos();
        let pc = (
            (lv.player.pos.x / TILE_SIZE).floor() as usize,
            (lv.player.pos.y / TILE_SIZE).floor() as usize,
        );
        assert_eq!(pc, (sx, sy), "player re-spawned at start");
    }

    #[test]
    fn zero_lives_ends_the_game() {
        let mut lv = level(12345);
        lv.lives = 1;
        let b_pos = lv.beasts[0].pos;
        lv.player.pos = b_pos;
        let ev = lv.update(no_input(), 1.0 / 60.0);
        assert_eq!(ev, LevelEvent::GameOver, "0 lives is game over");
    }

    #[test]
    fn reaching_exit_completes_the_level() {
        // Beasts spawn on the exit gap, which would catch the player first, so
        // use a beast-free map to isolate the exit-completion path.
        let no_beast = map_cfg("
            width = 30
            height = 20
            unmineable_count = 20
            beast_count = 0
            start = { x = 15, y = 19 }
            exit  = { x = 5,  y = 0 }
        ");
        let mut lv = Level::new(&game(), &no_beast, 12345).expect("builds");
        let (ex, ey) = lv.map.exit_pos();
        lv.player.pos = tile_center(ex as f32, ey as f32);
        let ev = lv.update(no_input(), 1.0 / 60.0);
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
        let mut lv = Level::new(&game(), &cfg, 12345).expect("builds");
        let (ex, ey) = lv.map.exit_pos();
        let rock_below_exit = (ex as i32, ey as i32 + 1);
        let dirt_before = lv.map.count(crate::game::map::Tile::Dirt);
        let player_pos = lv.player.pos;
        let spawn_dist = (lv.beasts[0].pos - player_pos).length();

        // ~20 s of a static player.
        for _ in 0..1200 {
            lv.update(no_input(), 1.0 / 60.0);
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
}
