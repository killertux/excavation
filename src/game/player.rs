//! Player entity: continuous movement + grid-aware collision + timed mining
//! (pure, no rendering or input polling).

use macroquad::prelude::Vec2;

use super::map::{Map, Tile};
use super::mining::{self, Mining};
use super::movement;
use crate::assets::ids::PlayerMotion;

/// Seconds per walk-frame when moving (two-frame walk cycle).
const WALK_FRAME_TIME: f32 = 0.25;

/// Seconds per mining-frame (raise/impact pickaxe cycle).
const MINE_FRAME_TIME: f32 = 0.15;

#[derive(Debug, Clone)]
pub struct Player {
    /// Center of the hitbox, in world pixels.
    pub pos: Vec2,
    /// Walk speed, px/s (from `game.toml`).
    pub speed: f32,
    /// Current animation motion (idle/walk/mining). The direction comes from
    /// [`Player::facing`] at render time.
    pub motion: PlayerMotion,
    /// Facing direction (one of the 8 compass directions, normalized). Updated
    /// from the last non-zero move intent; stable while mining.
    pub facing: Vec2,
    /// In-progress mine, if any.
    pub mining: Option<Mining>,
    walk_anim_timer: f32,
}

impl Player {
    pub fn new(pos: Vec2, speed: f32) -> Self {
        Player {
            pos,
            speed,
            motion: PlayerMotion::Idle,
            facing: Vec2::new(0.0, 1.0),
            mining: None,
            walk_anim_timer: 0.0,
        }
    }

    /// Advance the player by one frame.
    ///
    /// `intent` is an unnormalized move direction; `mine_held` is the mine
    /// action. If a mine is already in progress and `mine_held` is true, the
    /// player keeps mining the **same** cell (movement is ignored while mining,
    /// so the facing/target stays stable — never re-aimed or aborted by pressing
    /// a direction). Otherwise, if `mine_held` and the player faces a mineable
    /// cell, a new mine begins: movement stops and `progress` accrues over
    /// `mining_time` seconds, then the cell becomes `Excavated`. If nothing is
    /// being mined, the player moves normally and any mining state is cleared.
    pub fn update(&mut self, intent: Vec2, mine_held: bool, map: &mut Map, mining_time: f32, dt: f32) {
        let target = if mine_held {
            match self.mining {
                // Keep digging the current target; don't re-aim from movement.
                Some(m) => Some(m.target),
                None => mining::mine_target(self.pos, self.facing, map),
            }
        } else {
            None
        };

        match target {
            Some(t) => self.mine(t, map, mining_time, dt),
            None => {
                self.mining = None;
                if intent.length_squared() > 0.0 {
                    self.facing = intent.normalize();
                }
                self.move_free(intent, map, dt);
            }
        }
    }

    /// Advance mining of `target` (or begin it), ignoring movement.
    fn mine(&mut self, target: (i32, i32), map: &mut Map, mining_time: f32, dt: f32) {
        let continuing = self.mining.map(|m| m.target) == Some(target);
        let progress = if continuing {
            self.mining.as_ref().unwrap().progress + dt
        } else {
            0.0
        };
        self.mining = Some(Mining { target, progress });
        self.motion = PlayerMotion::Mine(((progress / MINE_FRAME_TIME) as u8) & 1);
        self.walk_anim_timer = 0.0;

        if progress >= mining_time {
            map.set_tile(target.0, target.1, Tile::Excavated);
            self.mining = None;
        }
    }

    /// Move without mining: apply `intent`, resolve collision, run walk anim.
    fn move_free(&mut self, intent: Vec2, map: &Map, dt: f32) {
        let moving = intent.length_squared() > 0.0;
        if moving {
            let step = intent.normalize() * (self.speed * dt);
            movement::move_axis(&mut self.pos, map, true, step.x);
            movement::move_axis(&mut self.pos, map, false, step.y);

            // Two-frame walk animation.
            self.walk_anim_timer += dt;
            let frame = (self.walk_anim_timer / WALK_FRAME_TIME) as usize % 2;
            self.motion = PlayerMotion::Walk(frame as u8);
        } else {
            self.motion = PlayerMotion::Idle;
            self.walk_anim_timer = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::movement::HITBOX_HALF;
    use super::*;
    use crate::game::TILE_SIZE;

    const TEST_SPEED: f32 = 120.0;

    /// 5x5 grid with a solid rock at (3,2) and an excavated interior.
    fn test_map() -> Map {
        let mut m = Map { width: 5, height: 5, tiles: vec![Tile::Border; 25] };
        for y in 1..4 {
            for x in 1..4 {
                m.tiles[y * 5 + x] = Tile::Excavated;
            }
        }
        m.tiles[2 * 5 + 3] = Tile::Mineable; // rock to the right of center
        m
    }

    fn open_map() -> Map {
        let mut map = Map { width: 5, height: 5, tiles: vec![Tile::Excavated; 25] };
        for y in 0..5 {
            map.tiles[y * 5 + 0] = Tile::Border;
            map.tiles[y * 5 + 4] = Tile::Border;
        }
        for x in 0..5 {
            map.tiles[0 * 5 + x] = Tile::Border;
            map.tiles[4 * 5 + x] = Tile::Border;
        }
        map
    }

    fn center_of(cell: (i32, i32)) -> Vec2 {
        Vec2::new(cell.0 as f32 * TILE_SIZE + TILE_SIZE / 2.0, cell.1 as f32 * TILE_SIZE + TILE_SIZE / 2.0)
    }

    fn player_at_center() -> Player {
        Player::new(center_of((2, 2)), TEST_SPEED)
    }

    #[test]
    fn moving_into_solid_tile_is_blocked() {
        let mut map = test_map();
        let mut p = player_at_center();
        p.update(Vec2::new(1.0, 0.0), false, &mut map, 0.8, 1.0);
        // The right edge must stop exactly at the rock's left edge (x = 3*16).
        assert!((p.pos.x + HITBOX_HALF - 3.0 * TILE_SIZE).abs() < 0.001);
        assert!(p.pos.x <= 3.0 * TILE_SIZE - HITBOX_HALF + 0.001);
    }

    #[test]
    fn moving_through_excavated_and_doors_is_allowed() {
        let mut map = open_map();
        map.tiles[2 * 5 + 2] = Tile::StartDoor;
        let mut p = Player::new(center_of((2, 2)), TEST_SPEED);
        let before = p.pos;
        p.update(Vec2::new(1.0, 0.0), false, &mut map, 0.8, 1.0 / 60.0);
        assert!(p.pos.x > before.x, "should move through open floor");
        assert!((p.pos.x - before.x - TEST_SPEED * (1.0 / 60.0)).abs() < 0.001);
    }

    #[test]
    fn diagonal_input_is_normalized() {
        let dt = 1.0 / 60.0;
        let (mut map1, mut map2) = (test_map(), test_map());
        let (mut p1, mut p2) = (player_at_center(), player_at_center());
        let (b1, b2) = (p1.pos, p2.pos);
        p1.update(Vec2::new(1.0, 0.0), false, &mut map1, 0.8, dt);
        p2.update(Vec2::new(1.0, 1.0), false, &mut map2, 0.8, dt);
        let d1 = (p1.pos - b1).length();
        let d2 = (p2.pos - b2).length();
        assert!((d1 - d2).abs() < 0.001, "diagonal must not be faster");
        assert!((d1 - TEST_SPEED * dt).abs() < 0.001);
    }

    #[test]
    fn walk_animation_alternates_while_moving_and_idles() {
        let mut map = open_map();
        let mut p = player_at_center();
        let dt = 1.0 / 60.0;
        let (mut saw1, mut saw2) = (false, false);
        for _ in 0..120 {
            p.update(Vec2::new(1.0, 0.0), false, &mut map, 0.8, dt);
            match p.motion {
                PlayerMotion::Walk(0) => saw1 = true,
                PlayerMotion::Walk(1) => saw2 = true,
                other => panic!("expected a walk phase, got {other:?}"),
            }
        }
        assert!(saw1 && saw2);
        for _ in 0..5 {
            p.update(Vec2::ZERO, false, &mut map, 0.8, dt);
        }
        assert_eq!(p.motion, PlayerMotion::Idle);
    }

    #[test]
    fn mining_completes_and_turns_rock_to_excavated() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        let mut p = player_at_center();
        p.facing = Vec2::new(1.0, 0.0);
        let before = p.pos;
        let dt = 1.0 / 60.0;
        // 1 second of held mining exceeds the 0.8s mining time.
        for _ in 0..60 {
            p.update(Vec2::ZERO, true, &mut map, 0.8, dt);
        }
        assert_eq!(map.tile(3, 2), Tile::Excavated, "rock was mined through");
        // Mining gates movement: the player never moved.
        assert!((p.pos - before).length() < 0.001);
    }

    #[test]
    fn unmineable_rock_cannot_be_mined() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Unmineable);
        let mut p = player_at_center();
        p.facing = Vec2::new(1.0, 0.0);
        for _ in 0..60 {
            p.update(Vec2::ZERO, true, &mut map, 0.8, 1.0 / 60.0);
        }
        assert_eq!(map.tile(3, 2), Tile::Unmineable, "unmineable rock stays");
        assert!(p.mining.is_none(), "no mining engaged against a non-diggable cell");
    }

    #[test]
    fn mining_holds_anim_and_ignores_walls() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Wall);
        let mut p = player_at_center();
        p.facing = Vec2::new(1.0, 0.0);
        // Holding mine against a wall does not engage mining (nothing to dig).
        p.update(Vec2::ZERO, true, &mut map, 0.8, 1.0 / 60.0);
        assert!(p.mining.is_none());
        assert_eq!(map.tile(3, 2), Tile::Wall);
    }

    #[test]
    fn releasing_mine_clears_mining_and_resets_progress() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        let mut p = player_at_center();
        p.facing = Vec2::new(1.0, 0.0);
        p.update(Vec2::ZERO, true, &mut map, 0.8, 1.0 / 60.0);
        assert!(p.mining.is_some());
        p.update(Vec2::ZERO, false, &mut map, 0.8, 1.0 / 60.0);
        assert!(p.mining.is_none(), "releasing the key clears mining");
        assert_eq!(map.tile(3, 2), Tile::Mineable, "not yet mined");
    }

    #[test]
    fn mining_is_not_retargeted_by_movement_input() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        map.set_tile(1, 2, Tile::Mineable);
        let mut p = player_at_center();
        // Begin mining the rock to the right for two frames (progress accrues).
        p.facing = Vec2::new(1.0, 0.0);
        p.update(Vec2::ZERO, true, &mut map, 0.8, 1.0 / 60.0);
        p.update(Vec2::ZERO, true, &mut map, 0.8, 1.0 / 60.0);
        let before = p.mining.unwrap().progress;
        assert!(before > 0.0);
        // Press a direction while still holding mine: the mine target stays the
        // same and progress keeps accruing (movement is ignored while mining).
        p.update(Vec2::new(-1.0, 0.0), true, &mut map, 0.8, 1.0 / 60.0);
        assert_eq!(p.mining.map(|m| m.target), Some((3, 2)), "target stable while mining");
        assert!(p.mining.unwrap().progress > before, "progress keeps accruing");
        // The player did not move despite the direction press (movement ignored).
        assert_eq!(p.pos, center_of((2, 2)));
    }

    /// Fuzz: random movement (incl. rapid reversals, diagonals, variable dt)
    /// against a real generated map must never tunnel through a solid tile or
    /// leave the map. Reproduces the "player walks over rocks / off screen" bug.
    #[test]
    fn fuzz_never_tunnels_or_leaves_map_with_real_map() {
        use crate::config::map::MapConfig;
        use crate::game::generation::generate;
        use macroquad::rand::RandGenerator;

        let cfg = MapConfig::from_toml(
            r#"
                width = 30
                height = 20
                unmineable_count = 20
                start_door = { x = 15, y = 19 }
                exit_door  = { x = 5,  y = 0 }
                visible_walls = [[8, 5], [9, 5], [10, 5]]
            "#,
        )
        .expect("valid config");
        let mut map = generate(&cfg, 12345).expect("generates");
        let start = map.start_pos().expect("start");
        let mut p = Player::new(center_of((start.0 as i32, start.1 as i32)), 120.0);

        let map_w = map.width as f32 * TILE_SIZE;
        let map_h = map.height as f32 * TILE_SIZE;
        let mut rng = RandGenerator::new();
        rng.srand(0xDEADBEEF);

        for frame in 0..50_000 {
            // Random direction incl. (0,0), diagonals, and cardinals.
            let dx = rng.gen_range(-1i32, 2i32);
            let dy = rng.gen_range(-1i32, 2i32);
            // dt from 1ms to 100ms (also stress the sub-stepper).
            let dt = rng.gen_range(1.0f32, 100.0) / 1000.0;
            let mine = rng.gen_range(-1i32, 2i32) == 0;

            p.update(Vec2::new(dx as f32, dy as f32), mine, &mut map, 0.8, dt);

            let (x, y) = (p.pos.x, p.pos.y);
            assert!(x.is_finite() && y.is_finite(), "NaN at frame {frame}: ({x},{y})");
            // Center must never sit inside a solid cell (rock/wall/border).
            let cx = (x / TILE_SIZE).floor() as i32;
            let cy = (y / TILE_SIZE).floor() as i32;
            assert!(
                !map.is_solid(cx, cy),
                "player centered inside solid cell ({cx},{cy}) at frame {frame} pos ({x},{y})"
            );
            // Hitbox must not overlap a solid tile by more than a hair either.
            let in_bounds = x - HITBOX_HALF >= -0.01
                && x + HITBOX_HALF <= map_w + 0.01
                && y - HITBOX_HALF >= -0.01
                && y + HITBOX_HALF <= map_h + 0.01;
            assert!(in_bounds, "player left the map at frame {frame}: ({x},{y})");
        }
    }
}
