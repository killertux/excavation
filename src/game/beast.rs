//! Beast entity: chases the player through the map (pure, no rendering).
//!
//! M2 introduces the beast as a simple chaser: it walks straight toward the
//! player and is blocked by solid cells (it cannot dig through rocks — that is
//! M3). With an all-rock starting map it holds position and "faces" the player
//! until the player digs a path, then pursues with a directional walk cycle.
//! Catching the player is also M3.

use macroquad::prelude::Vec2;

use super::map::Map;
use super::movement;
use crate::assets::ids::{BeastMotion, Direction, WALK_FRAMES};

/// Beast walk speed, world px/s (roughly half the player's 240 px/s).
pub const BEAST_SPEED: f32 = 140.0;

/// Seconds per beast walk-frame (four-frame cycle, per atlas timing).
const WALK_FRAME_TIME: f32 = 0.1;

#[derive(Debug, Clone)]
pub struct Beast {
    /// Center of the hitbox, in world pixels.
    pub pos: Vec2,
    /// Facing direction (normalized), updated from movement.
    pub facing: Vec2,
    /// Current animation motion.
    pub motion: BeastMotion,
    walk_timer: f32,
}

impl Beast {
    pub fn new(pos: Vec2) -> Self {
        Beast {
            pos,
            facing: Vec2::new(0.0, 1.0),
            motion: BeastMotion::Idle,
            walk_timer: 0.0,
        }
    }

    /// Advance the beast by `dt`. It moves toward `player_pos`; collision blocks
    /// it against solids (so it idles until the player opens a path).
    pub fn update(&mut self, player_pos: Vec2, map: &Map, dt: f32) {
        let to = player_pos - self.pos;
        let intent = if to.length_squared() > 0.0 { to.normalize() } else { Vec2::ZERO };

        if intent.length_squared() > 0.0 {
            self.facing = intent;
            let step = intent * (BEAST_SPEED * dt);
            movement::move_axis(&mut self.pos, map, true, step.x);
            movement::move_axis(&mut self.pos, map, false, step.y);

            self.walk_timer += dt;
            let frame = (self.walk_timer / WALK_FRAME_TIME) as usize % WALK_FRAMES;
            self.motion = BeastMotion::Walk(frame as u8);
        } else {
            self.motion = BeastMotion::Idle;
        }
    }

    /// Direction for animation (dominant axis of the facing vector).
    pub fn dir(&self) -> Direction {
        Direction::from_vec2(self.facing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::map::Tile;
    use crate::game::TILE_SIZE;

    fn open_map() -> Map {
        let mut map = Map { width: 5, height: 5, tiles: vec![Tile::Dirt; 25], start: (0, 2), exit: (4, 2) };
        for y in 0..5 {
            map.tiles[y * 5 + 0] = Tile::Unbreakable;
            map.tiles[y * 5 + 4] = Tile::Unbreakable;
        }
        for x in 0..5 {
            map.tiles[0 * 5 + x] = Tile::Unbreakable;
            map.tiles[4 * 5 + x] = Tile::Unbreakable;
        }
        map
    }

    fn center_of(c: (i32, i32)) -> Vec2 {
        Vec2::new(c.0 as f32 * TILE_SIZE + TILE_SIZE / 2.0, c.1 as f32 * TILE_SIZE + TILE_SIZE / 2.0)
    }

    #[test]
    fn beast_moves_toward_player_on_open_floor() {
        let map = open_map();
        let mut b = Beast::new(center_of((1, 2)));
        let start = b.pos;
        let player = center_of((3, 2));
        b.update(player, &map, 1.0 / 60.0);
        assert!(b.pos.x > start.x, "beast should move right toward player");
        assert!(b.pos.x <= player.x);
        assert_ne!(b.motion, BeastMotion::Idle, "beast should be walking");
    }

    #[test]
    fn beast_is_blocked_by_solid_cells() {
        // A rock column separates the beast (left) from the player (right).
        let mut map = open_map();
        for y in 1..4 {
            map.set_tile(2, y, Tile::Unbreakable);
        }
        let mut b = Beast::new(center_of((1, 2)));
        let player = center_of((3, 2));
        // After several frames the beast must not cross the rock column.
        for _ in 0..120 {
            b.update(player, &map, 1.0 / 60.0);
        }
        assert!(b.pos.x < 2.0 * TILE_SIZE, "beast must not pass the rock");
    }

    #[test]
    fn facing_updates_toward_player() {
        let map = open_map();
        let mut b = Beast::new(center_of((2, 3)));
        let player = center_of((2, 1)); // above
        b.update(player, &map, 1.0 / 60.0);
        assert!(b.facing.y < 0.0, "beast should face up");
        assert_eq!(b.dir(), Direction::Up);
    }
}
