//! Global game tuning from `assets/game.toml`.

use serde::Deserialize;

use super::ConfigError;

/// Root of `assets/game.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct GameConfig {
    pub player: PlayerConfig,
}

/// Player-related tuning.
#[derive(Debug, Clone, Deserialize)]
pub struct PlayerConfig {
    /// Walk speed, world px/s.
    pub base_speed: f32,
    /// Seconds to mine one rock (before upgrades).
    pub base_mining_time: f32,
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_valid_document() {
        let s = r#"
            [player]
            base_speed = 120.0
            base_mining_time = 0.8
        "#;
        let cfg = GameConfig::from_toml(s).expect("valid config");
        assert_eq!(cfg.player.base_speed, 120.0);
        assert_eq!(cfg.player.base_mining_time, 0.8);
    }

    #[test]
    fn rejects_non_positive_mining_time() {
        let s = "[player]\nbase_speed = 120.0\nbase_mining_time = 0.0\n";
        assert!(matches!(
            GameConfig::from_toml(s),
            Err(ConfigError::Validation(_))
        ));
    }

    #[test]
    fn rejects_non_positive_speed() {
        let s = "[player]\nbase_speed = -1.0\nbase_mining_time = 0.8\n";
        assert!(matches!(
            GameConfig::from_toml(s),
            Err(ConfigError::Validation(_))
        ));
    }

    #[test]
    fn rejects_missing_fields() {
        let s = "[player]\nbase_speed = 120.0\n";
        assert!(matches!(GameConfig::from_toml(s), Err(ConfigError::Toml(_))));
    }
}
