//! Upgrade levels and their effects (pure). Walk Speed is additive per level;
//! Mining Speed is multiplicative per level (`0.85^level`), matching the §9.1
//! example in the M4 plan.

use crate::config::game::{MiningSpeedConfig, WalkSpeedConfig};

/// The player's owned upgrade levels. Defaults to 0 (no upgrades bought).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Upgrades {
    pub walk_speed: u32,
    pub mining_speed: u32,
}

/// Effective walk speed given the base and the present upgrade level.
pub fn walk_speed(base: f32, upgrades: &Upgrades, cfg: &WalkSpeedConfig) -> f32 {
    base + upgrades.walk_speed as f32 * cfg.speed_increase_per_level
}

/// Effective per-rock mining time given the base and the present upgrade level.
pub fn mining_time(base: f32, upgrades: &Upgrades, cfg: &MiningSpeedConfig) -> f32 {
    base * cfg.mining_time_multiplier_per_level.powf(upgrades.mining_speed as f32)
}

/// The gold cost to go from `level` to `level + 1`. `None` when `level` is at
/// `max_level` (i.e. the upgrade is maxed).
pub fn cost(level: u32, max_level: u32, cost_per_level: &[u32]) -> Option<u32> {
    if level >= max_level {
        return None;
    }
    cost_per_level.get(level as usize).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::game::{MiningSpeedConfig, WalkSpeedConfig};

    fn walk_cfg() -> WalkSpeedConfig {
        WalkSpeedConfig { max_level: 5, cost_per_level: vec![50, 100, 200, 400, 800], speed_increase_per_level: 15.0 }
    }

    fn mine_cfg() -> MiningSpeedConfig {
        MiningSpeedConfig { max_level: 5, cost_per_level: vec![50, 100, 200, 400, 800], mining_time_multiplier_per_level: 0.85 }
    }

    #[test]
    fn walk_speed_is_additive_per_level() {
        let u = Upgrades { walk_speed: 2, mining_speed: 0 };
        assert!((walk_speed(240.0, &u, &walk_cfg()) - 270.0).abs() < 1e-5);
    }

    #[test]
    fn mining_time_is_multiplicative_per_level() {
        let u = Upgrades { walk_speed: 0, mining_speed: 3 };
        // 0.8 * 0.85^3
        let expected = 0.8 * 0.85f32.powi(3);
        assert!((mining_time(0.8, &u, &mine_cfg()) - expected).abs() < 1e-5);
    }

    #[test]
    fn cost_progression_and_none_at_max() {
        let (max, costs) = (5, vec![50, 100, 200, 400, 800]);
        assert_eq!(cost(0, max, &costs), Some(50));
        assert_eq!(cost(1, max, &costs), Some(100));
        assert_eq!(cost(4, max, &costs), Some(800));
        assert_eq!(cost(5, max, &costs), None, "at max -> no more cost");
        assert_eq!(cost(6, max, &costs), None, "past max -> none");
    }
}
