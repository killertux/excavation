//! Terrain autotiling (pure): map a cell and its neighbours to a terrain-atlas
//! tile `(row, col)`.
//!
//! The atlas is a 7×6 grid:
//!   row 0  : base fills (rock, floor, wall, border, start door, exit door)
//!   rows 1–2 : floor ↔ light-rock transitions
//!   rows 3–4 : floor ↔ border (dark rocky exterior) transitions
//!   rows 5–6 : floor ↔ masonry (visible wall) transitions
//!
//! A floor (excavated) cell is drawn with its neighbouring solid material's edge
//! baked in. We pick the material family from the solid cardinal neighbours,
//! then choose one of the transition shapes from a bitmask of which sides are
//! solid.
//!
//! NOTE: the exact shape→atlas-column mapping is empirical and verified against
//! the rendered map; the [`transition_shape`] table is the single place to tune
//! it. Commented candidate assignments are kept so adjustments are quick.

use crate::game::map::Tile;

/// Atlas coordinates of the base fills.
pub const ROCK: (usize, usize) = (0, 0);
pub const FLOOR: (usize, usize) = (0, 1);
pub const WALL: (usize, usize) = (0, 2);
pub const BORDER: (usize, usize) = (0, 3);
pub const START_DOOR: (usize, usize) = (0, 4);
pub const EXIT_DOOR: (usize, usize) = (0, 5);

/// First row of each floor-transition family.
const ROCK_FAMILY: usize = 1;
const BORDER_FAMILY: usize = 3;
const WALL_FAMILY: usize = 5;

/// Terrain-atlas `(row, col)` for a cell, given its own `Tile` and the tiles of
/// its four cardinal neighbours `(n, e, s, w)`.
pub fn tile_atlas(center: Tile, n: Tile, e: Tile, s: Tile, w: Tile) -> (usize, usize) {
    match center {
        Tile::Mineable | Tile::Unmineable => ROCK,
        Tile::Wall => WALL,
        Tile::Border => BORDER,
        Tile::StartDoor => START_DOOR,
        Tile::ExitDoor => EXIT_DOOR,
        Tile::Excavated => match edge_family(n, e, s, w) {
            Some(base) => transition_shape(base, neighbour_mask(n, e, s, w)),
            None => FLOOR,
        },
    }
}

/// Whether a tile is the kind a floor edge draws around (rock/wall/border).
fn is_edge(t: Tile) -> bool {
    matches!(t, Tile::Mineable | Tile::Unmineable | Tile::Wall | Tile::Border)
}

/// Pick the material family (its first atlas row) for a floor cell's edges.
///
/// Prefers rock (the common case: floor carved through rock), then wall, then
/// border. A floor cell typically borders a single material; the dominant match
/// is chosen when several are present.
fn edge_family(n: Tile, e: Tile, s: Tile, w: Tile) -> Option<usize> {
    if [n, e, s, w].iter().any(|&t| matches!(t, Tile::Mineable | Tile::Unmineable)) {
        return Some(ROCK_FAMILY);
    }
    if [n, e, s, w].iter().any(|&t| t == Tile::Wall) {
        return Some(WALL_FAMILY);
    }
    if [n, e, s, w].iter().any(|&t| t == Tile::Border) {
        return Some(BORDER_FAMILY);
    }
    None
}

/// A 4-bit mask of which cardinal neighbours are solid: N=1, E=2, S=4, W=8.
fn neighbour_mask(n: Tile, e: Tile, s: Tile, w: Tile) -> u8 {
    let mut m = 0u8;
    if is_edge(n) {
        m |= 1;
    }
    if is_edge(e) {
        m |= 2;
    }
    if is_edge(s) {
        m |= 4;
    }
    if is_edge(w) {
        m |= 8;
    }
    m
}

/// Atlas `(row, col)` for a floor cell with a `mask` of solid cardinal
/// neighbours, inside a transition family whose first row is `base`.
fn transition_shape(base: usize, mask: u8) -> (usize, usize) {
    // Column 0 of the family's two rows is the "straight" edge family.
    let (row_off, col) = match mask {
        0 => (0, 1),       // no solid neighbours -> plain floor fill
        1 => (0, 0),       // N edge
        2 => (0, 2),       // E edge
        4 => (1, 0),       // S edge
        8 => (0, 3),       // W edge
        3 => (0, 1),       // N+E convex corner
        6 => (1, 2),       // E+S convex corner
        12 => (1, 4),      // S+W convex corner
        9 => (0, 4),       // W+N convex corner
        7 => (1, 1),       // N+E+S
        14 => (1, 3),      // E+S+W
        13 => (1, 5),      // S+W+N
        11 => (0, 5),      // W+N+E
        15 => (0, 1),      // surrounded
        _ => (0, 1),
    };
    (base + row_off, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_fills_map_directly() {
        let b = Tile::Border;
        assert_eq!(tile_atlas(Tile::Mineable, b, b, b, b), ROCK);
        assert_eq!(tile_atlas(Tile::Unmineable, b, b, b, b), ROCK);
        assert_eq!(tile_atlas(Tile::Wall, b, b, b, b), WALL);
        assert_eq!(tile_atlas(Tile::Border, b, b, b, b), BORDER);
        assert_eq!(tile_atlas(Tile::StartDoor, b, b, b, b), START_DOOR);
        assert_eq!(tile_atlas(Tile::ExitDoor, b, b, b, b), EXIT_DOOR);
    }

    #[test]
    fn open_floor_uses_floor_fill() {
        let f = Tile::Excavated;
        assert_eq!(tile_atlas(Tile::Excavated, f, f, f, f), FLOOR);
    }

    #[test]
    fn floor_with_single_rock_edge_uses_rock_transition() {
        let f = Tile::Excavated;
        let rk = Tile::Mineable;
        // Rock above.
        let (r, c) = tile_atlas(Tile::Excavated, rk, f, f, f);
        assert_eq!(r, ROCK_FAMILY);
        assert_eq!(c, 0);
        // Rock to the right.
        let (r, c) = tile_atlas(Tile::Excavated, f, rk, f, f);
        assert_eq!(r, ROCK_FAMILY);
        assert_eq!(c, 2);
    }

    #[test]
    fn floor_next_to_wall_uses_wall_family() {
        let f = Tile::Excavated;
        let wall = Tile::Wall;
        let (r, _c) = tile_atlas(Tile::Excavated, wall, f, f, f);
        assert_eq!(r, WALL_FAMILY);
    }

    #[test]
    fn floor_next_to_border_uses_border_family() {
        let f = Tile::Excavated;
        let bd = Tile::Border;
        let (r, _c) = tile_atlas(Tile::Excavated, f, bd, f, f);
        assert_eq!(r, BORDER_FAMILY);
    }
}
