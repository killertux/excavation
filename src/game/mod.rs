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

/// Logical tile size, in world pixels. Every cell is a 32×32 tile (the atlas
/// cells are 32 px, rendered at native resolution).
pub const TILE_SIZE: f32 = 32.0;
