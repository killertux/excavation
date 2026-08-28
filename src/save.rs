//! Save/load: serialize the persistent run + settings as JSON and store it via a
//! platform backend.
//!
//! - **Desktop** — a plain JSON file (`excavation_save.json`) in the working
//!   directory via `std::fs`.
//! - **Web** — `localStorage` through a small custom JS bridge
//!   (`web/excavation_storage.js`, staged by `build-web.sh`).
//!
//! The save holds run-level state only (see [`RunSnapshot`]); the in-level sim
//! (map, positions, pickups, elapsed, active effect) is never persisted, so a
//! loaded save rebuilds the level at `level_index` fresh ([`Run::resume`]).
//! Corrupt / absent / version-mismatched saves are treated as "no save".

use serde::{Deserialize, Serialize};

use crate::game::run::RunSnapshot;
use crate::settings::Settings;

/// The current save format version. Saves with any other version are rejected.
pub const SAVE_VERSION: u32 = 1;

/// The full persisted document: version + run + settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveData {
    pub version: u32,
    pub run: RunSnapshot,
    pub settings: Settings,
}

/// Platform-specific persistence backend.
#[cfg(not(target_arch = "wasm32"))]
mod storage {
    /// Desktop save file (in the working directory).
    pub const SAVE_PATH: &str = "excavation_save.json";

    pub fn get() -> Option<String> {
        std::fs::read_to_string(SAVE_PATH)
            .ok()
            .and_then(|s| if s.trim().is_empty() { None } else { Some(s) })
    }

    pub fn set(value: &str) {
        let _ = std::fs::write(SAVE_PATH, value);
    }

    pub fn remove() {
        let _ = std::fs::remove_file(SAVE_PATH);
    }
}

/// Platform-specific persistence backend (web: localStorage via a self-contained
/// JS bridge — `web/excavation_storage.js`).
///
/// The vendored macroquad JS bundle does not include a storage plugin, so we use
/// our own `extern "C"` bridge: Rust passes raw UTF-8 byte pointers, the JS side
/// reads `localStorage` through the global `wasm_memory`. `none` results when the
/// key is absent (the JS returns -1).
#[cfg(target_arch = "wasm32")]
mod storage {
    const SAVE_KEY: &str = "excavation_save_v1";

    unsafe extern "C" {
        fn exc_save_set(key_ptr: *const u8, key_len: usize, val_ptr: *const u8, val_len: usize);
        fn exc_save_get_len(key_ptr: *const u8, key_len: usize) -> i32;
        fn exc_save_get_into(key_ptr: *const u8, key_len: usize, out_ptr: *mut u8, out_cap: usize) -> i32;
        fn exc_save_remove(key_ptr: *const u8, key_len: usize);
    }

    pub fn get() -> Option<String> {
        let (kp, kl) = (SAVE_KEY.as_ptr(), SAVE_KEY.len());
        let n = unsafe { exc_save_get_len(kp, kl) };
        if n < 0 {
            return None;
        }
        let mut buf = vec![0u8; n as usize];
        let written = unsafe { exc_save_get_into(kp, kl, buf.as_mut_ptr(), buf.len()) };
        if written < 0 {
            return None;
        }
        buf.truncate(written as usize);
        Some(String::from_utf8_lossy(&buf).into_owned())
    }

    pub fn set(value: &str) {
        let (kp, kl) = (SAVE_KEY.as_ptr(), SAVE_KEY.len());
        unsafe { exc_save_set(kp, kl, value.as_ptr(), value.len()) };
    }

    pub fn remove() {
        let (kp, kl) = (SAVE_KEY.as_ptr(), SAVE_KEY.len());
        unsafe { exc_save_remove(kp, kl) };
    }
}

/// Serialize a save to a JSON string.
pub fn to_json(save: &SaveData) -> String {
    // These types always serialize; a failure would be a programming bug.
    serde_json::to_string(save).expect("save should serialize to JSON")
}

/// Parse a save from a JSON string, rejecting corrupt input and unknown versions
/// (both become `None`, i.e. "no save"). Never panics.
pub fn from_json(s: &str) -> Option<SaveData> {
    let save: SaveData = serde_json::from_str(s).ok()?;
    if save.version != SAVE_VERSION {
        return None;
    }
    Some(save)
}

/// Persist the save via the platform backend.
pub fn save(save: &SaveData) {
    let json = to_json(save);
    storage::set(&json);
}

/// Load the save, returning `None` for absent/corrupt/version-mismatched data.
pub fn load() -> Option<SaveData> {
    let json = storage::get()?;
    from_json(&json)
}

/// Clear the save (used by "Play" to start a fresh run).
pub fn clear() {
    storage::remove();
}

/// Read a save from a specific path (desktop tests + clarity). `None` for
/// absent/corrupt input. Test-only (files).
#[cfg(all(test, not(target_arch = "wasm32")))]
pub fn load_file(path: &str) -> Option<SaveData> {
    let s = std::fs::read_to_string(path).ok()?;
    from_json(&s)
}

/// Write a save to a specific path (desktop tests + clarity). Test-only.
#[cfg(all(test, not(target_arch = "wasm32")))]
pub fn save_file(path: &str, save: &SaveData) {
    let _ = std::fs::write(path, to_json(save));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::game::GameConfig;
    use crate::config::map::MapConfig;
    use crate::game::consumables::ConsumableKind;
    use crate::game::run::Run;
    use crate::input::Input;

    const GAME_TOML: &str = r#"
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
        files = ["a.toml", "b.toml"]
    "#;

    fn game() -> GameConfig {
        GameConfig::from_toml(GAME_TOML).expect("valid game config")
    }

    fn map_cfg(beast_count: u32, seed: u64) -> MapConfig {
        MapConfig::from_toml(&format!(
            r#"
                width = 30
                height = 20
                unmineable_count = 20
                beast_count = {beast_count}
                start = {{ x = 15, y = 19 }}
                exit  = {{ x = 5,  y = 0 }}
                seed = {seed}
            "#
        ))
        .expect("valid map")
    }

    fn no_input() -> Input {
        Input { move_: Default::default(), use_super_pick: false, use_sticky_smell: false }
    }

    fn sample_run() -> Run {
        let cfgs = vec![map_cfg(1, 1), map_cfg(0, 1)];
        let mut r = Run::new(game(), cfgs).expect("run builds");
        r.gold = 900;
        r.upgrades.walk_speed = 4;
        r.consumables.add(ConsumableKind::StickySmell);
        r.lives = 5;
        r.score_total = 1234;
        r.unlocked = 2;
        r
    }

    fn center_of(x: usize, y: usize) -> macroquad::prelude::Vec2 {
        let t = crate::game::TILE_SIZE;
        macroquad::prelude::Vec2::new(x as f32 * t + t / 2.0, y as f32 * t + t / 2.0)
    }

    #[test]
    fn save_data_round_trips_through_json() {
        let r = sample_run();
        let save = SaveData { version: SAVE_VERSION, run: r.snapshot(), settings: Settings::default() };

        let json = to_json(&save);
        let back = from_json(&json).expect("valid save parses");
        assert_eq!(back, save, "save survives a JSON round-trip intact");
    }

    #[test]
    fn full_round_trip_resumes_the_run() {
        let r = sample_run();
        let save = SaveData { version: SAVE_VERSION, run: r.snapshot(), settings: Settings::default() };
        let json = to_json(&save);
        let loaded = from_json(&json).expect("parses");
        let restored = Run::resume(game(), vec![map_cfg(1, 1), map_cfg(0, 1)], loaded.run).expect("resumes");
        assert_eq!(restored.snapshot(), r.snapshot(), "a loaded run reproduces the original state");
    }

    #[test]
    fn rejects_unknown_version_as_no_save() {
        let r = sample_run();
        let save = SaveData { version: SAVE_VERSION + 1, run: r.snapshot(), settings: Settings::default() };
        let json = to_json(&save);
        assert!(from_json(&json).is_none(), "a future-version save is treated as absent");
    }

    #[test]
    fn rejects_corrupt_json_as_no_save() {
        assert!(from_json("this is not json").is_none());
        assert!(from_json("").is_none());
        assert!(from_json("{}").is_none(), "missing fields are rejected");
    }

    #[test]
    fn desktop_file_round_trips() {
        let path = "excavation_test_save.json";
        let r = sample_run();
        let save = SaveData { version: SAVE_VERSION, run: r.snapshot(), settings: Settings::default() };
        save_file(path, &save);
        let loaded = load_file(path).expect("file save loads");
        assert_eq!(loaded, save);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn desktop_storage_backend_round_trips_save_load_clear() {
        // The real persistence path the game uses (file backed on desktop).
        crate::save::clear();
        assert!(crate::save::load().is_none(), "cleared -> no save");
        let r = sample_run();
        let data = SaveData {
            version: SAVE_VERSION,
            run: r.snapshot(),
            settings: Settings { music_volume: 0.5, sfx_volume: 0.8, fullscreen: true },
        };
        crate::save::save(&data);
        assert_eq!(crate::save::load(), Some(data), "saved then loaded back unchanged");
        crate::save::clear();
        assert!(crate::save::load().is_none(), "cleared -> no save again");
    }

    #[test]
    fn a_run_with_progress_snapshots_and_resumes_identically() {
        // Exercise the actual progression: complete level 1, then snapshot.
        let cfgs = vec![map_cfg(1, 1), map_cfg(0, 1)];
        let mut r = Run::new(game(), cfgs).expect("builds");
        r.level.beasts.clear();
        r.level.gold_collected = 8;
        let (ex, ey) = r.level.map.exit_pos();
        r.level.player.pos = center_of(ex, ey);
        let ev = r.update(no_input(), 1.0 / 60.0);
        assert!(matches!(ev, crate::game::run::RunEvent::LevelCompleted { .. }));

        let snap = r.snapshot();
        assert_eq!(snap.gold, 8);
        assert_eq!(snap.unlocked, 2, "completing level 1 unlocks level 2");

        let save = SaveData { version: SAVE_VERSION, run: snap, settings: Settings::default() };
        let loaded = from_json(&to_json(&save)).expect("parses");
        let restored = Run::resume(game(), vec![map_cfg(1, 1), map_cfg(0, 1)], loaded.run).expect("resumes");
        assert_eq!(restored.snapshot(), snap, "restored run equals the snapshotted one");
    }
}
