//! Asset identifiers for the sheets loaded in M2 (the combined atlas).
//!
//! Character sheets are laid out **rows = facing (Down, Up, Right, Left)** and
//! columns = animation frames:
//! - The miner row has 11 columns: `base`(0), `idle`×2 (1..2), `walk`×4 (3..6),
//!   `mine`×4 (7..10).
//! - The beast (dino) row has 5 columns: `base`(0), `walk`×4 (1..4).
//!
//! The atlas packs these 32 px cells; the asset layer downscales to 16 px. The
//! `(row, col)` helpers here assume that ordering.

use macroquad::prelude::Vec2;

/// One of the four facing directions. Character-sheet rows are laid out
/// **Down, Up, Right, Left** in that atlas order (the artist's Front, Back,
/// Right, Left, where Front faces the camera = Down in a top-down game).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Down,
    Up,
    Right,
    Left,
}

impl Direction {
    /// Row index in a character sheet (Down, Up, Right, Left order).
    pub fn index(self) -> usize {
        match self {
            Direction::Down => 0,
            Direction::Up => 1,
            Direction::Right => 2,
            Direction::Left => 3,
        }
    }

    /// Pick a cardinal direction from a movement vector, choosing the dominant
    /// axis. Returns `Down` for a zero vector (a sensible default facing).
    pub fn from_vec2(v: Vec2) -> Direction {
        if v.x.abs() > v.y.abs() {
            if v.x > 0.0 {
                Direction::Right
            } else {
                Direction::Left
            }
        } else if v.y.abs() > 0.0 {
            if v.y > 0.0 {
                Direction::Down
            } else {
                Direction::Up
            }
        } else {
            Direction::Down
        }
    }
}

/// Frame counts per animation in the miner sheet.
pub const IDLE_FRAMES: usize = 2;
pub const WALK_FRAMES: usize = 4;
pub const MINE_FRAMES: usize = 4;

/// Player animation state (the "column" axis of the miner sheet).
///
/// Each variant carries a phase (0.. its frame count) that advances each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerMotion {
    Idle(u8),
    Walk(u8),
    Mine(u8),
}

impl PlayerMotion {
    /// Column index in the miner sheet.
    pub fn column(self) -> usize {
        match self {
            // base at 0; idle (2) at 1..2, walk (4) at 3..6, mine (4) at 7..10.
            PlayerMotion::Idle(p) => 1 + (p as usize % IDLE_FRAMES),
            PlayerMotion::Walk(p) => 3 + (p as usize % WALK_FRAMES),
            PlayerMotion::Mine(p) => 7 + (p as usize % MINE_FRAMES),
        }
    }
}

/// A complete player pose: which direction + which motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerAnim {
    pub dir: Direction,
    pub motion: PlayerMotion,
}

impl PlayerAnim {
    pub fn row(self) -> usize {
        self.dir.index()
    }
    pub fn col(self) -> usize {
        self.motion.column()
    }
}

/// Beast animation state (the "column" axis of the beast sheet).
///
/// The beast has a single stopped sprite (`base`, column 0) and a 4-frame walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BeastMotion {
    Idle,
    Walk(u8),
}

impl BeastMotion {
    /// Column index in the beast sheet.
    pub fn column(self) -> usize {
        match self {
            BeastMotion::Idle => 0,
            BeastMotion::Walk(p) => 1 + (p as usize % WALK_FRAMES),
        }
    }
}

/// A complete beast pose: which direction + which motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeastAnim {
    pub dir: Direction,
    pub motion: BeastMotion,
}

impl BeastAnim {
    pub fn row(self) -> usize {
        self.dir.index()
    }
    pub fn col(self) -> usize {
        self.motion.column()
    }
}

/// Pickup sprites (the M4 `PickupId` atlas row), sliced from `assets/` — the
/// source rects live in the asset loader. These map to `Assets::pickup`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PickupId {
    Gold,
}

/// HUD/shop icon sprites (the M4 icon row). These map to `Assets::icon`.
///
/// `Heart` and `BuyLives` are both hearts (the latter is the shop "buy a life"
/// icon). The ordering must match the loader's icon rect list exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconId {
    /// Super Pickaxe.
    SuperPick,
    /// Jar of stench (Sticky Smell).
    StickySmell,
    /// Heart (lives HUD).
    Heart,
    /// Buy-a-life heart (shop).
    BuyLives,
    /// Boot (Walk Speed upgrade).
    WalkSpeed,
    /// Pickaxe (Mining Speed upgrade).
    MiningSpeed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_from_vec2_picks_dominant_axis() {
        assert_eq!(Direction::from_vec2(Vec2::new(1.0, 0.0)), Direction::Right);
        assert_eq!(Direction::from_vec2(Vec2::new(-1.0, 0.0)), Direction::Left);
        assert_eq!(Direction::from_vec2(Vec2::new(0.0, 1.0)), Direction::Down);
        assert_eq!(Direction::from_vec2(Vec2::new(0.0, -1.0)), Direction::Up);
        assert_eq!(Direction::from_vec2(Vec2::ZERO), Direction::Down);
        // Diagonal prefers the larger axis component.
        assert_eq!(Direction::from_vec2(Vec2::new(3.0, 1.0)), Direction::Right);
        assert_eq!(Direction::from_vec2(Vec2::new(1.0, 3.0)), Direction::Down);
    }

    #[test]
    fn direction_row_order_is_down_up_right_left() {
        assert_eq!(Direction::Down.index(), 0);
        assert_eq!(Direction::Up.index(), 1);
        assert_eq!(Direction::Right.index(), 2);
        assert_eq!(Direction::Left.index(), 3);
    }

    #[test]
    fn player_motion_columns_match_sheet() {
        assert_eq!(PlayerMotion::Idle(0).column(), 1);
        assert_eq!(PlayerMotion::Idle(1).column(), 2);
        assert_eq!(PlayerMotion::Walk(0).column(), 3);
        assert_eq!(PlayerMotion::Walk(3).column(), 6);
        assert_eq!(PlayerMotion::Mine(0).column(), 7);
        assert_eq!(PlayerMotion::Mine(3).column(), 10);
        // Phases wrap within their frame counts.
        assert_eq!(PlayerMotion::Walk(4).column(), 3);
        assert_eq!(PlayerMotion::Mine(4).column(), 7);
        assert_eq!(PlayerMotion::Idle(2).column(), 1);
    }

    #[test]
    fn beast_motion_columns_match_sheet() {
        assert_eq!(BeastMotion::Idle.column(), 0);
        assert_eq!(BeastMotion::Walk(0).column(), 1);
        assert_eq!(BeastMotion::Walk(3).column(), 4);
        assert_eq!(BeastMotion::Walk(4).column(), 1);
    }

    #[test]
    fn anim_row_col_combine() {
        let a = PlayerAnim {
            dir: Direction::Left,
            motion: PlayerMotion::Walk(1),
        };
        assert_eq!(a.row(), 3);
        assert_eq!(a.col(), 4);
    }
}
