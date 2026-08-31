//! Per-level scoring (pure). A level score rewards finishing quickly and
//! collecting gold, both scaled by `game.toml` `[score]` values.

use crate::config::game::ScoreConfig;

/// Score for a completed level.
///
/// `elapsed` is seconds taken (capped at the par time so slow rounds still score
/// 0 for time), `gold` is the gold collected this level, and `cfg` holds the
/// `[score]` tuning. Returns a rounded `u64`.
pub fn level_score(elapsed: f32, gold: u32, cfg: &ScoreConfig) -> u64 {
    let time_part = (cfg.par_time - elapsed).max(0.0) * cfg.time_multiplier;
    let gold_part = gold as f32 * cfg.gold_multiplier;
    (time_part + gold_part).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::game::ScoreConfig;

    fn cfg() -> ScoreConfig {
        ScoreConfig {
            par_time: 60.0,
            time_multiplier: 10.0,
            gold_multiplier: 5.0,
        }
    }

    #[test]
    fn full_par_time_and_no_gold_scores_zero_time() {
        // Exactly on par: the time term is 0; only gold counts.
        assert_eq!(level_score(60.0, 0, &cfg()), 0);
    }

    #[test]
    fn faster_than_par_earns_time_score() {
        // 10s under par = 50s * 10 = 500.
        assert_eq!(level_score(10.0, 0, &cfg()), 500);
    }

    #[test]
    fn gold_adds_gold_multiplier_scores() {
        // 8 gold = 8 * 5 = 40.
        assert_eq!(level_score(60.0, 8, &cfg()), 40);
    }

    #[test]
    fn time_and_gold_combine() {
        // 20s elapsed (40s under par) * 10 = 400, + 3 gold * 5 = 15.
        assert_eq!(level_score(20.0, 3, &cfg()), 415);
    }

    #[test]
    fn over_par_clamps_to_zero_time() {
        // 100s (40s over par) -> time term clamps to 0, gold still counts.
        assert_eq!(level_score(100.0, 2, &cfg()), 10);
    }
}
