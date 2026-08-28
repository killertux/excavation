//! Global game tuning from `assets/game.toml`.

use serde::Deserialize;

use super::ConfigError;

/// Root of `assets/game.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct GameConfig {
    pub player: PlayerConfig,
    pub beast: BeastConfig,
}

/// Player-related tuning.
#[derive(Debug, Clone, Deserialize)]
pub struct PlayerConfig {
    /// Walk speed, world px/s.
    pub base_speed: f32,
    /// Seconds to mine one rock (before upgrades).
    pub base_mining_time: f32,
    /// Lives at the start of a run (before M4's max-lives/shop).
    pub starting_lives: u32,
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

impl GameConfig {
    /// Parse and validate a `game.toml` document.
    pub fn from_toml(s: &str) -> Result<GameConfig, ConfigError> {
        let cfg: GameConfig = toml::from_str(s)?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), ConfigError> {
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
        Ok(())
    }
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

        [beast]
        base_speed = 140.0
        base_mining_time = 1.6
        replan_interval = 0.25
    "#;

    #[test]
    fn parses_a_valid_document() {
        let cfg = GameConfig::from_toml(VALID).expect("valid config");
        assert_eq!(cfg.player.base_speed, 240.0);
        assert_eq!(cfg.player.base_mining_time, 0.8);
        assert_eq!(cfg.player.starting_lives, 3);
        assert_eq!(cfg.beast.base_speed, 140.0);
        assert_eq!(cfg.beast.base_mining_time, 1.6);
        assert_eq!(cfg.beast.replan_interval, 0.25);
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
    fn rejects_missing_fields() {
        let s = "[player]\nbase_speed = 120.0\nbase_mining_time = 0.8\n";
        assert!(matches!(GameConfig::from_toml(s), Err(ConfigError::Toml(_))));
    }
}
