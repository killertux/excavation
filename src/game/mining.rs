//! Mining mechanics (pure): target selection and the in-progress mining state.
//!
//! The player mines by facing an adjacent mineable rock and holding the mine
//! action. Targeting is **facing-based** (deterministic and direction-aware)
//! rather than "nearest adjacent rock", so mining a specific rock is predictable.

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

/// The mineable cell the player is facing, if any.
///
/// `pos` is the player's world-pixel center; `facing` is a (normalized)
/// direction vector. The player's cell is the tile containing its center, and
/// the target is that cell offset by the rounded facing (one of the 8
/// neighbours). Returns `Some` only when that target is actually `Mineable`.
pub fn mine_target(pos: Vec2, facing: Vec2, map: &Map) -> Option<(i32, i32)> {
    let cell = ((pos.x / TILE_SIZE).floor() as i32, (pos.y / TILE_SIZE).floor() as i32);
    let dir = (facing.x.round() as i32, facing.y.round() as i32);
    if dir == (0, 0) {
        return None;
    }
    let target = (cell.0 + dir.0, cell.1 + dir.1);
    if map.tile(target.0, target.1).mineable() {
        Some(target)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::map::Tile;

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

    #[test]
    fn returns_facing_cell_when_mineable() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        let t = mine_target(center_of((2, 2)), Vec2::new(1.0, 0.0), &map);
        assert_eq!(t, Some((3, 2)));
    }

    #[test]
    fn returns_none_for_unmineable() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Unmineable);
        assert_eq!(mine_target(center_of((2, 2)), Vec2::new(1.0, 0.0), &map), None);
    }

    #[test]
    fn returns_none_for_unmineable_unbreakable_dirt_and_out_of_bounds() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Unmineable);
        assert_eq!(mine_target(center_of((2, 2)), Vec2::new(1.0, 0.0), &map), None);

        map.set_tile(3, 2, Tile::Unbreakable);
        assert_eq!(mine_target(center_of((2, 2)), Vec2::new(1.0, 0.0), &map), None);

        map.set_tile(3, 2, Tile::Dirt);
        assert_eq!(mine_target(center_of((2, 2)), Vec2::new(1.0, 0.0), &map), None);

        // Facing the unbreakable border from the edge.
        assert_eq!(mine_target(center_of((1, 3)), Vec2::new(0.0, 1.0), &map), None);
        // Facing off the map at the very edge.
        assert_eq!(mine_target(center_of((0, 1)), Vec2::new(-1.0, 0.0), &map), None);
    }

    #[test]
    fn diagonal_facing_rounds_to_nearest_neighbor() {
        let mut map = open_map();
        map.set_tile(3, 3, Tile::Mineable);
        let t = mine_target(center_of((2, 2)), Vec2::new(1.0, 1.0).normalize(), &map);
        assert_eq!(t, Some((3, 3)));
    }

    #[test]
    fn zero_facing_returns_none() {
        let map = open_map();
        assert_eq!(mine_target(center_of((2, 2)), Vec2::ZERO, &map), None);
    }
}
