//! Map grid and tile types (pure, no rendering).

/// One cell of the map grid. Each cell is a 16×16 px tile.
///
/// Mineable and unmineable rocks are visually identical (both render the rock
/// atlas fill); the distinction is gameplay data, not appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    /// A diggable rock. Blocks movement until mined through.
    Mineable,
    /// A rock that looks exactly like a mineable one but cannot be dug.
    Unmineable,
    /// An excavated (dug-out) cell — open floor.
    Excavated,
    /// A visible (decorative) wall — never mineable, blocks movement.
    Wall,
    /// The outer impassable frame.
    Border,
    /// The spawn door on the border.
    StartDoor,
    /// The exit door on the border.
    ExitDoor,
}

impl Tile {
    /// Whether this tile blocks player movement.
    pub fn solid(self) -> bool {
        matches!(
            self,
            Tile::Mineable | Tile::Unmineable | Tile::Wall | Tile::Border
        )
    }

    /// Whether this tile can be mined (dug) by the player.
    pub fn mineable(self) -> bool {
        matches!(self, Tile::Mineable)
    }
}

/// A rectangular grid of tiles, stored row-major (`index = y * width + x`).
#[derive(Debug, Clone)]
pub struct Map {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<Tile>,
}

impl Map {
    /// Returns true if `(x, y)` is within the grid.
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height
    }

    /// Returns the tile at `(x, y)`. Out-of-bounds coordinates return `Border`
    /// so that collision logic treats the edges of the world as solid.
    pub fn tile(&self, x: i32, y: i32) -> Tile {
        if !self.in_bounds(x, y) {
            return Tile::Border;
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
    pub fn count(&self, tile: Tile) -> usize {
        self.tiles.iter().filter(|&&t| t == tile).count()
    }

    /// Grid coordinates of the start door, if present.
    pub fn start_pos(&self) -> Option<(usize, usize)> {
        self.find(Tile::StartDoor)
    }

    /// Grid coordinates of the exit door, if present.
    pub fn exit_pos(&self) -> Option<(usize, usize)> {
        self.find(Tile::ExitDoor)
    }

    fn find(&self, target: Tile) -> Option<(usize, usize)> {
        for y in 0..self.height {
            for x in 0..self.width {
                if self.tiles[y * self.width + x] == target {
                    return Some((x, y));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_map() -> Map {
        Map {
            width: 5,
            height: 5,
            tiles: vec![Tile::Border; 25],
        }
    }

    #[test]
    fn tile_solid_covers_rocks_wall_border() {
        assert!(Tile::Mineable.solid());
        assert!(Tile::Unmineable.solid());
        assert!(Tile::Wall.solid());
        assert!(Tile::Border.solid());
        assert!(!Tile::Excavated.solid());
        assert!(!Tile::StartDoor.solid());
        assert!(!Tile::ExitDoor.solid());
    }

    #[test]
    fn mineable_is_only_true_for_mineable() {
        assert!(Tile::Mineable.mineable());
        assert!(!Tile::Unmineable.mineable());
        assert!(!Tile::Wall.mineable());
        assert!(!Tile::Border.mineable());
        assert!(!Tile::Excavated.mineable());
        assert!(!Tile::StartDoor.mineable());
        assert!(!Tile::ExitDoor.mineable());
    }

    #[test]
    fn out_of_bounds_reads_as_solid_border() {
        let m = test_map();
        assert_eq!(m.tile(-1, 0), Tile::Border);
        assert_eq!(m.tile(0, -1), Tile::Border);
        assert_eq!(m.tile(5, 0), Tile::Border);
        assert_eq!(m.tile(0, 5), Tile::Border);
        assert!(m.is_solid(-1, 0));
        assert!(!m.in_bounds(-1, 0));
    }

    #[test]
    fn set_tile_and_accessors_round_trip() {
        let mut m = test_map();
        m.set_tile(2, 2, Tile::Excavated);
        assert_eq!(m.tile(2, 2), Tile::Excavated);
        assert_eq!(m.count(Tile::Excavated), 1);
        assert_eq!(m.count(Tile::Border), 24);
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
    fn find_doors() {
        let mut m = test_map();
        m.set_tile(0, 2, Tile::StartDoor);
        m.set_tile(4, 2, Tile::ExitDoor);
        assert_eq!(m.start_pos(), Some((0, 2)));
        assert_eq!(m.exit_pos(), Some((4, 2)));
    }
}
