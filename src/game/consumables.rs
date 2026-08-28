//! Consumable items (pure): owned counts persist across levels; the **active**
//! effect is per-level (owned by `Level`) and resets when a new level starts.

use crate::config::game::ConsumablesConfig;

/// The two consumable item kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConsumableKind {
    /// Instantly mine any rock except unbreakable, for the effect's duration.
    SuperPick,
    /// Make beasts wander randomly (pathfinding disabled), for the duration.
    StickySmell,
}

/// An in-progress consumable effect, ticking down each frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveEffect {
    pub kind: ConsumableKind,
    /// Seconds of effect remaining.
    pub remaining: f32,
}

/// Owned consumable counts, persisted across levels in `Run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Consumables {
    pub super_pick: u32,
    pub sticky_smell: u32,
}

impl Consumables {
    /// The owned count for `kind`.
    pub fn count(&self, kind: ConsumableKind) -> u32 {
        match kind {
            ConsumableKind::SuperPick => self.super_pick,
            ConsumableKind::StickySmell => self.sticky_smell,
        }
    }

    /// Add one of `kind` (e.g. from a shop purchase).
    pub fn add(&mut self, kind: ConsumableKind) {
        match kind {
            ConsumableKind::SuperPick => self.super_pick += 1,
            ConsumableKind::StickySmell => self.sticky_smell += 1,
        }
    }

    /// Consume one of `kind` if any are owned; returns whether one was used.
    pub fn use_one(&mut self, kind: ConsumableKind) -> bool {
        let slot = match kind {
            ConsumableKind::SuperPick => &mut self.super_pick,
            ConsumableKind::StickySmell => &mut self.sticky_smell,
        };
        if *slot > 0 {
            *slot -= 1;
            true
        } else {
            false
        }
    }
}

/// The active duration for a consumable kind, from `game.toml`.
pub fn duration(kind: ConsumableKind, cfg: &ConsumablesConfig) -> f32 {
    match kind {
        ConsumableKind::SuperPick => cfg.super_pick.duration,
        ConsumableKind::StickySmell => cfg.sticky_smell.duration,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::game::ConsumablesConfig;

    fn cfg() -> ConsumablesConfig {
        ConsumablesConfig {
            super_pick: crate::config::game::ConsumableConfig { cost: 60, duration: 3.0 },
            sticky_smell: crate::config::game::ConsumableConfig { cost: 40, duration: 5.0 },
        }
    }

    #[test]
    fn use_one_decrements_and_reports() {
        let mut c = Consumables { super_pick: 2, sticky_smell: 0 };
        assert!(c.use_one(ConsumableKind::SuperPick));
        assert_eq!(c.count(ConsumableKind::SuperPick), 1);
        assert!(!c.use_one(ConsumableKind::StickySmell), "none owned -> cannot use");
        assert_eq!(c.count(ConsumableKind::StickySmell), 0);
    }

    #[test]
    fn use_one_at_zero_returns_false_and_stays_zero() {
        let mut c = Consumables::default();
        assert!(!c.use_one(ConsumableKind::SuperPick));
        assert_eq!(c.count(ConsumableKind::SuperPick), 0);
    }

    #[test]
    fn add_increments() {
        let mut c = Consumables::default();
        c.add(ConsumableKind::SuperPick);
        c.add(ConsumableKind::SuperPick);
        c.add(ConsumableKind::StickySmell);
        assert_eq!(c.super_pick, 2);
        assert_eq!(c.sticky_smell, 1);
    }

    #[test]
    fn duration_looks_up_kind() {
        assert_eq!(duration(ConsumableKind::SuperPick, &cfg()), 3.0);
        assert_eq!(duration(ConsumableKind::StickySmell, &cfg()), 5.0);
    }
}
