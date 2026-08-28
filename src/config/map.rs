//! Per-level configuration from `assets/maps/levelXX.toml`.

use serde::Deserialize;

use super::ConfigError;

/// A grid coordinate `(x, y)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

/// Root of a map TOML document.
///
/// Fields marked with `#[serde(default)]` are optional and are expected from
/// later milestones (`gold_*`, `beast_*`); M2 parses but does not yet use them.
///
/// The `gold_*`/`beast_*` and `structures` fields are read by M3/M4, so dead
/// code is allowed on the ones not consumed yet.
///
/// `start`/`exit` are the two **gaps in the border wall** (rendered as `Dirt`);
/// the doors themselves were removed.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MapConfig {
    /// Grid width, in cells.
    pub width: usize,
    /// Grid height, in cells.
    pub height: usize,
    /// Fixed number of unmineable rocks to scatter.
    pub unmineable_count: usize,
    /// How many mineable rocks hide gold (used in M4).
    #[serde(default)]
    pub gold_count: u32,
    /// How many beasts spawn (used in M3).
    #[serde(default)]
    pub beast_count: u32,
    /// Beast speed multiplier (used in M3).
    #[serde(default = "one")]
    pub beast_speed_multiplier: f32,
    /// Beast mining-time multiplier (used in M3).
    #[serde(default = "one")]
    pub beast_mining_time_multiplier: f32,
    /// Start gap, on the border (a hole in the wall).
    pub start: Pos,
    /// Exit gap, on the border (a hole in the wall).
    pub exit: Pos,
    /// Optional seed for reproducible generation; omit to randomize each run.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Optional unbreakable internal structures, as `[x, y]` cells.
    #[serde(default)]
    pub structures: Vec<[i32; 2]>,
}

fn one() -> f32 {
    1.0
}

impl MapConfig {
    /// Parse and validate a map TOML document.
    pub fn from_toml(s: &str) -> Result<MapConfig, ConfigError> {
        let cfg: MapConfig = toml::from_str(s)?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.width == 0 || self.height == 0 {
            return Err(ConfigError::Validation(
                "width and height must be > 0".into(),
            ));
        }
        if !self.on_border(self.start) {
            return Err(ConfigError::Validation(format!(
                "start {:?} is not on the border",
                self.start
            )));
        }
        if !self.on_border(self.exit) {
            return Err(ConfigError::Validation(format!(
                "exit {:?} is not on the border",
                self.exit
            )));
        }
        if self.start == self.exit {
            return Err(ConfigError::Validation(
                "start and exit must be different cells".into(),
            ));
        }
        // The maximum number of rocks that can be unmineable is the interior cell
        // count (the border ring is off-limits). Exceeding it makes a valid map
        // impossible, so reject it here rather than failing at generation time.
        let interior = self.width.saturating_sub(2) * self.height.saturating_sub(2);
        if self.unmineable_count > interior {
            return Err(ConfigError::Validation(format!(
                "unmineable_count {} exceeds the interior cell count {interior}",
                self.unmineable_count
            )));
        }
        // Gold hides in mineable cells that survive the unmineable flip, so the
        // two counts together cannot exceed the interior.
        if self.gold_count as usize + self.unmineable_count > interior {
            return Err(ConfigError::Validation(format!(
                "gold_count {} + unmineable_count {} exceeds the interior cell count {interior}",
                self.gold_count, self.unmineable_count
            )));
        }
        Ok(())
    }

    fn on_border(&self, pos: Pos) -> bool {
        let w = self.width as i32;
        let h = self.height as i32;
        pos.x == 0 || pos.y == 0 || pos.x == w - 1 || pos.y == h - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_valid_map() {
        let s = r#"
            width = 30
            height = 20
            unmineable_count = 20
            start = { x = 15, y = 19 }
            exit  = { x = 5,  y = 0 }
            seed = 12345
            structures = [[8, 5], [9, 5]]
        "#;
        let cfg = MapConfig::from_toml(s).expect("valid map");
        assert_eq!(cfg.width, 30);
        assert_eq!(cfg.height, 20);
        assert_eq!(cfg.unmineable_count, 20);
        assert_eq!(cfg.start, Pos { x: 15, y: 19 });
        assert_eq!(cfg.exit, Pos { x: 5, y: 0 });
        assert_eq!(cfg.seed, Some(12345));
        assert_eq!(cfg.structures, vec![[8, 5], [9, 5]]);
        // Optional fields default.
        assert_eq!(cfg.gold_count, 0);
        assert_eq!(cfg.beast_count, 0);
        assert_eq!(cfg.beast_speed_multiplier, 1.0);
    }

    #[test]
    fn defaults_when_seed_and_optional_fields_are_missing() {
        let s = r#"
            width = 30
            height = 20
            unmineable_count = 20
            start = { x = 15, y = 19 }
            exit  = { x = 5,  y = 0 }
        "#;
        let cfg = MapConfig::from_toml(s).expect("valid map");
        assert_eq!(cfg.seed, None);
        assert!(cfg.structures.is_empty());
    }

    #[test]
    fn rejects_off_border_gap() {
        let s = r#"
            width = 30
            height = 20
            unmineable_count = 20
            start = { x = 15, y = 10 }
            exit  = { x = 5,  y = 0 }
        "#;
        assert!(matches!(
            MapConfig::from_toml(s),
            Err(ConfigError::Validation(_))
        ));
    }

    #[test]
    fn rejects_unmineable_count_that_exceeds_interior() {
        let s = r#"
            width = 4
            height = 4
            unmineable_count = 5
            start = { x = 0, y = 2 }
            exit  = { x = 3, y = 2 }
        "#;
        // Interior of a 4x4 map is 2x2 = 4 cells; 5 is impossible.
        assert!(matches!(
            MapConfig::from_toml(s),
            Err(ConfigError::Validation(_))
        ));
    }

    #[test]
    fn rejects_zero_dimensions() {
        let s = r#"
            width = 0
            height = 20
            unmineable_count = 0
            start = { x = 0, y = 3 }
            exit  = { x = 1, y = 0 }
        "#;
        assert!(matches!(
            MapConfig::from_toml(s),
            Err(ConfigError::Validation(_))
        ));
    }

    #[test]
    fn rejects_identical_start_and_exit_gap() {
        let s = r#"
            width = 30
            height = 20
            unmineable_count = 0
            start = { x = 15, y = 19 }
            exit  = { x = 15, y = 19 }
        "#;
        assert!(matches!(
            MapConfig::from_toml(s),
            Err(ConfigError::Validation(_))
        ));
    }

    #[test]
    fn accepts_zero_unmineable_rocks() {
        let s = r#"
            width = 30
            height = 20
            unmineable_count = 0
            start = { x = 15, y = 19 }
            exit  = { x = 5,  y = 0 }
        "#;
        assert!(MapConfig::from_toml(s).is_ok());
    }

    #[test]
    fn rejects_gold_count_that_exceeds_remaining_mineable_cells() {
        // Interior of a 6x6 map is 4x4 = 16 cells. 16 gold + 1 unmineable
        // exceeds it, so the combination is rejected.
        let s = r#"
            width = 6
            height = 6
            unmineable_count = 1
            gold_count = 16
            start = { x = 0, y = 2 }
            exit  = { x = 5, y = 2 }
        "#;
        assert!(matches!(
            MapConfig::from_toml(s),
            Err(ConfigError::Validation(_))
        ));
    }
}
