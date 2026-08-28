//! Seeded, deterministic map generation (pure, no rendering).
//!
//! Algorithm (matches `REQUIREMENTS.md` §6, simplified to the 3-terrain model):
//! 1. Fill the border ring with `Unbreakable` and the interior with `Mineable`.
//! 2. Carve the start/exit gaps: those two border cells become `Dirt` (a hole in
//!    the wall).
//! 3. Place unbreakable internal structures.
//! 4. Carve a guaranteed **corridor** (grid A\*) from start to exit over rock,
//!    and mark it `protected` so it can never become unmineable.
//! 5. Shuffle the remaining interior cells with the seeded RNG and flip the first
//!    `unmineable_count` to `Unmineable`.
//!
//! Generation is reproducible: the same `config` and `seed` always produce the
//! same map. A deterministic RNG instance is created locally (not the global
//! quad-rand state) so tests that run in parallel remain reproducible.

use std::collections::HashSet;
use std::fmt;

use macroquad::rand::{ChooseRandom, RandGenerator};

use super::map::{Map, Tile};
use super::pathfinding;
use crate::config::map::MapConfig;

/// Error produced when a map cannot be generated.
#[derive(Debug)]
pub enum GenError {
    /// The guaranteed start->exit corridor could not be carved (invalid config).
    NoPath,
    /// Fewer eligible interior cells than `unmineable_count`.
    NotEnoughCells { needed: usize, available: usize },
}

impl fmt::Display for GenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenError::NoPath => write!(f, "no start->exit path could be carved"),
            GenError::NotEnoughCells { needed, available } => {
                write!(f, "need {needed} unmineable cells but only {available} are available")
            }
        }
    }
}

impl std::error::Error for GenError {}

/// Resolve the seed to use for `config`: its own `seed` when present, otherwise
/// a fresh per-run value derived from the wall clock (works on desktop & wasm).
pub fn resolve_seed(config: &MapConfig) -> u64 {
    config.seed.unwrap_or_else(random_run_seed)
}

/// A fresh, non-reproducible per-run seed. Used when a level **restarts** after
/// the player is caught (the config `seed` still makes the *first* load
/// reproducible, but a caught life regenerates a brand-new random map).
pub fn fresh_random_seed() -> u64 {
    random_run_seed()
}

/// A per-run seed from the current wall clock in nanoseconds, mixed through the
/// (locally seeded) macroquad PRNG so the low bits carry real entropy.
fn random_run_seed() -> u64 {
    let nanos = (macroquad::miniquad::date::now() * 1_000_000_000.0) as u64;
    let rng = RandGenerator::new();
    rng.srand(nanos);
    rng.gen_range(0u64, u64::MAX)
}

/// Cells the player (and the guaranteed corridor) can traverse: diggable rock or
/// an already-visible dirt gap.
fn is_passable(t: Tile) -> bool {
    matches!(t, Tile::Mineable | Tile::Dirt)
}

/// Generate a map from `config` using the deterministic RNG seeded by `seed`.
pub fn generate(config: &MapConfig, seed: u64) -> Result<Map, GenError> {
    let w = config.width;
    let h = config.height;

    let mut tiles = vec![Tile::Mineable; w * h];

    // 1. Border ring of unbreakable rock.
    for y in 0..h {
        for x in 0..w {
            if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                tiles[idx(w, x, y)] = Tile::Unbreakable;
            }
        }
    }

    // 2. Start/exit gaps: carve a hole in the border wall.
    let start = (config.start.x, config.start.y);
    let exit = (config.exit.x, config.exit.y);
    if start.0 >= 0 && start.1 >= 0 && (start.0 as usize) < w && (start.1 as usize) < h {
        tiles[idx(w, start.0 as usize, start.1 as usize)] = Tile::Dirt;
    }
    if exit.0 >= 0 && exit.1 >= 0 && (exit.0 as usize) < w && (exit.1 as usize) < h {
        tiles[idx(w, exit.0 as usize, exit.1 as usize)] = Tile::Dirt;
    }

    // 3. Internal unbreakable structures (never covering a start/exit gap).
    for [vx, vy] in &config.structures {
        let (x, y) = (*vx, *vy);
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            let i = idx(w, x as usize, y as usize);
            let t = tiles[i];
            if t != Tile::Dirt {
                tiles[i] = Tile::Unbreakable;
            }
        }
    }

    // 4. Guaranteed corridor (protected from becoming unmineable). A\* routes
    //    over diggable rock and the two dirt gaps, avoiding unbreakable cells.
    let cell_passable = |x: i32, y: i32| -> bool {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            return false;
        }
        is_passable(tiles[idx(w, x as usize, y as usize)])
    };
    let corridor = pathfinding::astar(start, exit, cell_passable).ok_or(GenError::NoPath)?;

    let mut protected = vec![false; w * h];
    for &(x, y) in &corridor {
        protected[idx(w, x as usize, y as usize)] = true;
    }

    // 5. Eligible interior cells: interior Mineable cells not on the corridor.
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = idx(w, x, y);
            if tiles[i] == Tile::Mineable && !protected[i] {
                candidates.push((x, y));
            }
        }
    }

    if candidates.len() < config.unmineable_count {
        return Err(GenError::NotEnoughCells {
            needed: config.unmineable_count,
            available: candidates.len(),
        });
    }

    // 5b. Seeded Fisher-Yates shuffle, then flip the first N to unmineable.
    let rng = RandGenerator::new();
    rng.srand(seed);
    candidates.shuffle_with_state(&rng);
    for &(x, y) in candidates.iter().take(config.unmineable_count) {
        tiles[idx(w, x, y)] = Tile::Unmineable;
    }

    // 6. Place gold into a random subset of the still-mineable interior cells.
    //    The number of mineable cells after the flip is (at least) the interior
    //    count minus `unmineable_count`, so `gold_count` cannot exceed it when
    //    the config is validated. Using the same RNG keeps gold reproducible with
    //    the seed. A `gold_count` larger than the available mineable cells is
    //    clamped so generation never errors (an over-large count is rejected at
    //    config-parse time anyway).
    let mut gold_cells: Vec<(usize, usize)> = Vec::new();
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = idx(w, x, y);
            if tiles[i] == Tile::Mineable {
                gold_cells.push((x, y));
            }
        }
    }
    gold_cells.shuffle_with_state(&rng);
    let gold: HashSet<(usize, usize)> = gold_cells
        .into_iter()
        .take(config.gold_count as usize)
        .collect();

    let map = Map {
        width: w,
        height: h,
        tiles,
        start: (start.0 as usize, start.1 as usize),
        exit: (exit.0 as usize, exit.1 as usize),
        gold,
    };

    // Guaranteed post-condition: a diggable path from start to exit always exists
    // (this is what makes every generated level solvable). By construction the
    // corridor is protected, so this never fails; it's a cheap safety net and
    // keeps `has_path` exercised in the binary too.
    let passable = |x: i32, y: i32| is_passable(map.tile(x, y));
    debug_assert!(pathfinding::has_path(start, exit, passable));

    Ok(map)
}

#[inline]
fn idx(w: usize, x: usize, y: usize) -> usize {
    y * w + x
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid, seeded config used by most tests.
    fn config() -> MapConfig {
        MapConfig::from_toml(
            r#"
                width = 30
                height = 20
                unmineable_count = 20
                start = { x = 15, y = 19 }
                exit  = { x = 5,  y = 0 }
                structures = [[8, 5], [9, 5], [10, 5]]
            "#,
        )
        .expect("valid config")
    }

    #[test]
    fn generates_expected_shape_gaps_and_structures() {
        let map = generate(&config(), 12345).expect("generates");
        assert_eq!(map.width, 30);
        assert_eq!(map.height, 20);

        let start = map.start_pos();
        let exit = map.exit_pos();
        assert_eq!(start, (15, 19));
        assert_eq!(exit, (5, 0));

        // Structures are exactly the configured ones.
        assert_eq!(map.tile(8, 5), Tile::Unbreakable);
        assert_eq!(map.tile(9, 5), Tile::Unbreakable);
        assert_eq!(map.tile(10, 5), Tile::Unbreakable);

        // Border ring is unbreakable everywhere except the two dirt gaps.
        for y in 0..map.height as i32 {
            for x in 0..map.width as i32 {
                if x == 0 || y == 0 || x == map.width as i32 - 1 || y == map.height as i32 - 1 {
                    let expected = if (x as usize, y as usize) == exit || (x as usize, y as usize) == start {
                        Tile::Dirt
                    } else {
                        Tile::Unbreakable
                    };
                    assert_eq!(map.tile(x, y), expected, "border cell ({x},{y})");
                }
            }
        }
    }

    #[test]
    fn places_exactly_unmineable_count() {
        let cfg = config();
        let map = generate(&cfg, 7).expect("generates");
        assert_eq!(map.count(Tile::Unmineable), cfg.unmineable_count);
    }

    #[test]
    fn same_seed_is_identical_and_different_seeds_differ() {
        let a = generate(&config(), 12345).expect("a");
        let b = generate(&config(), 12345).expect("b");
        assert_eq!(a.tiles, b.tiles, "same seed must reproduce the map exactly");

        // With ~40 unmineable cells out of a 500-cell interior, two different
        // seeds should almost never collide.
        let c = generate(&config(), 99999).expect("c");
        assert_ne!(a.tiles, c.tiles, "different seeds must differ");
    }

    #[test]
    fn always_has_a_mineable_path_start_to_exit() {
        // Across many seeds, the generated map must remain solvable via a
        // mineable-only path (never unmineable/blocked on the corridor).
        for seed in 1..=40 {
            let map = generate(&config(), seed).expect("generates");
            let start = map.start_pos();
            let exit = map.exit_pos();
            let passable = |x: i32, y: i32| {
                matches!(map.tile(x, y), Tile::Mineable | Tile::Dirt)
            };
            assert!(
                pathfinding::has_path(
                    (start.0 as i32, start.1 as i32),
                    (exit.0 as i32, exit.1 as i32),
                    passable
                ),
                "no mineable path for seed {seed}"
            );
        }
    }

    #[test]
    fn structure_on_corridor_yields_valid_map() {
        // A structure that intersects the direct route must not produce an
        // error; A* routes around it and the map remains valid.
        let cfg = MapConfig::from_toml(
            r#"
                width = 30
                height = 20
                unmineable_count = 10
                start = { x = 15, y = 19 }
                exit  = { x = 15, y = 0 }
                structures = [[15, 10], [15, 11]]
            "#,
        )
        .expect("valid");
        let map = generate(&cfg, 42).expect("generates despite structure on path");
        assert!(pathfinding::has_path(
            (15, 19),
            (15, 0),
            |x, y| matches!(map.tile(x, y), Tile::Mineable | Tile::Dirt)
        ));
    }

    #[test]
    fn structure_does_not_cover_a_gap() {
        let cfg = MapConfig::from_toml(
            r#"
                width = 30
                height = 20
                unmineable_count = 0
                start = { x = 15, y = 19 }
                exit  = { x = 5,  y = 0 }
                structures = [[5, 0], [15, 19]]
            "#,
        )
        .expect("valid");
        let map = generate(&cfg, 1).expect("generates");
        assert_eq!(map.tile(5, 0), Tile::Dirt, "exit gap not covered");
        assert_eq!(map.tile(15, 19), Tile::Dirt, "start gap not covered");
    }

    #[test]
    fn too_few_candidate_cells_errors() {
        // A tiny map (interior 2x2) with unmineable_count beyond its capacity.
        let cfg = MapConfig::from_toml(
            r#"
                width = 4
                height = 4
                unmineable_count = 4
                start = { x = 0, y = 2 }
                exit  = { x = 3, y = 2 }
            "#,
        )
        .expect("valid");
        assert!(matches!(
            generate(&cfg, 1),
            Err(GenError::NotEnoughCells { .. })
        ));
    }

    #[test]
    fn resolve_seed_prefers_config_seed() {
        let cfg = config(); // has no seed set
        assert!(cfg.seed.is_none());
        // With no seed, it still returns a u64.
        let _ = resolve_seed(&cfg);

        let mut seeded = config();
        seeded.seed = Some(987);
        assert_eq!(resolve_seed(&seeded), 987);
    }

    #[test]
    fn places_exactly_gold_count_on_mineable_cells_only() {
        let mut cfg = config();
        cfg.gold_count = 12;
        let map = generate(&cfg, 12345).expect("generates");
        assert_eq!(map.gold.len(), cfg.gold_count as usize, "gold count matches");
        for &(x, y) in &map.gold {
            assert_eq!(map.tile(x as i32, y as i32), Tile::Mineable, "gold is only ever on mineable rock");
        }
    }

    #[test]
    fn same_seed_places_gold_identically_and_different_seeds_differ() {
        let mut cfg = config();
        cfg.gold_count = 12;
        let a = generate(&cfg, 42).expect("a");
        let b = generate(&cfg, 42).expect("b");
        assert_eq!(a.gold, b.gold, "same seed must reproduce gold placement");

        let c = generate(&cfg, 4242).expect("c");
        assert_ne!(a.gold, c.gold, "different seeds usually differ");
    }

    #[test]
    fn gold_never_lands_on_unmineable_or_corridor_breaking_cells() {
        // With unmineable_count == interior left untouched, gold must not appear
        // on any unmineable cell even when the count is large.
        let mut cfg = config();
        cfg.unmineable_count = 20;
        cfg.gold_count = 60;
        // The interior (28*18 = 504) comfortably holds 20 unmineable + 60 gold.
        let map = generate(&cfg, 7).expect("generates");
        for &(x, y) in &map.gold {
            assert_eq!(map.tile(x as i32, y as i32), Tile::Mineable);
            assert_ne!(map.tile(x as i32, y as i32), Tile::Unmineable);
        }
    }

    #[test]
    fn loads_and_generates_the_committed_sample_maps() {
        for path in ["assets/maps/level01.toml", "assets/maps/level02.toml"] {
            let toml = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
            let cfg = MapConfig::from_toml(&toml).unwrap_or_else(|e| panic!("{path}: {e}"));
            let seed = resolve_seed(&cfg);
            let map = generate(&cfg, seed).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert_eq!(map.count(Tile::Unmineable), cfg.unmineable_count, "{path}");
            let start = map.start_pos();
            let exit = map.exit_pos();
            assert!(
                pathfinding::has_path(
                    (start.0 as i32, start.1 as i32),
                    (exit.0 as i32, exit.1 as i32),
                    |x, y| matches!(map.tile(x, y), Tile::Mineable | Tile::Dirt)
                ),
                "{path} must be solvable"
            );
        }
    }
}
