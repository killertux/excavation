//! Pickup entities (pure, no rendering). M4 has a single pickup kind (Gold); a
//! future milestone may add gems, power-ups, etc.

use macroquad::prelude::Vec2;

/// What a pickup grants or represents. Only `Gold` exists in M4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickupKind {
    Gold,
}

/// A collectible dropped at a world-pixel position.
#[derive(Debug, Clone, Copy)]
pub struct Pickup {
    /// Center of the pickup, in world pixels.
    pub pos: Vec2,
    pub kind: PickupKind,
}

impl Pickup {
    /// A gold pickup at the given world position.
    pub fn gold(pos: Vec2) -> Pickup {
        Pickup {
            pos,
            kind: PickupKind::Gold,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gold_pickup_has_gold_kind() {
        let p = Pickup::gold(Vec2::new(10.0, 20.0));
        assert_eq!(p.pos, Vec2::new(10.0, 20.0));
        assert_eq!(p.kind, PickupKind::Gold);
    }
}
