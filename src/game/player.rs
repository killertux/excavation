//! Player entity: continuous movement + grid-aware collision + timed mining
//! (pure, no rendering or input polling).

use macroquad::prelude::Vec2;

use super::map::{Map, Tile};
use super::mining::{self, Mining};
use super::TILE_SIZE;
use crate::assets::ids::PlayerAnim;

/// Half-extent of the player hitbox. The hitbox is a square slightly smaller
/// than a tile (12×12 inside a 16×16 tile), so the player can slip through
/// one-tile-wide openings.
pub const HITBOX_HALF: f32 = 6.0;

/// Seconds per walk-frame when moving (two-frame walk cycle).
const WALK_FRAME_TIME: f32 = 0.25;

/// Sub-step size used when resolving movement, kept below `HITBOX_HALF` so the
/// player can never tunnel through a tile at any speed or dt.
const MAX_SUBSTEP: f32 = TILE_SIZE / 4.0;

#[derive(Debug, Clone)]
pub struct Player {
    /// Center of the hitbox, in world pixels.
    pub pos: Vec2,
    /// Walk speed, px/s (from `game.toml`).
    pub speed: f32,
    /// Current animation frame.
    pub anim: PlayerAnim,
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
            anim: PlayerAnim::Idle,
            facing: Vec2::new(0.0, 1.0),
            mining: None,
            walk_anim_timer: 0.0,
        }
    }

    /// Advance the player by one frame.
    ///
    /// `intent` is an unnormalized move direction; `mine_held` is the mine
    /// action. If `mine_held` and the player faces a mineable cell, the player
    /// **stops moving** and mines: `progress` accrues over `mining_time` seconds,
    /// then the cell becomes `Excavated`. Otherwise the player moves normally
    /// and any mining state is cleared.
    pub fn update(&mut self, intent: Vec2, mine_held: bool, map: &mut Map, mining_time: f32, dt: f32) {
        if intent.length_squared() > 0.0 {
            self.facing = intent.normalize();
        }

        let target = if mine_held {
            mining::mine_target(self.pos, self.facing, map)
        } else {
            None
        };

        match target {
            Some(t) => self.mine(t, map, mining_time, dt),
            None => {
                self.mining = None;
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
        self.anim = PlayerAnim::Mining;
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
            self.move_axis(map, true, step.x);
            self.move_axis(map, false, step.y);

            // Two-frame walk animation.
            self.walk_anim_timer += dt;
            let frame = (self.walk_anim_timer / WALK_FRAME_TIME) as usize % 2;
            self.anim = if frame == 0 { PlayerAnim::Walk1 } else { PlayerAnim::Walk2 };
        } else {
            self.anim = PlayerAnim::Idle;
            self.walk_anim_timer = 0.0;
        }
    }

    /// Move along a single axis, sub-stepping and resolving collisions.
    fn move_axis(&mut self, map: &Map, horizontal: bool, amount: f32) {
        if amount == 0.0 {
            return;
        }
        let steps = ((amount.abs() / MAX_SUBSTEP).ceil().max(1.0)) as u32;
        let sub = amount / steps as f32;
        for _ in 0..steps {
            if horizontal {
                self.pos.x += sub;
            } else {
                self.pos.y += sub;
            }
            self.resolve_overlaps(map, horizontal, sub);
        }
    }

    /// Push the hitbox out of every solid tile it currently overlaps, along the
    /// given axis, in the direction it moved (`dir` is the sign of that axis's
    /// movement this step).
    fn resolve_overlaps(&mut self, map: &Map, horizontal: bool, dir: f32) {
        let half = HITBOX_HALF;
        let min_col = ((self.pos.x - half) / TILE_SIZE).floor() as i32;
        let max_col = ((self.pos.x + half) / TILE_SIZE).floor() as i32;
        let min_row = ((self.pos.y - half) / TILE_SIZE).floor() as i32;
        let max_row = ((self.pos.y + half) / TILE_SIZE).floor() as i32;

        for row in min_row..=max_row {
            for col in min_col..=max_col {
                if !map.is_solid(col, row) {
                    continue;
                }
                if horizontal {
                    if dir > 0.0 {
                        let tile_left = col as f32 * TILE_SIZE;
                        if self.pos.x + half > tile_left {
                            self.pos.x = tile_left - half;
                        }
                    } else if dir < 0.0 {
                        let tile_right = (col + 1) as f32 * TILE_SIZE;
                        if self.pos.x - half < tile_right {
                            self.pos.x = tile_right + half;
                        }
                    }
                } else if dir > 0.0 {
                    let tile_top = row as f32 * TILE_SIZE;
                    if self.pos.y + half > tile_top {
                        self.pos.y = tile_top - half;
                    }
                } else if dir < 0.0 {
                    let tile_bottom = (row + 1) as f32 * TILE_SIZE;
                    if self.pos.y - half < tile_bottom {
                        self.pos.y = tile_bottom + half;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            match p.anim {
                PlayerAnim::Walk1 => saw1 = true,
                PlayerAnim::Walk2 => saw2 = true,
                other => panic!("expected a walk frame, got {other:?}"),
            }
        }
        assert!(saw1 && saw2);
        for _ in 0..5 {
            p.update(Vec2::ZERO, false, &mut map, 0.8, dt);
        }
        assert_eq!(p.anim, PlayerAnim::Idle);
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
    fn changing_target_resets_progress() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        map.set_tile(1, 2, Tile::Mineable);
        let mut p = player_at_center();
        // Mine the rock to the right for two frames (progress accrues).
        p.facing = Vec2::new(1.0, 0.0);
        p.update(Vec2::ZERO, true, &mut map, 0.8, 1.0 / 60.0);
        p.update(Vec2::ZERO, true, &mut map, 0.8, 1.0 / 60.0);
        let first = p.mining.unwrap().progress;
        assert!(first > 0.0, "progress should have accrued");
        // Turn to face the other rock (a different target) without releasing.
        p.facing = Vec2::new(-1.0, 0.0);
        p.update(Vec2::ZERO, true, &mut map, 0.8, 1.0 / 60.0);
        assert_eq!(p.mining.map(|m| m.target), Some((1, 2)));
        assert!(p.mining.unwrap().progress < first, "progress reset on target change");
    }
}
