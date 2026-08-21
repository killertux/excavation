//! Map grid and tile types (pure, no rendering).

use crate::assets::ids::TileId;

/// One cell of the map grid. Each cell is a 16×16 px tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    /// A rock. Looks identical whether mineable or unmineable (see requirements
    /// §4.2). Blocks movement in M1.
    Rock,
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
        matches!(self, Tile::Rock | Tile::Wall | Tile::Border)
    }

    /// The terrain-atlas frame used to render this tile.
    pub fn tile_id(self) -> TileId {
        match self {
            Tile::Rock => TileId::Rock,
            Tile::Excavated => TileId::Floor,
            Tile::Wall => TileId::Wall,
            Tile::Border => TileId::Border,
            Tile::StartDoor => TileId::StartDoor,
            Tile::ExitDoor => TileId::ExitDoorClosed,
        }
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

/// A hand-written placeholder map for M1.
///
/// 30×20 grid with a full border, a start door on the bottom edge, an exit door
/// on the top edge, a pre-excavated walkable interior, and a few rocks and
/// visible walls to exercise collision. No generation and no TOML yet.
pub fn placeholder_map() -> Map {
    const W: usize = 30;
    const H: usize = 20;

    let mut tiles = vec![Tile::Border; W * H];

    // Interior is pre-excavated walkable floor.
    for y in 1..H - 1 {
        for x in 1..W - 1 {
            tiles[y * W + x] = Tile::Excavated;
        }
    }

    // Doors sit on the border. Player spawns at the start door; the exit is the
    // goal (a path of excavated cells connects the two in this placeholder).
    let start = (15, H - 1);
    let exit = (5, 0);
    tiles[start.1 * W + start.0] = Tile::StartDoor;
    tiles[exit.1 * W + exit.0] = Tile::ExitDoor;

    // Scattered rocks and a small wall cluster to exercise collision. These do
    // not fully block the start->exit path (the interior is otherwise open).
    let rocks = [(8, 5), (9, 5), (10, 5), (8, 6), (9, 6), (20, 10), (21, 10), (22, 10), (12, 14), (13, 14)];
    for (x, y) in rocks {
        tiles[y * W + x] = Tile::Rock;
    }
    let walls = [(25, 8), (26, 8), (25, 9)];
    for (x, y) in walls {
        tiles[y * W + x] = Tile::Wall;
    }

    Map { width: W, height: H, tiles }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_solid_covers_rock_wall_border() {
        assert!(Tile::Rock.solid());
        assert!(Tile::Wall.solid());
        assert!(Tile::Border.solid());
        assert!(!Tile::Excavated.solid());
        assert!(!Tile::StartDoor.solid());
        assert!(!Tile::ExitDoor.solid());
    }

    #[test]
    fn tile_id_maps_to_expected_frames() {
        assert_eq!(Tile::Rock.tile_id(), TileId::Rock);
        assert_eq!(Tile::Excavated.tile_id(), TileId::Floor);
        assert_eq!(Tile::Wall.tile_id(), TileId::Wall);
        assert_eq!(Tile::Border.tile_id(), TileId::Border);
        assert_eq!(Tile::StartDoor.tile_id(), TileId::StartDoor);
        assert_eq!(Tile::ExitDoor.tile_id(), TileId::ExitDoorClosed);
    }

    #[test]
    fn placeholder_map_has_border_all_edges_and_doors() {
        let m = placeholder_map();
        assert_eq!(m.width, 30);
        assert_eq!(m.height, 20);

        let start = m.start_pos().expect("start door present");
        let exit = m.exit_pos().expect("exit door present");

        // The whole outer ring is a border, except the door cells themselves.
        for x in 0..m.width as i32 {
            let top = if (x as usize, 0) == exit { Tile::ExitDoor } else { Tile::Border };
            assert_eq!(m.tile(x, 0), top);
            let bottom = if (x as usize, m.height - 1) == start {
                Tile::StartDoor
            } else {
                Tile::Border
            };
            assert_eq!(m.tile(x, m.height as i32 - 1), bottom);
        }
        for y in 0..m.height as i32 {
            assert_eq!(m.tile(0, y), Tile::Border);
            assert_eq!(m.tile(m.width as i32 - 1, y), Tile::Border);
        }
        // The interior corners are walkable (not border).
        assert_eq!(m.tile(1, 1), Tile::Excavated);
    }

    #[test]
    fn out_of_bounds_reads_as_border() {
        let m = placeholder_map();
        assert_eq!(m.tile(-1, 0), Tile::Border);
        assert_eq!(m.tile(0, -1), Tile::Border);
        assert_eq!(m.tile(m.width as i32, 0), Tile::Border);
        assert_eq!(m.tile(0, m.height as i32), Tile::Border);
        assert!(m.is_solid(-1, 0));
        assert!(!m.in_bounds(-1, 0));
    }

    #[test]
    fn set_tile_panics_out_of_bounds() {
        let mut m = placeholder_map();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            m.set_tile(m.width as i32, 0, Tile::Rock);
        }));
        assert!(result.is_err());
    }
}
