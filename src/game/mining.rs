//! Mining mechanics (pure): detecting a rock the player is pressing into, and
//! the in-progress mining state.
//!
//! Mining is now **contact-based**: the player starts digging a rock by walking
//! into it and holding the direction for a short time (`MINE_PUSH_TIME` in the
//! player module). Targeting is derived from the dominant axis of the player's
//! facing and requires the player's hitbox to actually be flush against the
//! rock, so the player cannot mine from a distance.

use macroquad::prelude::Vec2;

use super::map::Map;
use super::TILE_SIZE;

/// An in-progress mine of a single target cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mining {
    /// Grid coords of the cell being dug.
    pub target: (i32, i32),
    /// Seconds of mining accumulated so far.
    pub progress: f32,
}

/// The mineable cell the player is currently **pushing into**, if any.
///
/// `pos` is the player's world-pixel center; `facing` a normalized direction.
/// The player's own cell is the tile containing its center, and the candidate is
/// that cell offset by the dominant axis of `facing`. Returns `Some` only when
/// the candidate is `Mineable` **and** the player's hitbox is flush against it on
/// that axis (the leading edge has reached the shared boundary) — i.e. the player
/// is genuinely walking into the rock and being blocked by it.
pub fn pushed_target(pos: Vec2, facing: Vec2, map: &Map, half: f32) -> Option<(i32, i32)> {
    let cell = ((pos.x / TILE_SIZE).floor() as i32, (pos.y / TILE_SIZE).floor() as i32);
    let (dx, dy) = dominant_axis(facing);
    if (dx, dy) == (0, 0) {
        return None;
    }
    let target = (cell.0 + dx, cell.1 + dy);
    if !map.tile(target.0, target.1).mineable() {
        return None;
    }

    // The hitbox's leading edge must be at the boundary between the player's
    // cell and the target cell (collision resolution pushes the player exactly
    // flush when blocked, so a small tolerance is enough).
    const EPS: f32 = 1.0;
    let flush = if dx != 0 {
        let boundary = (cell.0 as f32 + if dx > 0 { 1.0 } else { 0.0 }) * TILE_SIZE;
        let edge = pos.x + dx as f32 * half;
        (edge - boundary).abs() < EPS
    } else {
        let boundary = (cell.1 as f32 + if dy > 0 { 1.0 } else { 0.0 }) * TILE_SIZE;
        let edge = pos.y + dy as f32 * half;
        (edge - boundary).abs() < EPS
    };

    if flush {
        Some(target)
    } else {
        None
    }
}

/// The dominant cardinal axis of `v`, as `(dx, dy)` in {-1, 0, 1}. Returns
/// `(0, 0)` for a zero (or ambiguous-empty) vector.
fn dominant_axis(v: Vec2) -> (i32, i32) {
    let ax = v.x.abs();
    let ay = v.y.abs();
    if ax > ay {
        (if v.x > 0.0 { 1 } else { -1 }, 0)
    } else if ay > ax {
        (0, if v.y > 0.0 { 1 } else { -1 })
    } else if ax > 0.0 {
        (if v.x > 0.0 { 1 } else { -1 }, 0)
    } else {
        (0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::map::Tile;

    const HALF: f32 = 12.0;

    /// 5x5 grid, border ring, interior dirt; helper to build one.
    fn open_map() -> Map {
        let mut m = Map { width: 5, height: 5, tiles: vec![Tile::Unbreakable; 25], start: (0, 2), exit: (4, 2) };
        for y in 1..4 {
            for x in 1..4 {
                m.tiles[y * 5 + x] = Tile::Dirt;
            }
        }
        m
    }

    fn center_of(cell: (i32, i32)) -> Vec2 {
        Vec2::new(cell.0 as f32 * TILE_SIZE + TILE_SIZE / 2.0, cell.1 as f32 * TILE_SIZE + TILE_SIZE / 2.0)
    }

    /// Position flush against the east edge of a mineable rock at `(3, 2)`
    /// (i.e. the player's right edge touches the rock's left edge).
    fn flush_east_of_rock() -> Vec2 {
        Vec2::new(3.0 * TILE_SIZE - HALF, 2.0 * TILE_SIZE + TILE_SIZE / 2.0)
    }

    #[test]
    fn pushes_a_mineable_rock_it_is_flush_against() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        let t = pushed_target(flush_east_of_rock(), Vec2::new(1.0, 0.0), &map, HALF);
        assert_eq!(t, Some((3, 2)));
    }

    #[test]
    fn unmineable_unbreakable_dirt_and_out_of_bounds_are_not_mined() {
        let mut map = open_map();
        // Not flush: player still centred in its own cell.
        assert_eq!(pushed_target(center_of((2, 2)), Vec2::new(1.0, 0.0), &map, HALF), None);

        map.set_tile(3, 2, Tile::Unmineable);
        assert_eq!(pushed_target(flush_east_of_rock(), Vec2::new(1.0, 0.0), &map, HALF), None);

        map.set_tile(3, 2, Tile::Unbreakable);
        assert_eq!(pushed_target(flush_east_of_rock(), Vec2::new(1.0, 0.0), &map, HALF), None);

        map.set_tile(3, 2, Tile::Dirt);
        assert_eq!(pushed_target(flush_east_of_rock(), Vec2::new(1.0, 0.0), &map, HALF), None);

        // Facing off the map at the very edge.
        map.set_tile(1, 0, Tile::Mineable);
        assert_eq!(pushed_target(center_of((0, 1)), Vec2::new(-1.0, 0.0), &map, HALF), None);
    }

    #[test]
    fn needs_to_be_flush_against_the_target() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);

        // Flush east of the rock and pushing east -> targeted.
        let flush = flush_east_of_rock();
        assert_eq!(pushed_target(flush, Vec2::new(1.0, 0.0), &map, HALF), Some((3, 2)));

        // Mid-cell (not touching the rock) pushing east -> nothing to dig.
        assert_eq!(pushed_target(center_of((2, 2)), Vec2::new(1.0, 0.0), &map, HALF), None);

        // Flush east but pushing west (away from the rock), other cell empty.
        assert_eq!(pushed_target(flush, Vec2::new(-1.0, 0.0), &map, HALF), None);
    }

    #[test]
    fn diagonal_facing_uses_dominant_axis() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        // Diagonal down-right: dominant axis is east when |x| > |y|.
        let f = Vec2::new(1.0, 0.2).normalize();
        assert_eq!(pushed_target(flush_east_of_rock(), f, &map, HALF), Some((3, 2)));
    }

    #[test]
    fn zero_facing_returns_none() {
        let map = open_map();
        assert_eq!(pushed_target(center_of((2, 2)), Vec2::ZERO, &map, HALF), None);
    }

    #[test]
    fn dominant_axis_maps_cardinals() {
        assert_eq!(dominant_axis(Vec2::new(3.0, 1.0)), (1, 0));
        assert_eq!(dominant_axis(Vec2::new(-3.0, 1.0)), (-1, 0));
        assert_eq!(dominant_axis(Vec2::new(1.0, 3.0)), (0, 1));
        assert_eq!(dominant_axis(Vec2::new(1.0, -3.0)), (0, -1));
        assert_eq!(dominant_axis(Vec2::ZERO), (0, 0));
    }
}
