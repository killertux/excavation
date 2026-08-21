//! TOML-backed configuration: per-game tuning and per-level map definitions.
//!
//! Both [`game::GameConfig`] and [`map::MapConfig`] are parsed from TOML then
//! validated, so an invalid file fails loudly at load time rather than producing
//! a subtly broken game. Parsing and validation are pure (no IO) so they are
//! unit-testable; the callers (e.g. `App`) fetch the bytes via macroquad's
//! `load_file` and hand the text here.

pub mod game;
pub mod map;

use std::fmt;

/// Error produced while parsing or validating a TOML config file.
#[derive(Debug)]
pub enum ConfigError {
    /// TOML didn't deserialize into the expected struct.
    Toml(toml::de::Error),
    /// The TOML was well-formed but semantically invalid.
    Validation(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Toml(e) => write!(f, "TOML parse error: {e}"),
            ConfigError::Validation(msg) => write!(f, "config validation error: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        ConfigError::Toml(e)
    }
}
