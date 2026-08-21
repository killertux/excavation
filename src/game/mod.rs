//! Game core: map, generation, pathfinding, mining, player, camera. Pure (no
//! rendering, no input polling — those live in `app`/`input`).

pub mod beast;
pub mod camera;
pub mod generation;
pub mod map;
pub mod mining;
pub mod movement;
pub mod pathfinding;
pub mod player;
pub mod terrain;

/// Logical tile size, in world pixels. Every cell is a 16×16 tile.
pub const TILE_SIZE: f32 = 16.0;
