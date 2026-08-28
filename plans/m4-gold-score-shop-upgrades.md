# M4 — Gold, Score, Shop, Upgrades & Consumables (Detailed Plan)

> Scope: milestone **M4** from [`PLAN.md`](../PLAN.md).
> **Working binary =** mine gold, finish a level, see your score, spend gold in a
> shop between levels, and see the five purchases (Walk Speed, Mining Speed,
> Lives, Super Pick, Sticky Smell) take effect in the next level — all driven by
> `game.toml`.

---

## 0. Current Baseline (post-M3)

- Beast AI is complete: `game/beast.rs` implements the §5 knowledge model
  (`known_mineable`, perception) and decision loop (charge → dirt-only A\* →
  diggable A\* → nearest-to-player known rock). Beasts dig, catch, and the level
  auto-restarts with a **new random map** on death.
- `game/level.rs` owns the per-level sim (map/player/beasts/lives) and returns
  `LevelEvent { None, Completed, Caught, GameOver }`. `app.rs` renders it plus a
  text lives counter and placeholder LEVEL COMPLETE / GAME OVER overlays.
- `config/game.rs` has `[player]` (base_speed, base_mining_time, starting_lives)
  and `[beast]`. `config/map.rs` already parses `gold_count` (unused).
- `game.toml` and `level01/level02.toml` are committed; `App` hard-loads
  `level01.toml` only. **110 tests pass.**
- Assets: the combined atlas (`My project atlas.png`, 560×694) already contains
  every sprite M4 needs, but the loader only slices characters/terrain/burst.
  The M4 sprites (24×24 each, from the atlas `.json`) are **not sliced yet**.

---

## 1. Outcome (acceptance checklist)

- [x] `game.toml` gains `max_lives`, `[upgrades]`, `[lives]`, `[consumables]`,
      `[score]`, and `[map_order]`; all load + validate.
- [x] `gold_count` mineable rocks hide gold; mining one drops a gold pickup; the
      player collects it by walking over it.
- [x] A per-level score (speed + gold) is computed and shown at level end, with a
      running total.
- [x] A shop between levels lets the player buy Walk Speed, Mining Speed, Lives,
      Super Pick, and Sticky Smell (costs/effects from TOML).
- [x] Walk Speed and Mining Speed upgrades change the next level's player.
- [x] Super Pick (3s): instant mining of any rock except unbreakable.
- [x] Sticky Smell (5s): beasts wander randomly instead of pathfinding.
- [x] One consumable effect active at a time; using one replaces the current.
- [x] Levels play in `map_order` sequence; lives/gold/upgrades/consumables/score
      persist across levels.
- [x] All new unit tests pass.

---

## 2. Dependencies

None expected. (Icon rects are hardcoded like the existing loader; the Aseprite
`.json` stays as the human-readable reference. If we later want to parse it,
`serde_json` would be the only addition — not needed now.)

---

## 3. Config Changes (`assets/game.toml` + `config/game.rs`)

```toml
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
```

`config/game.rs` adds (all validated: positive/finite, `cost_per_level` non-empty
and length ≥ `max_level`, `max_lives ≥ starting_lives`):

```rust
PlayerConfig      { .., max_lives: u32 }
UpgradeConfig     { max_level: u32, cost_per_level: Vec<u32>, effect: … }
UpgradesConfig    { walk_speed: WalkSpeedConfig, mining_speed: MiningSpeedConfig }
LivesConfig       { cost: u32 }
ConsumableConfig  { cost: u32, duration: f32 }
ConsumablesConfig { super_pick: ConsumableConfig, sticky_smell: ConsumableConfig }
ScoreConfig       { par_time: f32, time_multiplier: f32, gold_multiplier: f32 }
GameConfig        { .., upgrades, lives, consumables, score, map_order: Vec<String> }
```

`WalkSpeedConfig`/`MiningSpeedConfig` share `{ max_level, cost_per_level }`; the
effect value differs (`speed_increase_per_level` vs `mining_time_multiplier_per_level`).

---

## 4. Asset Changes (`assets/ids.rs` + `assets/mod.rs`)

Slice the new 24×24 sprites (from the atlas `.json`, all at `x=0`):

| Sprite | Rect | New id |
| --- | --- | --- |
| Gold | (0, 396, 24×24) | `PickupId::Gold` |
| Super Pickaxe | (0, 421, 24×24) | `IconId::SuperPick` |
| Jar of stench | (0, 446, 24×24) | `IconId::StickySmell` |
| Heart | (0, 471, 24×24) | `IconId::Heart` |
| Buy Heart | (0, 496, 24×24) | `IconId::BuyLives` |
| Boot | (0, 521, 24×24) | `IconId::WalkSpeed` |
| Pickaxe | (0, 546, 24×24) | `IconId::MiningSpeed` |

- Add `PickupId` and `IconId` enums + `Assets::pickup(id)` / `Assets::icon(id)`.
- Add a `load_icon(rect, scale_mode)` helper (24×24 → 16×16 `Stretch` for pickups
  and HUD icons; shop icons can render at 16×16 too for now).
- No new art is required — it is all already in the committed atlas.

---

## 5. Gold

### 5.1 Map stores gold
Add `Map.gold: HashSet<(usize, usize)>` plus `Map::has_gold(x, y)` and
`Map::take_gold(x, y) -> bool` (removes + returns whether gold was present).
Generation (`game/generation.rs`) places `gold_count` gold into random
**Mineable** cells using the same seeded RNG (after the unmineable flip), so gold
is reproducible with the seed. Update the `Map { … }` literals in tests (add a
`Map::new(..)` helper to reduce churn).

### 5.2 Pickup entity
`game/pickup.rs`: `Pickup { pos: Vec2, kind: PickupKind }` (only `Gold` for now).
`Level` owns `pickups: Vec<Pickup>`.

### 5.3 Drop & collect
- `Player::update` and `Beast::update` return `Option<(i32, i32)>` — the cell they
  just excavated (or `None`). (Small ripple: callers/tests ignore or use the
  return.)
- `Level::update`, when an update returns `Some(cell)`, calls `map.take_gold(cell)`
  and, if true, spawns a `Gold` pickup at that cell's centre.
- The player collects a pickup on hitbox overlap (`movement::hits`): increment the
  level's `gold_collected` and remove the pickup.

### 5.4 Gold accounting (bank-on-complete)
- `Level.gold_collected: u32` = gold gathered in the **current attempt**.
- On `Completed`, `Run` banks it: `run.gold += level.gold_collected`.
- On `Caught` (restart), the attempt's gold is **discarded** (reset to 0) — gold
  only banks when you escape. See §18 decision 1.

---

## 6. Score

`game/score.rs` (pure):

```
level_score(elapsed, gold, cfg) =
    max(0, par_time - elapsed) * time_multiplier + gold * gold_multiplier
```

- `Level` tracks `elapsed` (seconds since level start; reset on restart).
- On `Completed`, `Run` computes the level score, shows it, and adds it to
  `run.score_total` (a `u64`, or `f64` — pick `u64` and round).

---

## 7. Upgrades (`game/upgrades.rs`)

```rust
struct Upgrades { walk_speed: u32, mining_speed: u32 }   // owned levels

fn walk_speed(base, upgrades, cfg) -> f32     // base + lvl * speed_increase_per_level
fn mining_time(base, upgrades, cfg) -> f32    // base * multiplier^lvl
fn cost(upgrade, upgrades, cfg) -> Option<u32>   // None when maxed
```

- `Run` owns `upgrades`. Effective player speed/mining time are computed from
  `upgrades` and passed into `Level::new` (Level no longer reads base values
  directly for the player).
- Buying increments the level (capped at `max_level`) and deducts gold.

---

## 8. Consumables (`game/consumables.rs`)

```rust
struct Consumables { super_pick: u32, sticky_smell: u32 }        // owned counts
enum ConsumableKind { SuperPick, StickySmell }
struct ActiveEffect { kind: ConsumableKind, remaining: f32 }     // in Level
```

- `Run` owns the counts (persist across levels); `Level` owns the **active
  effect** (per-level, resets when entering a level).
- `use_consumable(kind)`: if count > 0, decrement and set
  `active_effect = Some(ActiveEffect { kind, remaining: duration })` — **replacing**
  any current effect (one-at-a-time rule).
- `Level::update` ticks `remaining -= dt` and clears the effect at 0.

---

## 9. Super Pick (player mining changes)

`Player::update` gains a `super_pick: bool` parameter (or an equivalent flag).

- `game/mining.rs::pushed_target` (or a wrapper) must, during Super Pick, also
  target **`Unmineable`** cells (still never `Unbreakable`).
- During Super Pick, mining is **instant**: the rock breaks the frame the player
  pushes into it (progress jumps to completion). Unmineable rocks become `Dirt`.
- Outside Super Pick, behaviour is unchanged.

Update `mining.rs`/`player.rs` and their tests.

---

## 10. Sticky Smell (beast wander)

- Add `BeastState::Wander { dir: Vec2, timer: f32 }`.
- `Level` passes `sticky: bool` to `Beast::update`. While `sticky`:
  - Skip `decide`/`replan` entirely (pathfinding disabled).
  - Move in a random cardinal direction; re-roll the direction when blocked or
    every ~0.5s. Movement stays collision-blocked (can't enter `Unmineable` /
    `Unbreakable`), and the beast **does not dig** while wandering.
- Randomness uses a locally-seeded `macroquad::rand::RandGenerator` (injectable
  for deterministic tests).
- Keep the existing `dig_frame`/animation wiring.

---

## 11. Run Container (`game/run.rs`, new) + Level refactor

Cross-level state moves out of `Level` into a new `Run` (testable, no GPU):

```rust
struct Run {
    gold: u32,
    upgrades: Upgrades,
    consumables: Consumables,
    lives: u32,
    score_total: u64,
    level_index: usize,
    map_order: Vec<String>,        // from game.toml [map_order]
    level: Level,
}

enum RunEvent { Playing, Caught, LevelCompleted { score: u64 }, GameOver, Victory }
```

**Level refactor** (remove lives; they live in `Run` now):
- `Level` drops `lives`, `starting_lives`, and the `on_caught` lives logic;
  `Level::update` returns `Caught` as a plain event (no lives change).
- `Level` gains `gold_collected`, `elapsed`, `pickups`, and `active_effect`.
- `Level::new(…)` takes the **effective** player speed/mining time (from upgrades)
  and beast speed/mining time/replan interval.

**Run flow:**
- `Run::new(game, map_cfg0, seed)` builds the first `Level` with `starting_lives`.
- `Run::update(input, dt)`:
  - `Level::update` → `Caught`: `lives -= 1`; `0` → `GameOver`; else
    `level.restart(fresh_seed)` (discard the attempt's gold), return `Caught`.
  - `Completed`: bank gold, add level score, return `LevelCompleted { score }`.
  - last level completed → `Victory` (placeholder for M5).
- `Run::begin_next_level()`: `level_index += 1`, build the next `Level` (lives/gold/
  upgrades/consumables/score persist; consumable effect resets).
- `Run::buy(item)` (shop) and `Run::use_consumable(kind)` delegate to pure shop
  logic.

---

## 12. Shop (`game/shop.rs` + `app.rs`)

**Pure logic (`game/shop.rs`):**
- `ShopItem { WalkSpeed, MiningSpeed, Lives, SuperPick, StickySmell }`.
- `can_buy(item, run, cfg) -> bool` and `buy(item, run, cfg) -> Result<(), ShopError>`
  (deduct gold; apply upgrade / add life up to `max_lives` / add consumable).

**App shop screen (simple, text-only — the real UI is M5):**
- Shown after `LevelCompleted`; list items with cost and owned state, plus
  "Continue → next level".
- Navigation: Up/Down/W/S to select, Enter/Space to buy, the Continue option (or
  Esc) advances to the next level (or Victory after the last).
- Extend the `LEVEL COMPLETE` overlay to show `Score` and `Gold` for the level
  before entering the shop.
- `GameState` gains `Shop` and `Victory` (placeholder overlay).

---

## 13. Input (`input.rs`)

Add one-shot (edge-triggered) consumable keys and note shop navigation:

```rust
struct Input { move_: Vec2, use_super_pick: bool, use_sticky_smell: bool }
```

- `use_super_pick` ← `is_key_pressed(KeyCode::Key1 | Q)`
- `use_sticky_smell` ← `is_key_pressed(KeyCode::Key2 | E)`
- Shop navigation reads keys in `app.rs` (Up/Down/W/S, Enter/Space, Esc).

---

## 14. Sample Maps & map_order

- `level01.toml` and `level02.toml` already declare `gold_count = 8`.
- `game.toml` `[map_order].files` lists both (see §3). `App` uses `map_order`
  instead of hard-coding `level01.toml`.

---

## 15. Tests (unit)

- `config/game.rs`: full document parses; reject bad `max_lives`, empty
  `cost_per_level`, non-positive costs/durations.
- `generation`: `gold_count` gold placed on `Mineable` cells only; count matches;
  same seed → identical gold placement.
- `pickup`/`gold`: mining a gold rock spawns a pickup; overlap collects it and
  increments `gold_collected`; `take_gold` clears it (no double-collect).
- `score`: formula correctness (incl. clamped par-time and multipliers).
- `upgrades`: cost progression + `None` at max; speed/mining-time math.
- `consumables`: use decrements count; second use replaces the active effect;
  effect expires after its duration.
- `player`/`mining`: Super Pick targets `Unmineable` (not `Unbreakable`) and is
  instant; normal mining unchanged.
- `beast`: Sticky Smell → `Wander`, no pathfinding, no digging; still blocked by
  rock.
- `run`: gold banks on complete and discards on caught; lives persist across
  levels; `level_index`/`map_order` advance; last level → `Victory`; `GameOver`
  at 0 lives.
- `shop`: `can_buy`/`buy` gold math, max-level cap, `max_lives` cap.

---

## 16. Task List (ordered)

- [x] 1. Extend `config/game.rs` + `game.toml` (upgrades/lives/consumables/score/map_order/max_lives) + tests.
- [x] 2. Slice the 7 new sprites in `assets/` (`PickupId`, `IconId`, accessors).
- [x] 3. Add `Map.gold` + generation gold placement + `has_gold`/`take_gold` + tests.
- [x] 4. Add `game/pickup.rs` + gold drop/collect; make `Player`/`Beast::update`
      return the excavated cell.
- [x] 5. Add `game/score.rs` + `Level.elapsed`/`gold_collected`.
- [x] 6. Add `game/upgrades.rs` + `game/consumables.rs`.
- [x] 7. Super Pick: extend `mining.rs`/`player.rs` + tests.
- [x] 8. Sticky Smell: add `BeastState::Wander` + `sticky` flag + tests.
- [x] 9. Add `game/run.rs`; refactor `Level` (drop lives; add elapsed/gold/pickups/effect).
- [x] 10. Add `game/shop.rs` (pure buy logic) + tests.
- [x] 11. Wire `app.rs`: `Run` + `GameState::{Shop, Victory}`, score/gold overlay,
       simple shop screen, consumable keys, `map_order`.
- [x] 12. Full `cargo test`; desktop run + `--screenshot` (gold pickup, shop, super
       pick, sticky smell); WASM build/boot check.

---

## 17. Decisions & Risks (confirm if they differ)

1. **Gold banks only on level completion.** Gold gathered in an attempt is
   discarded if you die (prevents "die to farm gold"). Confirm if you'd rather
   keep collected gold even on death.
2. **Score is `u64`, rounded** from the float formula.
3. **Shop is a simple text/keyboard screen** in M4 (the polished 9-slice UI and
   full menus are M5).
4. **`map_order` drives linear progression**; level *selection* (choose any
   unlocked level) and unlock persistence are M5.
5. **Wander does not dig** and is cardinal-only, re-rolling every ~0.5s or on a
   block.
6. **Active consumable is per-level** (resets when you enter the next level);
   owned counts persist in `Run`.
7. **Upgrade math:** walk speed is additive per level; mining time is
   multiplicative per level (`0.85^lvl`), matching the §9.1 example comment.

---

## 18. Out of Scope (later milestones)

- Full menu system, level select, pause, real HUD/hearts, save/load, settings (M5).
- Audio (M6).
- Map editor (M7).
- Final 10-level balancing (M8).
