//! Map grid and tile types (pure, no rendering).
//!
//! The map now has **three visual** terrains but four gameplay states:
//!
//! - `Unbreakable` rock: the border ring and internal structures. Solid, never
//!   mined, and drawn with the unbreakable-rock family.
//! - `Mineable`/`Unmineable` rock: visually identical (both render the mineable
//!   rock family) — the difference is purely gameplay (whether it can be dug).
//! - `Dirt`: the excavated walkable path, drawn as a flat dirt fill.

use std::collections::HashSet;

/// One cell of the map grid. Each cell is a 16×16 px tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    /// A diggable rock. Blocks movement until mined through.
    Mineable,
    /// A rock that looks exactly like a mineable one but cannot be dug.
    Unmineable,
    /// Unbreakable rock: the map border ring and internal structures. Solid,
    /// never mineable, drawn with the unbreakable-rock wang family.
    Unbreakable,
    /// The excavated (dug-out) path — open, walkable dirt.
    Dirt,
}

impl Tile {
    /// Whether this tile blocks player movement.
    pub fn solid(self) -> bool {
        matches!(self, Tile::Mineable | Tile::Unmineable | Tile::Unbreakable)
    }
}

/// A rectangular grid of tiles, stored row-major (`index = y * width + x`).
#[derive(Debug, Clone)]
pub struct Map {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<Tile>,
    /// Grid coords of the start gap (a `Dirt` hole in the border ring).
    pub start: (usize, usize),
    /// Grid coords of the exit gap (a `Dirt` hole in the border ring).
    pub exit: (usize, usize),
    /// Grid coords of cells whose mineable rock hides gold (M4). A cell is
    /// present iff its rock is currently concealing a gold pickup.
    pub gold: HashSet<(usize, usize)>,
}

impl Map {
    /// A map whose interior/border is entirely `Unbreakable`, with empty gold
    /// and the given start/exit gaps. Callers replace `tiles` as needed. The
    /// border ring is populated with `Unbreakable`, so a freshly-constructed map
    /// is fully solid.
    ///
    /// Used by tests to build a small solid map quickly.
    #[allow(dead_code)]
    pub fn new(width: usize, height: usize, start: (usize, usize), exit: (usize, usize)) -> Map {
        Map {
            width,
            height,
            tiles: vec![Tile::Unbreakable; width * height],
            start,
            exit,
            gold: HashSet::new(),
        }
    }

    /// Returns true if `(x, y)` is within the grid.
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height
    }

    /// Returns the tile at `(x, y)`. Out-of-bounds coordinates return
    /// `Unbreakable` so collision logic treats the world edges as solid rock.
    pub fn tile(&self, x: i32, y: i32) -> Tile {
        if !self.in_bounds(x, y) {
            return Tile::Unbreakable;
        }
        self.tiles[y as usize * self.width + x as usize]
    }

    /// Sets the tile at `(x, y)`. Panics if out of bounds.
    pub fn set_tile(&mut self, x: i32, y: i32, tile: Tile) {
        assert!(self.in_bounds(x, y), "set_tile out of bounds: ({x}, {y})");
        self.tiles[y as usize * self.width + x as usize] = tile;
    }

    /// Whether `(x, y)` is a solid tile (or out of bounds).
    pub fn is_solid(&self, x: i32, y: i32) -> bool {
        self.tile(x, y).solid()
    }

    /// Number of cells occupied by a given tile kind.
    #[allow(dead_code)]
    pub fn count(&self, tile: Tile) -> usize {
        self.tiles.iter().filter(|&&t| t == tile).count()
    }

    /// Grid coordinates of the start gap.
    pub fn start_pos(&self) -> (usize, usize) {
        self.start
    }

    /// Grid coordinates of the exit gap.
    pub fn exit_pos(&self) -> (usize, usize) {
        self.exit
    }

    /// Whether the cell at `(x, y)` currently hides gold.
    #[allow(dead_code)]
    pub fn has_gold(&self, x: usize, y: usize) -> bool {
        self.gold.contains(&(x, y))
    }

    /// Remove gold from `(x, y)`, returning whether it was present. Gold is
    /// consumed exactly once: the second call returns `false`.
    pub fn take_gold(&mut self, x: usize, y: usize) -> bool {
        self.gold.remove(&(x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_map() -> Map {
        Map::new(5, 5, (0, 2), (4, 2))
    }

    #[test]
    fn tile_solid_covers_rocks_and_border() {
        assert!(Tile::Mineable.solid());
        assert!(Tile::Unmineable.solid());
        assert!(Tile::Unbreakable.solid());
        assert!(!Tile::Dirt.solid());
    }

    #[test]
    fn out_of_bounds_reads_as_solid_rock() {
        let m = test_map();
        assert_eq!(m.tile(-1, 0), Tile::Unbreakable);
        assert_eq!(m.tile(0, -1), Tile::Unbreakable);
        assert_eq!(m.tile(5, 0), Tile::Unbreakable);
        assert_eq!(m.tile(0, 5), Tile::Unbreakable);
        assert!(m.is_solid(-1, 0));
        assert!(!m.in_bounds(-1, 0));
    }

    #[test]
    fn set_tile_and_accessors_round_trip() {
        let mut m = test_map();
        m.set_tile(2, 2, Tile::Dirt);
        assert_eq!(m.tile(2, 2), Tile::Dirt);
        assert_eq!(m.count(Tile::Dirt), 1);
        assert_eq!(m.count(Tile::Unbreakable), 24);
    }

    #[test]
    fn set_tile_panics_out_of_bounds() {
        let mut m = test_map();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            m.set_tile(5, 0, Tile::Mineable);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn find_gaps() {
        let m = test_map();
        assert_eq!(m.start_pos(), (0, 2));
        assert_eq!(m.exit_pos(), (4, 2));
    }

    #[test]
    fn gold_is_present_then_consumed_once() {
        let mut m = test_map();
        m.gold.insert((2, 2));
        assert!(m.has_gold(2, 2));
        assert!(!m.has_gold(1, 1));
        assert!(m.take_gold(2, 2), "gold present -> consumed");
        assert!(!m.has_gold(2, 2), "gold gone after take");
        assert!(
            !m.take_gold(2, 2),
            "second take returns false (no double collect)"
        );
    }
}
