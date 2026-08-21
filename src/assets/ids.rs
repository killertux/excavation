//! Asset identifiers for the sheets loaded in M2 (directional atlases).
//!
//! - The terrain atlas is a 7×6 modular grid (see [`crate::game::terrain`] for
//!   the autotile selection).
//! - The player sheet is a 4×5 grid: rows are UP, DOWN, RIGHT, LEFT; columns are
//!   idle, walk×2, mining raise/impact×2.
//! - The beast sheet is a 4×3 grid: rows are UP, DOWN, RIGHT, LEFT; columns are
//!   idle, walk×2.

use macroquad::prelude::Vec2;

/// One of the four cardineal facing directions. Rows in the character sheets are
/// laid out UP, DOWN, RIGHT, LEFT in that order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    Down,
    Right,
    Left,
}

impl Direction {
    /// Row index in a character sheet (UP, DOWN, RIGHT, LEFT order).
    pub fn index(self) -> usize {
        match self {
            Direction::Up => 0,
            Direction::Down => 1,
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

/// Frames in a walk/mine cycle (kept consistent across all animations).
pub const CYCLE_FRAMES: usize = 10;

/// Player animation state (the "column" axis of the player sheet).
///
/// `Walk`/`Mine` carry a phase (0..[`CYCLE_FRAMES`]) that advances each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerMotion {
    Idle,
    Walk(u8),
    Mine(u8),
}

impl PlayerMotion {
    /// Column index in the player sheet.
    pub fn column(self) -> usize {
        match self {
            PlayerMotion::Idle => 0,
            PlayerMotion::Walk(p) => 1 + (p as usize % CYCLE_FRAMES),
            PlayerMotion::Mine(p) => 11 + (p as usize % CYCLE_FRAMES),
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
            BeastMotion::Walk(p) => 1 + (p as usize % CYCLE_FRAMES),
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
    fn direction_row_order_is_up_down_right_left() {
        assert_eq!(Direction::Up.index(), 0);
        assert_eq!(Direction::Down.index(), 1);
        assert_eq!(Direction::Right.index(), 2);
        assert_eq!(Direction::Left.index(), 3);
    }

    #[test]
    fn player_motion_columns_match_sheet() {
        assert_eq!(PlayerMotion::Idle.column(), 0);
        assert_eq!(PlayerMotion::Walk(0).column(), 1);
        assert_eq!(PlayerMotion::Walk(9).column(), 10);
        assert_eq!(PlayerMotion::Mine(0).column(), 11);
        assert_eq!(PlayerMotion::Mine(9).column(), 20);
        // Phases wrap within the 10-frame cycle.
        assert_eq!(PlayerMotion::Walk(10).column(), 1);
        assert_eq!(PlayerMotion::Mine(10).column(), 11);
    }

    #[test]
    fn beast_motion_columns_match_sheet() {
        assert_eq!(BeastMotion::Idle.column(), 0);
        assert_eq!(BeastMotion::Walk(0).column(), 1);
        assert_eq!(BeastMotion::Walk(9).column(), 10);
        assert_eq!(BeastMotion::Walk(10).column(), 1);
    }

    #[test]
    fn anim_row_col_combine() {
        let a = PlayerAnim { dir: Direction::Left, motion: PlayerMotion::Mine(1) };
        assert_eq!(a.row(), 3);
        assert_eq!(a.col(), 12);
    }
}
