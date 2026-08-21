//! Game core: map, player, camera. All pure (no rendering).

pub mod camera;
pub mod map;
pub mod player;

/// Logical tile size, in world pixels. Every cell is a 16×16 tile.
pub const TILE_SIZE: f32 = 16.0;
