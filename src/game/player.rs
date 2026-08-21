//! Player entity: continuous movement + grid-aware collision (pure).

use macroquad::prelude::Vec2;

use super::map::Map;
use super::TILE_SIZE;
use crate::assets::ids::PlayerAnim;

/// Player walk speed in world px/s.
/// TODO: move to game.toml in M2.
pub const PLAYER_SPEED: f32 = 120.0;

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
    /// Walk speed, px/s.
    pub speed: f32,
    /// Current animation frame.
    pub anim: PlayerAnim,
    walk_anim_timer: f32,
}

impl Player {
    pub fn new(pos: Vec2) -> Self {
        Player {
            pos,
            speed: PLAYER_SPEED,
            anim: PlayerAnim::Idle,
            walk_anim_timer: 0.0,
        }
    }

    /// Advance the player by `move_intent` (a normalized-or-not direction vector
    /// in world units) against `map`, using `dt` seconds.
    ///
    /// Movement is axis-separated: X is applied and resolved, then Y. Each axis
    /// is sub-stepped so the hitbox can never skip past a solid tile.
    pub fn update(&mut self, move_intent: Vec2, map: &Map, dt: f32) {
        let moving = move_intent.length_squared() > 0.0;
        if moving {
            let step = move_intent.normalize() * (self.speed * dt);
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
    use crate::game::map::Tile;

    /// 5x5 grid with a solid rock at (3,2) and an excavated interior.
    fn test_map() -> Map {
        let mut m = Map {
            width: 5,
            height: 5,
            tiles: vec![Tile::Border; 25],
        };
        for y in 1..4 {
            for x in 1..4 {
                m.tiles[y * 5 + x] = Tile::Excavated;
            }
        }
        m.tiles[2 * 5 + 3] = Tile::Rock; // rock to the right of center
        m
    }

    fn player_at_center() -> Player {
        Player::new(Vec2::new(2.0 * TILE_SIZE + 8.0, 2.0 * TILE_SIZE + 8.0))
    }

    #[test]
    fn moving_into_solid_tile_is_blocked() {
        let map = test_map();
        let mut p = player_at_center();
        // Move right toward the rock at column 3. Use a large dt so the player
        // would overshoot if collision were absent.
        p.update(Vec2::new(1.0, 0.0), &map, 1.0);

        // Right edge must stop exactly at the rock's left edge (x = 3*16).
        assert!((p.pos.x + HITBOX_HALF - 3.0 * TILE_SIZE).abs() < 0.001);
        assert!(p.pos.x <= 3.0 * TILE_SIZE - HITBOX_HALF + 0.001);
    }

    #[test]
    fn moving_through_excavated_and_doors_is_allowed() {
        // A fully open map (no rocks) so movement is unobstructed.
        let mut map = Map {
            width: 5,
            height: 5,
            tiles: vec![Tile::Excavated; 25],
        };
        // Re-add a border so out-of-bounds still blocks, keeping the interior open.
        for y in 0..5 {
            map.tiles[y * 5 + 0] = Tile::Border;
            map.tiles[y * 5 + 4] = Tile::Border;
        }
        for x in 0..5 {
            map.tiles[0 * 5 + x] = Tile::Border;
            map.tiles[4 * 5 + x] = Tile::Border;
        }
        map.tiles[2 * 5 + 2] = Tile::StartDoor;

        let mut p = Player::new(Vec2::new(2.0 * TILE_SIZE + 8.0, 2.0 * TILE_SIZE + 8.0));
        let before = p.pos;
        p.update(Vec2::new(1.0, 0.0), &map, 1.0 / 60.0);
        assert!(p.pos.x > before.x, "player should move right through open floor");
        assert_eq!(p.pos.x - before.x, PLAYER_SPEED * (1.0 / 60.0));
    }

    #[test]
    fn diagonal_input_is_normalized() {
        let map = test_map();
        let dt = 1.0 / 60.0;
        let mut p1 = Player::new(Vec2::new(2.0 * TILE_SIZE + 8.0, 2.0 * TILE_SIZE + 8.0));
        let mut p2 = Player::new(Vec2::new(2.0 * TILE_SIZE + 8.0, 2.0 * TILE_SIZE + 8.0));

        let before1 = p1.pos;
        let before2 = p2.pos;
        // Horizontal-only and diagonal inputs must both move at the same speed.
        p1.update(Vec2::new(1.0, 0.0), &map, dt);
        p2.update(Vec2::new(1.0, 1.0), &map, dt);

        let d1 = (p1.pos - before1).length();
        let d2 = (p2.pos - before2).length();
        assert!((d1 - d2).abs() < 0.001, "diagonal must not be faster: {d1} vs {d2}");
        assert!((d1 - PLAYER_SPEED * dt).abs() < 0.001);
    }

    /// A fully open map so movement/animation is unobstructed.
    fn open_map() -> Map {
        let mut map = Map {
            width: 5,
            height: 5,
            tiles: vec![Tile::Excavated; 25],
        };
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

    #[test]
    fn walk_animation_alternates_while_moving_and_idles() {
        let map = open_map();
        let mut p = Player::new(Vec2::new(2.0 * TILE_SIZE + 8.0, 2.0 * TILE_SIZE + 8.0));
        let dt = 1.0 / 60.0;

        // Moving alternates between the two walk frames.
        let mut saw_walk1 = false;
        let mut saw_walk2 = false;
        for _ in 0..120 {
            p.update(Vec2::new(1.0, 0.0), &map, dt);
            match p.anim {
                PlayerAnim::Walk1 => saw_walk1 = true,
                PlayerAnim::Walk2 => saw_walk2 = true,
                other => panic!("expected a walk frame while moving, got {other:?}"),
            }
        }
        assert!(saw_walk1 && saw_walk2, "walk animation should cycle both frames");

        // Stopping returns to idle.
        for _ in 0..5 {
            p.update(Vec2::ZERO, &map, dt);
        }
        assert_eq!(p.anim, PlayerAnim::Idle);
    }
}
