//! User settings: music/SFX volume and fullscreen.
//!
//! Settings are plain data (serde-serializable) so they can be persisted inside
//! the save file. Volumes are clamped to `0.0..=1.0`. The values are applied
//! now (fullscreen at startup/toggle) but volume only *takes effect* on audio in
//! M6 (no audio yet) — this milestone just stores and persists them.

use serde::{Deserialize, Serialize};

/// The persisted user settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Music volume, clamped to `0.0..=1.0`.
    pub music_volume: f32,
    /// SFX volume, clamped to `0.0..=1.0`.
    pub sfx_volume: f32,
    /// Whether the window/canvas is fullscreen.
    pub fullscreen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            music_volume: 1.0,
            sfx_volume: 1.0,
            fullscreen: false,
        }
    }
}

impl Settings {
    /// Clamp the volume fields into `0.0..=1.0` (defensive; they are set via the
    /// settings UI which already clamps, but a hand-edited save must not panic).
    pub fn clamp(&mut self) {
        self.music_volume = clamp_volume(self.music_volume);
        self.sfx_volume = clamp_volume(self.sfx_volume);
    }
}

/// Clamp a volume to `0.0..=1.0`, snapping NaN to 0.0.
fn clamp_volume(v: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) }
}

/// Raise a volume by `delta` (clamped to the unit range). Callers pass a small
/// step (e.g. 0.1).
pub fn volume_step(v: f32, delta: f32) -> f32 {
    clamp_volume(v + delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_full_and_not_fullscreen() {
        let s = Settings::default();
        assert_eq!(s.music_volume, 1.0);
        assert_eq!(s.sfx_volume, 1.0);
        assert!(!s.fullscreen);
    }

    #[test]
    fn clamp_keeps_in_range() {
        let mut s = Settings {
            music_volume: 1.5,
            sfx_volume: -0.2,
            fullscreen: true,
        };
        s.clamp();
        assert_eq!(s.music_volume, 1.0);
        assert_eq!(s.sfx_volume, 0.0);
    }

    #[test]
    fn clamp_maps_nan_to_zero() {
        let mut s = Settings {
            music_volume: f32::NAN,
            sfx_volume: 0.5,
            fullscreen: false,
        };
        s.clamp();
        assert_eq!(s.music_volume, 0.0);
        assert_eq!(s.sfx_volume, 0.5);
    }

    #[test]
    fn volume_step_clamps_to_unit_range() {
        assert_eq!(volume_step(0.95, 0.1), 1.0);
        assert_eq!(volume_step(0.05, -0.1), 0.0);
        assert_eq!(volume_step(0.5, 0.1), 0.6);
        assert_eq!(volume_step(0.5, -0.1), 0.4);
    }
}
