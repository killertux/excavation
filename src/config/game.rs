//! Global game tuning from `assets/game.toml`.

use serde::Deserialize;

use super::ConfigError;

/// Root of `assets/game.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct GameConfig {
    pub player: PlayerConfig,
    pub beast: BeastConfig,
    pub upgrades: UpgradesConfig,
    pub lives: LivesConfig,
    pub consumables: ConsumablesConfig,
    pub score: ScoreConfig,
    pub map_order: MapOrderConfig,
}

/// Player-related tuning.
#[derive(Debug, Clone, Deserialize)]
pub struct PlayerConfig {
    /// Walk speed, world px/s.
    pub base_speed: f32,
    /// Seconds to mine one rock (before upgrades).
    pub base_mining_time: f32,
    /// Lives at the start of a run.
    pub starting_lives: u32,
    /// Lifetime cap on lives (the shop's lives purchase cannot exceed this).
    pub max_lives: u32,
}

/// Beast-related tuning (the `[beast]` section).
#[derive(Debug, Clone, Deserialize)]
pub struct BeastConfig {
    /// Beast walk speed, world px/s (slower than the player).
    pub base_speed: f32,
    /// Seconds to dig one rock (slower than the player).
    pub base_mining_time: f32,
    /// Seconds between beast AI re-plans.
    pub replan_interval: f32,
}

/// The `[upgrades]` section: one entry per purchasable upgrade.
#[derive(Debug, Clone, Deserialize)]
pub struct UpgradesConfig {
    pub walk_speed: WalkSpeedConfig,
    pub mining_speed: MiningSpeedConfig,
}

/// The `[upgrades.walk_speed]` upgrade: additive walk-speed gain per level.
#[derive(Debug, Clone, Deserialize)]
pub struct WalkSpeedConfig {
    pub max_level: u32,
    pub cost_per_level: Vec<u32>,
    pub speed_increase_per_level: f32,
}

/// The `[upgrades.mining_speed]` upgrade: multiplicative mining-time reduction
/// per level (e.g. `0.85^level`).
#[derive(Debug, Clone, Deserialize)]
pub struct MiningSpeedConfig {
    pub max_level: u32,
    pub cost_per_level: Vec<u32>,
    pub mining_time_multiplier_per_level: f32,
}

/// The `[lives]` section: the one-time cost to buy an extra life.
#[derive(Debug, Clone, Deserialize)]
pub struct LivesConfig {
    pub cost: u32,
}

/// A single consumable's cost + active duration (the `[consumables.*]` sections).
#[derive(Debug, Clone, Deserialize)]
pub struct ConsumableConfig {
    pub cost: u32,
    pub duration: f32,
}

/// The `[consumables]` section.
#[derive(Debug, Clone, Deserialize)]
pub struct ConsumablesConfig {
    pub super_pick: ConsumableConfig,
    pub sticky_smell: ConsumableConfig,
}

/// The `[score]` section: how a level score is derived from time and gold.
#[derive(Debug, Clone, Deserialize)]
pub struct ScoreConfig {
    pub par_time: f32,
    pub time_multiplier: f32,
    pub gold_multiplier: f32,
}

/// The `[map_order]` section: the level files played in sequence.
#[derive(Debug, Clone, Deserialize)]
pub struct MapOrderConfig {
    pub files: Vec<String>,
}

impl GameConfig {
    /// Parse and validate a `game.toml` document.
    pub fn from_toml(s: &str) -> Result<GameConfig, ConfigError> {
        let cfg: GameConfig = toml::from_str(s)?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        // Player.
        if !self.player.base_speed.is_finite() || self.player.base_speed <= 0.0 {
            return Err(ConfigError::Validation(
                "player.base_speed must be a positive number".into(),
            ));
        }
        if !self.player.base_mining_time.is_finite() || self.player.base_mining_time <= 0.0 {
            return Err(ConfigError::Validation(
                "player.base_mining_time must be a positive number".into(),
            ));
        }
        if self.player.starting_lives == 0 {
            return Err(ConfigError::Validation(
                "player.starting_lives must be > 0".into(),
            ));
        }
        if self.player.max_lives == 0 {
            return Err(ConfigError::Validation(
                "player.max_lives must be > 0".into(),
            ));
        }
        if self.player.max_lives < self.player.starting_lives {
            return Err(ConfigError::Validation(
                "player.max_lives must be >= player.starting_lives".into(),
            ));
        }

        // Beast.
        if !self.beast.base_speed.is_finite() || self.beast.base_speed <= 0.0 {
            return Err(ConfigError::Validation(
                "beast.base_speed must be a positive number".into(),
            ));
        }
        if !self.beast.base_mining_time.is_finite() || self.beast.base_mining_time <= 0.0 {
            return Err(ConfigError::Validation(
                "beast.base_mining_time must be a positive number".into(),
            ));
        }
        if !self.beast.replan_interval.is_finite() || self.beast.replan_interval <= 0.0 {
            return Err(ConfigError::Validation(
                "beast.replan_interval must be a positive number".into(),
            ));
        }

        // Upgrades: both entries must have a reachable cost curve.
        validate_upgrade(
            "upgrades.walk_speed",
            &self.upgrades.walk_speed.max_level,
            &self.upgrades.walk_speed.cost_per_level,
            self.upgrades.walk_speed.speed_increase_per_level,
        )?;
        validate_upgrade(
            "upgrades.mining_speed",
            &self.upgrades.mining_speed.max_level,
            &self.upgrades.mining_speed.cost_per_level,
            self.upgrades.mining_speed.mining_time_multiplier_per_level,
        )?;

        // Lives.
        if self.lives.cost == 0 {
            return Err(ConfigError::Validation(
                "lives.cost must be > 0".into(),
            ));
        }

        // Consumables.
        validate_consumable("consumables.super_pick", &self.consumables.super_pick)?;
        validate_consumable("consumables.sticky_smell", &self.consumables.sticky_smell)?;

        // Score.
        if !self.score.par_time.is_finite() || self.score.par_time <= 0.0 {
            return Err(ConfigError::Validation(
                "score.par_time must be a positive number".into(),
            ));
        }
        if !self.score.time_multiplier.is_finite() || self.score.time_multiplier < 0.0 {
            return Err(ConfigError::Validation(
                "score.time_multiplier must be a non-negative number".into(),
            ));
        }
        if !self.score.gold_multiplier.is_finite() || self.score.gold_multiplier < 0.0 {
            return Err(ConfigError::Validation(
                "score.gold_multiplier must be a non-negative number".into(),
            ));
        }

        // Map order must list at least one level.
        if self.map_order.files.is_empty() {
            return Err(ConfigError::Validation(
                "map_order.files must list at least one map".into(),
            ));
        }

        Ok(())
    }
}

/// Validate the shared parts of an upgrade config (max level + cost curve).
fn validate_upgrade(
    name: &str,
    max_level: &u32,
    cost_per_level: &[u32],
    effect: f32,
) -> Result<(), ConfigError> {
    if *max_level == 0 {
        return Err(ConfigError::Validation(format!("{name}.max_level must be > 0")));
    }
    if cost_per_level.is_empty() {
        return Err(ConfigError::Validation(format!(
            "{name}.cost_per_level must not be empty"
        )));
    }
    if (cost_per_level.len() as u32) < *max_level {
        return Err(ConfigError::Validation(format!(
            "{name}.cost_per_level must have at least max_level entries"
        )));
    }
    if cost_per_level.iter().any(|&c| c == 0) {
        return Err(ConfigError::Validation(format!(
            "{name}.cost_per_level entries must be > 0"
        )));
    }
    if !effect.is_finite() || effect <= 0.0 {
        return Err(ConfigError::Validation(format!(
            "{name} effect value must be a positive number"
        )));
    }
    Ok(())
}

/// Validate a single consumable config.
fn validate_consumable(name: &str, c: &ConsumableConfig) -> Result<(), ConfigError> {
    if c.cost == 0 {
        return Err(ConfigError::Validation(format!("{name}.cost must be > 0")));
    }
    if !c.duration.is_finite() || c.duration <= 0.0 {
        return Err(ConfigError::Validation(format!(
            "{name}.duration must be a positive number"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid, fully-populated `game.toml` document.
    const VALID: &str = r#"
        [player]
        base_speed = 240.0
        base_mining_time = 0.8
        starting_lives = 3
        max_lives = 9

        [beast]
        base_speed = 140.0
        base_mining_time = 1.6
        replan_interval = 0.25

        [upgrades.walk_speed]
        max_level = 5
        cost_per_level = [50, 100, 200, 400, 800]
        speed_increase_per_level = 15.0

        [upgrades.mining_speed]
        max_level = 5
        cost_per_level = [50, 100, 200, 400, 800]
        mining_time_multiplier_per_level = 0.85

        [lives]
        cost = 100

        [consumables.super_pick]
        cost = 60
        duration = 3.0

        [consumables.sticky_smell]
        cost = 40
        duration = 5.0

        [score]
        par_time = 60.0
        time_multiplier = 10.0
        gold_multiplier = 5.0

        [map_order]
        files = ["assets/maps/level01.toml", "assets/maps/level02.toml"]
    "#;

    #[test]
    fn parses_a_valid_document() {
        let cfg = GameConfig::from_toml(VALID).expect("valid config");
        assert_eq!(cfg.player.base_speed, 240.0);
        assert_eq!(cfg.player.base_mining_time, 0.8);
        assert_eq!(cfg.player.starting_lives, 3);
        assert_eq!(cfg.player.max_lives, 9);
        assert_eq!(cfg.beast.base_speed, 140.0);
        assert_eq!(cfg.beast.base_mining_time, 1.6);
        assert_eq!(cfg.beast.replan_interval, 0.25);
        assert_eq!(cfg.upgrades.walk_speed.max_level, 5);
        assert_eq!(cfg.upgrades.walk_speed.cost_per_level, vec![50, 100, 200, 400, 800]);
        assert_eq!(cfg.upgrades.walk_speed.speed_increase_per_level, 15.0);
        assert_eq!(cfg.upgrades.mining_speed.mining_time_multiplier_per_level, 0.85);
        assert_eq!(cfg.lives.cost, 100);
        assert_eq!(cfg.consumables.super_pick.cost, 60);
        assert_eq!(cfg.consumables.super_pick.duration, 3.0);
        assert_eq!(cfg.consumables.sticky_smell.cost, 40);
        assert_eq!(cfg.consumables.sticky_smell.duration, 5.0);
        assert_eq!(cfg.score.par_time, 60.0);
        assert_eq!(cfg.score.time_multiplier, 10.0);
        assert_eq!(cfg.score.gold_multiplier, 5.0);
        assert_eq!(cfg.map_order.files.len(), 2);
    }

    #[test]
    fn rejects_non_positive_player_mining_time() {
        let s = VALID.replace("base_mining_time = 0.8", "base_mining_time = 0.0");
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_non_positive_player_speed() {
        let s = VALID.replace("base_speed = 240.0", "base_speed = -1.0");
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_zero_starting_lives() {
        let s = VALID.replace("starting_lives = 3", "starting_lives = 0");
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_zero_max_lives() {
        let s = VALID.replace("max_lives = 9", "max_lives = 0");
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_max_lives_below_starting_lives() {
        let s = VALID.replace("max_lives = 9", "max_lives = 2");
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_empty_cost_per_level() {
        let s = VALID.replace("cost_per_level = [50, 100, 200, 400, 800]", "cost_per_level = []");
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_short_cost_per_level() {
        let s = VALID.replace("cost_per_level = [50, 100, 200, 400, 800]", "cost_per_level = [50, 100]");
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_zero_cost_entry() {
        let s = VALID.replace("cost_per_level = [50, 100, 200, 400, 800]", "cost_per_level = [0, 100, 200, 400, 800]");
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_zero_consumable_duration() {
        let s = VALID.replace("duration = 3.0", "duration = 0.0");
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_zero_lives_cost() {
        let s = VALID.replace("cost = 100", "cost = 0");
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_non_positive_beast_speed() {
        let s = VALID.replace("base_speed = 140.0", "base_speed = 0.0");
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_non_positive_beast_mining_time() {
        let s = VALID.replace("base_mining_time = 1.6", "base_mining_time = -2.0");
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_non_positive_replan_interval() {
        let s = VALID.replace("replan_interval = 0.25", "replan_interval = 0.0");
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_empty_map_order() {
        let s = VALID.replace(
            r#"files = ["assets/maps/level01.toml", "assets/maps/level02.toml"]"#,
            "files = []",
        );
        assert!(matches!(GameConfig::from_toml(&s), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn rejects_missing_fields() {
        let s = "[player]\nbase_speed = 120.0\nbase_mining_time = 0.8\n";
        assert!(matches!(GameConfig::from_toml(s), Err(ConfigError::Toml(_))));
    }
}
