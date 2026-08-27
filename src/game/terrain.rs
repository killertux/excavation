//! Terrain autotiling (pure): map a cell and its cardinal neighbours to the tile
//! family + Wang-mask tile to draw.
//!
//! The new atlas has one row per **terrain family**, each containing a `base`
//! fill (column 0) plus a 16-tile **Wang** set (columns 1..16). A Wang tile is
//! selected by a 4-bit mask of which cardinal neighbours share the *same*
//! material (N=1, E=2, S=4, W=8); the mask's set bits mean "connected, no border"
//! so `wang[15]` is a fully-merged interior tile and `wang[0]` is an isolated
//! one bordered on all four sides. The atlas packs `wang[k]` at column `k + 1`,
//! with the plain `base` fill at column 0.
//!
//! There are three visual families:
//! - **Unbreakable** rock (`Unbreakable`) — the border ring and structures.
//! - **Mineable** rock (`Mineable`/`Unmineable`, visually identical).
//! - **Dirt** — flat, walkable, no Wang set (always the base fill).
//!
//! Rocks draw edges toward any differing material (a different rock family or
//! dirt); dirt is always drawn flat (rocks draw the transition toward it). The
//! 4-bit mask and the per-family row are the only things this module decides —
//! the concrete atlas `(row, col)` lives in the asset layer.

use crate::game::map::Tile;

/// The terrain-family + Wang-mask selection for a cell.
///
/// `Dirt` carries no mask (it is a flat fill). `Mineable`/`Unbreakable` carry a
/// 4-bit mask of which cardinal neighbours share the same material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainTile {
    /// Flat dirt path.
    Dirt,
    /// Mineable rock drawn with Wang mask `m` (0..15).
    Mineable(u8),
    /// Unbreakable rock drawn with Wang mask `m` (0..15).
    Unbreakable(u8),
}

/// Select the tile to draw for `center`, given its four cardinal neighbours.
pub fn tile_atlas(center: Tile, n: Tile, e: Tile, s: Tile, w: Tile) -> TerrainTile {
    match center {
        Tile::Mineable | Tile::Unmineable => {
            TerrainTile::Mineable(mask(n, e, s, w, |t| matches!(t, Tile::Mineable | Tile::Unmineable)))
        }
        Tile::Unbreakable => {
            TerrainTile::Unbreakable(mask(n, e, s, w, |t| t == Tile::Unbreakable))
        }
        Tile::Dirt => TerrainTile::Dirt,
    }
}

/// Build a 4-bit mask (N=1, E=2, S=4, W=8) where a bit is *set* when the
/// neighbour on that side is the **same material** (so the tile merges, no
/// border). `same` decides what "same material" means for a family.
fn mask(n: Tile, e: Tile, s: Tile, w: Tile, same: impl Fn(Tile) -> bool) -> u8 {
    let mut m = 0u8;
    if same(n) {
        m |= 1;
    }
    if same(e) {
        m |= 2;
    }
    if same(s) {
        m |= 4;
    }
    if same(w) {
        m |= 8;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_fills_map_directly() {
        let m = Tile::Mineable;
        let ub = Tile::Unbreakable;
        let d = Tile::Dirt;
        // A mineable cell fully surrounded by *mineable-family* rock merges.
        let inner = Tile::Mineable;
        assert_eq!(tile_atlas(Tile::Mineable, inner, inner, inner, inner), TerrainTile::Mineable(15));
        assert_eq!(tile_atlas(Tile::Unmineable, inner, inner, inner, inner), TerrainTile::Mineable(15));
        // Surrounded by unbreakable rock: no mineable merge -> isolated border.
        assert_eq!(tile_atlas(Tile::Mineable, ub, ub, ub, ub), TerrainTile::Mineable(0));
        // Unbreakable fully surrounded by unbreakable merges.
        assert_eq!(tile_atlas(Tile::Unbreakable, ub, ub, ub, ub), TerrainTile::Unbreakable(15));
        // Dirt is flat regardless of neighbours.
        assert_eq!(tile_atlas(Tile::Dirt, ub, m, d, m), TerrainTile::Dirt);
    }

    #[test]
    fn dirt_is_always_flat() {
        // Dirt is flat regardless of neighbours.
        let dirt = Tile::Dirt;
        let rock = Tile::Mineable;
        assert_eq!(tile_atlas(Tile::Dirt, rock, rock, rock, rock), TerrainTile::Dirt);
        assert_eq!(tile_atlas(Tile::Dirt, dirt, dirt, dirt, dirt), TerrainTile::Dirt);
    }

    #[test]
    fn mineable_mask_detects_same_rock_family() {
        let m = Tile::Mineable;
        let u = Tile::Unmineable;
        let d = Tile::Dirt;

        // All four neighbours mineable -> fully merged.
        assert_eq!(
            tile_atlas(Tile::Mineable, m, m, m, m),
            TerrainTile::Mineable(0b1111)
        );
        // Unmineable neighbours count as the same rock family.
        assert_eq!(
            tile_atlas(Tile::Mineable, u, u, u, u),
            TerrainTile::Mineable(0b1111)
        );
        // North needs a border -> that bit is not set.
        assert_eq!(
            tile_atlas(Tile::Mineable, d, m, m, m),
            TerrainTile::Mineable(0b1110)
        );
    }

    #[test]
    fn unbreakable_mask_ignores_other_materials() {
        let ub = Tile::Unbreakable;
        let rk = Tile::Mineable;
        // All four unbreakable -> fully merged.
        assert_eq!(tile_atlas(Tile::Unbreakable, ub, ub, ub, ub), TerrainTile::Unbreakable(15));
        // Rock neighbour is a different material -> that side needs a border.
        assert_eq!(tile_atlas(Tile::Unbreakable, rk, ub, ub, ub), TerrainTile::Unbreakable(14));
    }

    #[test]
    fn single_edge_masks_have_expected_bits() {
        let rk = Tile::Mineable;
        let f = Tile::Dirt;
        // Rock above only.
        assert_eq!(tile_atlas(Tile::Mineable, rk, f, f, f), TerrainTile::Mineable(1));
        // Rock to the right only.
        assert_eq!(tile_atlas(Tile::Mineable, f, rk, f, f), TerrainTile::Mineable(2));
        // Rock below only.
        assert_eq!(tile_atlas(Tile::Mineable, f, f, rk, f), TerrainTile::Mineable(4));
        // Rock to the left only.
        assert_eq!(tile_atlas(Tile::Mineable, f, f, f, rk), TerrainTile::Mineable(8));
    }
}
