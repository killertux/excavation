//! Asset identifiers for the sheets loaded in M1.
//!
//! These enums index frames within their respective atlases. The numeric order
//! must match `REQUIREMENTS.md` §15.3 (left-to-right frame order).

/// Frames in `assets/images/tiles/terrain_atlas.png`.
///
/// A single rock sprite is deliberately reused for both mineable and unmineable
/// rocks (they must look identical); that mapping happens at the game layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TileId {
    /// Mineable / unmineable rock (identical sprite).
    Rock,
    /// Excavated floor.
    Floor,
    /// Visible (decorative) wall — never mineable.
    Wall,
    /// Impassable border frame.
    Border,
    /// Spawn door.
    StartDoor,
    /// Exit door, closed state.
    ExitDoorClosed,
    /// Exit door, open state.
    ExitDoorOpen,
}

impl TileId {
    /// Number of frames in the terrain atlas.
    pub const COUNT: usize = 7;

    /// Index of this frame in the atlas (left-to-right).
    pub fn index(self) -> usize {
        match self {
            TileId::Rock => 0,
            TileId::Floor => 1,
            TileId::Wall => 2,
            TileId::Border => 3,
            TileId::StartDoor => 4,
            TileId::ExitDoorClosed => 5,
            TileId::ExitDoorOpen => 6,
        }
    }
}

/// Frames in `assets/images/characters/player_sheet.png`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerAnim {
    /// Standing idle.
    Idle,
    /// First walk frame.
    Walk1,
    /// Second walk frame.
    Walk2,
    /// Mining pose.
    Mining,
}

impl PlayerAnim {
    /// Number of frames in the player sheet.
    pub const COUNT: usize = 4;

    /// Index of this frame in the atlas (left-to-right).
    pub fn index(self) -> usize {
        match self {
            PlayerAnim::Idle => 0,
            PlayerAnim::Walk1 => 1,
            PlayerAnim::Walk2 => 2,
            PlayerAnim::Mining => 3,
        }
    }
}
