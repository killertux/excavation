//! Shop purchase logic (pure): deciding whether an item can be bought and
//! applying the purchase to a [`Run`]. The UI (app.rs) renders items and calls
//! into here.

use crate::config::game::GameConfig;
use crate::game::consumables::ConsumableKind;
use crate::game::run::Run;
use crate::game::upgrades;

/// The purchasable shop items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShopItem {
    /// Permanently +Walk speed (additive per level).
    WalkSpeed,
    /// Permanently faster mining (multiplicative per level).
    MiningSpeed,
    /// +1 life (capped at `player.max_lives`).
    Lives,
    /// +1 Super Pick consumable.
    SuperPick,
    /// +1 Sticky Smell consumable.
    StickySmell,
}

/// Why a purchase was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopError {
    /// Not enough gold for the item's cost.
    NotEnoughGold { cost: u32, gold: u32 },
    /// The item is already maxed (upgrades at max level, lives at the cap).
    AlreadyMaxed,
}

/// The cost of `item` for the current run state (upgrade costs grow per level).
pub fn cost(item: ShopItem, run: &Run, cfg: &GameConfig) -> u32 {
    match item {
        ShopItem::WalkSpeed => {
            upgrades::cost(run.upgrades.walk_speed, cfg.upgrades.walk_speed.max_level, &cfg.upgrades.walk_speed.cost_per_level)
                .unwrap_or(u32::MAX)
        }
        ShopItem::MiningSpeed => {
            upgrades::cost(run.upgrades.mining_speed, cfg.upgrades.mining_speed.max_level, &cfg.upgrades.mining_speed.cost_per_level)
                .unwrap_or(u32::MAX)
        }
        ShopItem::Lives => cfg.lives.cost,
        ShopItem::SuperPick => cfg.consumables.super_pick.cost,
        ShopItem::StickySmell => cfg.consumables.sticky_smell.cost,
    }
}

/// Whether `item` can currently be bought (not maxed, affordable).
pub fn can_buy(item: ShopItem, run: &Run, cfg: &GameConfig) -> bool {
    let not_maxed = match item {
        ShopItem::WalkSpeed => run.upgrades.walk_speed < cfg.upgrades.walk_speed.max_level,
        ShopItem::MiningSpeed => run.upgrades.mining_speed < cfg.upgrades.mining_speed.max_level,
        ShopItem::Lives => run.lives < cfg.player.max_lives,
        ShopItem::SuperPick | ShopItem::StickySmell => true,
    };
    not_maxed && run.gold >= cost(item, run, cfg)
}

/// Buy `item`: deduct gold and apply its effect.
pub fn buy(item: ShopItem, run: &mut Run, cfg: &GameConfig) -> Result<(), ShopError> {
    if !can_buy(item, run, cfg) {
        // Distinguish maxed from unaffordable for the UI.
        let maxed = match item {
            ShopItem::WalkSpeed => run.upgrades.walk_speed >= cfg.upgrades.walk_speed.max_level,
            ShopItem::MiningSpeed => run.upgrades.mining_speed >= cfg.upgrades.mining_speed.max_level,
            ShopItem::Lives => run.lives >= cfg.player.max_lives,
            ShopItem::SuperPick | ShopItem::StickySmell => false,
        };
        if maxed {
            return Err(ShopError::AlreadyMaxed);
        }
        return Err(ShopError::NotEnoughGold { cost: cost(item, run, cfg), gold: run.gold });
    }

    let c = cost(item, run, cfg);
    run.gold -= c;
    match item {
        ShopItem::WalkSpeed => run.upgrades.walk_speed += 1,
        ShopItem::MiningSpeed => run.upgrades.mining_speed += 1,
        ShopItem::Lives => run.lives += 1,
        ShopItem::SuperPick => run.consumables.add(ConsumableKind::SuperPick),
        ShopItem::StickySmell => run.consumables.add(ConsumableKind::StickySmell),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::game::GameConfig;
    use crate::config::map::MapConfig;
    use crate::game::run::Run;
    use crate::game::upgrades::Upgrades;

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
        files = ["a.toml"]
    "#;

    fn game() -> GameConfig {
        GameConfig::from_toml(GAME_TOML).expect("valid game config")
    }

    fn map_cfg() -> MapConfig {
        let mut c = MapConfig::from_toml(
            r#"
                width = 30
                height = 20
                unmineable_count = 20
                beast_count = 0
                start = { x = 15, y = 19 }
                exit  = { x = 5,  y = 0 }
            "#,
        )
        .expect("valid map");
        c.seed = Some(1);
        c
    }

    fn run_with_gold(gold: u32) -> Run {
        // Two maps so `begin_next_level` (used by the upgrade test) can advance.
        let mut r = Run::new(game(), vec![map_cfg(), map_cfg()]).expect("run builds");
        r.gold = gold;
        r
    }

    #[test]
    fn buy_consumables_adds_one_and_deducts_gold() {
        let mut r = run_with_gold(200);
        let c = cost(ShopItem::SuperPick, &r, &game());
        assert_eq!(c, 60);
        buy(ShopItem::SuperPick, &mut r, &game()).unwrap();
        assert_eq!(r.consumables.super_pick, 1);
        assert_eq!(r.gold, 140);

        buy(ShopItem::StickySmell, &mut r, &game()).unwrap();
        assert_eq!(r.consumables.sticky_smell, 1);
        assert_eq!(r.gold, 100);
    }

    #[test]
    fn buy_walk_speed_increments_level_and_cost_grows() {
        let mut r = run_with_gold(1000);
        assert_eq!(cost(ShopItem::WalkSpeed, &r, &game()), 50);
        buy(ShopItem::WalkSpeed, &mut r, &game()).unwrap();
        assert_eq!(r.upgrades.walk_speed, 1);
        assert_eq!(r.gold, 950);
        // Next level costs more.
        assert_eq!(cost(ShopItem::WalkSpeed, &r, &game()), 100);
    }

    #[test]
    fn buy_lives_caps_at_max_lives() {
        let mut r = run_with_gold(1000);
        r.lives = 9; // at the cap
        assert!(!can_buy(ShopItem::Lives, &r, &game()));
        assert_eq!(buy(ShopItem::Lives, &mut r, &game()), Err(ShopError::AlreadyMaxed));
    }

    #[test]
    fn upgrading_to_max_then_cannot_buy() {
        let mut r = run_with_gold(10_000);
        r.upgrades.walk_speed = 5; // at max
        assert!(!can_buy(ShopItem::WalkSpeed, &r, &game()));
        assert_eq!(buy(ShopItem::WalkSpeed, &mut r, &game()), Err(ShopError::AlreadyMaxed));
    }

    #[test]
    fn not_enough_gold_rejects_purchase() {
        let mut r = run_with_gold(40);
        buy(ShopItem::SuperPick, &mut r, &game()).unwrap_err();
        assert_eq!(r.gold, 40, "no gold deducted on failure");
        assert_eq!(r.consumables.super_pick, 0);
    }

    #[test]
    fn can_buy_report_affordability_and_max_state() {
        let r = run_with_gold(200);
        assert!(can_buy(ShopItem::SuperPick, &r, &game()));
        let poor = run_with_gold(10);
        assert!(!can_buy(ShopItem::SuperPick, &poor, &game()));
        // Mining speed at max -> cannot buy regardless of gold.
        let mut maxed = run_with_gold(10_000);
        maxed.upgrades.mining_speed = 5;
        assert!(!can_buy(ShopItem::MiningSpeed, &maxed, &game()));
    }

    #[test]
    fn upgrades_are_honored_by_run_build() {
        // Buying walk speed then beginning a level must raise the player speed.
        let mut r = run_with_gold(1000);
        let base_speed = r.level.player.speed;
        buy(ShopItem::WalkSpeed, &mut r, &game()).unwrap();
        r.begin_next_level().expect("rebuild level");
        let upgraded = r.level.player.speed;
        assert!(upgraded > base_speed, "next level uses the upgraded speed");
        let expected = 240.0 + 15.0;
        assert!((upgraded - expected).abs() < 1e-3, "got {upgraded}");
    }
}
