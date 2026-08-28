# M3 — Beasts & Lives (Detailed Plan)

> Scope: milestone **M3** from [`PLAN.md`](../PLAN.md).
> **Working binary =** beasts hunt and dig toward the player using the spec'd AI;
> getting caught costs a life and restarts the level; zero lives ends the game.

---

## 0. Current Baseline (post-M2) — read this first

M2 (and its follow-ups) landed with changes from the original M2 plan. M3 is
planned against the **actual current code**, not the M2 plan text:

- **Tiles** are now `Mineable | Unmineable | Unbreakable | Dirt` (4 states, 3
  visual families). Doors were replaced by **start/exit dirt "gaps"** stored as
  `Map.start` / `Map.exit`.
- **32 px tiles** at native resolution (camera zoom 1.0), from a single combined
  atlas (`assets/images/My project atlas.png`) with directional character
  cycles and terrain **autotiling** (Wang masks) via `game/terrain.rs`.
- **Mining is contact-based** (no mine key): walk into a rock and hold the
  direction; `game/mining.rs` + `game/player.rs` handle it.
- A simple **`Beast` already exists** (`game/beast.rs`): it walks straight toward
  the player, is blocked by solids, but **cannot dig, cannot catch, and has no
  knowledge model**. `App` spawns exactly one beast at the exit gap and ignores
  `beast_count`.
- `pathfinding.rs` (generic 4-neighbour A\*), `movement.rs` (shared collision,
  `HITBOX_HALF = 12.0`), `generation.rs` (seeded), and `config/` are all in place.
- Config already parses `beast_count`, `beast_speed_multiplier`,
  `beast_mining_time_multiplier` (unused). `game.toml` has only `[player]`.

M3 turns the stub beast into the full AI and adds lives/restart/game-over.

---

## 1. Outcome (acceptance checklist)

- [x] `game.toml` gains `[beast]` (speed, mining time, replan interval) and
      `player.starting_lives`; both load and validate.
- [x] `beast_count` beasts spawn per level and chase independently.
- [x] Beasts implement the §5 knowledge model: a growing `known_mineable` set
      learned from adjacent cells.
- [x] Beasts dig through known mineable rocks (never unmineable/unbreakable).
- [x] Beast decision loop: straight-line charge → A\* to player → A\* toward the
      known mineable rock closest to the player → idle (re-plans on a timer /
      when the plan breaks).
- [x] Touching a beast costs one life and regenerates a new map; zero lives =
      game over (placeholder overlays for both).
- [x] Lives are shown as a simple on-screen counter (real HUD is M5).
- [x] All new unit tests pass (beast AI, catch, lives, restart, multi-beast).

---

## 2. Dependencies

None. (A\* is in-house; RNG already uses `macroquad::rand`.)

---

## 3. Config Changes

### 3.1 `assets/game.toml` (extend)

```toml
[player]
base_speed = 240.0
base_mining_time = 0.8
starting_lives = 3

[beast]
base_speed = 140.0        # world px/s (slower than the player)
base_mining_time = 1.6    # seconds to dig one rock (slower than the player)
replan_interval = 0.25    # seconds between AI re-plans
```

`config/game.rs` → add `PlayerConfig.starting_lives: u32` and a new
`BeastConfig { base_speed, base_mining_time, replan_interval }`. Validate all are
positive/finite (`starting_lives > 0`, `replan_interval > 0`). `max_lives` is
deferred to M4 (the lives shop).

### 3.2 Per-beast tuning (already parsed)

- `beast.speed = base_speed * map.beast_speed_multiplier`
- `beast.mining_time = base_mining_time * map.beast_mining_time_multiplier`

---

## 4. Beast Entity (`game/beast.rs`)

Extend the existing `Beast` (replace the `BEAST_SPEED` const):

```rust
struct Beast {
    pos: Vec2,            // world px (center of hitbox)
    facing: Vec2,         // animation/last-move direction
    motion: BeastMotion,  // animation state
    speed: f32,
    mining_time: f32,     // seconds to dig one rock
    replan_interval: f32,
    replan_timer: f32,
    known_mineable: HashSet<(i32, i32)>,   // rocks confirmed diggable
    state: BeastState,
}
```

`BeastState` (drives behaviour each frame):

```rust
enum BeastState {
    Idle,
    Charge,                              // straight-line toward player
    Follow { path: Vec<(i32,i32)>, next: usize },
    Dig { target: (i32,i32), progress: f32 },
}
```

`Beast::update(&mut self, player_pos: Vec2, map: &mut Map, dt: f32)` — now takes
`&mut Map` because the beast digs. It:

1. **Perceives** its four cardinal neighbours; for each `Mineable` cell it inserts
   the cell into `known_mineable`.
2. **Re-plans** when `replan_timer` elapses, when the current plan is invalidated
   (path exhausted / target cell no longer reachable), or after finishing a dig.
3. **Acts** per its state (below).

Keep the existing `dir()` and directional walk/idle animation (reuse the walk
cycle already there; no new animation assets needed).

---

## 5. Beast AI (the decision loop)

The beast **always knows the player's position** and **sees the physical map**
(which cells are open `Dirt`, which are `Rock`, which are `Unbreakable`). What it
must *learn* locally is a rock's **mineability**: a `Mineable` rock becomes
diggable-by-the-beast only once the beast has been adjacent to it (added to
`known_mineable`). Unknown rocks are treated as blocked.

Passability (for A\*, from the beast's point of view):

```rust
fn passable(cell, map, known) -> bool {
    matches!(map.tile(cell), Tile::Dirt)
        || (map.tile(cell) == Tile::Mineable && known.contains(cell))
}
```

Decision function (pure, unit-tested) — `decide(map, known, beast_cell, player_cell) -> Plan`:

1. **Straight-line charge.** If the beast and player share a row **or** a column
   and every cell strictly between them is `Dirt` (no rock/wall in the way),
   return `Charge` (move directly toward the player). This is the "sees a clear
   straight path" case.
2. **A\* to the player.** Else run `pathfinding::astar(beast_cell, player_cell,
   passable)`. If it returns a path, return `Path(path)`.
3. **A\* toward the known mineable rock closest to the player.** Else, among
   `known_mineable`, choose the rock **closest to the player** (by Manhattan
   distance), then A\* to it and return `Path(path)`. This is how the beast
   carves toward the player when the direct route is blocked.
4. **Idle.** Otherwise wait for the next re-plan.

Following a path: move toward the next cell's centre via
`movement::move_axis` (collision blocks it). When the next cell is a **known
mineable rock**, stop flush against it and switch to `Dig`; accumulate
`progress += dt`; when `progress >= mining_time`, `map.set_tile(target, Dirt)`,
remove it from `known_mineable`, and re-plan. When the next cell is `Dirt`, walk
into it. Advance `next` as cells are reached/opened.

`Charge` simply moves toward the player each frame (collision stops it at a wall
or a not-yet-known rock); if blocked, it re-plans on the next tick.

> Note: a stale `known_mineable` entry (a cell the player already dug to `Dirt`)
> is harmless — `passable` checks the live tile, so a dug cell is just open.

---

## 6. Catch, Lives & Restart

- **Catch = hitbox overlap.** Add a pure helper `movement::hits(a: Vec2, b: Vec2) -> bool`
  (AABB overlap of the two 24×24 hitboxes, i.e. `|a-b| < 2 * HITBOX_HALF` on both
  axes). A grazing touch counts as a catch.
- **Lives** come from `game.toml` `player.starting_lives` (default 3).
- On a catch: decrement lives; if `lives == 0` → **game over**; otherwise
  **regenerate a new random map** and re-spawn (a fresh random seed, ignoring the
  config `seed`).

---

## 7. Level Container (`game/level.rs`, new)

To keep the growing simulation **testable without a GPU**, move the play-state out
of `App` into a `Level` struct (pure — only depends on `Vec2`/map/game types):

```rust
struct Level {
    map: Map,
    map_cfg: MapConfig,      // kept so a restart can regenerate a fresh map
    player: Player,
    beasts: Vec<Beast>,
    lives: u32,
    // config-derived: mining_time, beast speed/mining_time, replan_interval
}

enum LevelEvent { None, Completed, Caught, GameOver }

impl Level {
    fn new(map, map_cfg, player, beasts, lives, ...) -> Level
    fn update(&mut self, input: Input, dt: f32) -> LevelEvent
    fn restart(&mut self, seed: u64)   // regenerate map with `seed`, re-spawn player+beasts
}
```

- `update` runs: player update (movement + mining) → each beast update → catch
  check → exit check. Returns the first significant event (`Caught`,
  `Completed`, `GameOver`).
- `Caught` (with lives remaining) calls `restart(fresh_random_seed())` and
  returns `Caught` so `App` can show a brief "hit" state if desired (M3 keeps it
  simple: instant restart + overlay).
- `Completed` = player reached the exit gap (reuse the existing `player_on_exit`
  logic, moved here or kept as a helper).
- `restart(seed)` regenerates the map via `generation::generate(&map_cfg, seed)`
  and re-spawns the player at the new start and the beasts at the new exit.
  Expose `generation::random_run_seed` (currently private) as a
  `fresh_random_seed()` helper for the catch path.

`App` then holds `Level` + `Assets` + `Camera` + the UI `GameState`, and renders
from `Level`'s data. This is the main structural change of M3.

---

## 8. Spawning Multiple Beasts

- Spawn `map_cfg.beast_count` beasts. All spawn at the **exit gap** (a dirt cell,
  so they are never inside rock) — beast 0 exactly on it, extras stacked with a
  small per-beast pixel offset so they are visible.
- `beast_count = 0` → no beasts (level01 today); `beast_count ≥ 1` → the exit
  guard + extras.
- *"More beasts at other places"* is deferred: we'll add a `beast_spawns` list to
  map TOML in a later milestone. For M3, all beasts spawn at the exit gap (there
  are no other open cells at level start, and the start gap is where the player
  spawns). See §12.

---

## 9. App / UI Changes

- `App` owns `Level` (instead of `map/player/beast` directly) and a
  `GameState { Playing, LevelComplete, GameOver }`.
- Translate `LevelEvent` → `GameState`: `Completed` → `LevelComplete`, `GameOver`
  → `GameOver`, `Caught` → stay `Playing` (level already restarted).
- Draw lives as a simple text counter (e.g. `"Lives: 3"`, top-left) — the real
  HUD/hearts is M5.
- Add a placeholder `GAME OVER` overlay (same style as the existing
  `LEVEL COMPLETE` overlay).
- `App::new` loads config + map, builds the `Level`, and passes `beast_count` +
  multipliers through.

---

## 10. Sample Maps

- `level01.toml` stays `beast_count = 0` (intro); `level02.toml` already has
  `beast_count = 1`.
- For manual M3 testing, either point `App` at `level02.toml` temporarily or set
  `level01.toml`'s `beast_count = 1`. Final counts are tuned in M8.

---

## 11. Tests (unit)

- `game/beast.rs` (AI):
  - Perception adds adjacent `Mineable` cells to `known_mineable`; ignores
    `Unmineable`/`Unbreakable`/`Dirt`.
  - `passable` = `Dirt` always; `Mineable` only when in `known_mineable`;
    `Unmineable`/`Unbreakable`/unknown never.
  - Straight-line: clear row/column → `Charge`; a rock in between → not `Charge`.
  - A\* to player succeeds through open dirt + known mineable; fails when the only
    route is unknown/unmineable.
  - When the player is unreachable, the beast targets the known mineable rock
    closest to the player.
  - Digging: a known mineable rock becomes `Dirt` after `mining_time`; unknown,
    unmineable, and unbreakable rocks are never dug.
- `game/movement.rs`:
  - `hits` is true on overlap/touch, false when separated.
- `game/level.rs`:
  - A catch decrements lives and regenerates a new map (call `restart` with a
    fixed seed and assert the tiles differ from the previous map).
  - Zero lives → `GameOver`.
  - Multiple beasts all update (positions advance).
  - Reaching the exit → `Completed`.
- `config/game.rs`:
  - Parses `[beast]` + `starting_lives`; rejects non-positive/invalid values.

---

## 12. Decisions & Risks (confirm if they differ)

1. **Beast knowledge = physical layout is visible, mineability is local.**
   The beast sees open `Dirt` vs rock everywhere, but only knows a rock is
   *mineable* once adjacent (the growing `known_mineable` list). This is my
   reading of "know all mineable rocks in its adjacent … the list grows" and
   avoids the "player in unknown territory" dead-end.
2. **Restart = new random map.** Catching the player regenerates a fresh map with
   a new random seed (the config `seed` is ignored on restart). The config `seed`
   still makes the *first* load reproducible.
3. **All beasts spawn at the exit gap** for now. We will add a `beast_spawns`
   list to map TOML in a later milestone for "other places" (deferred — no other
   open cells exist at level start, and the start gap is where the player spawns).
4. **Catch = hitbox touch** (24×24 boxes overlap). No grace window; a grazing
   touch counts.
5. **Lives default 3**, from `game.toml`; `max_lives` (and buying lives) is M4.
6. **Fallback dig target = the known mineable rock closest to the player** (not
   nearest to the beast), so the beast carves *toward* the player when it can't
   path directly. Confirmed with the user.

---

## 13. Task List (ordered)

- [x] 1. Extend `config/game.rs` (`starting_lives`, `[beast]`) + `game.toml` + tests.
- [x] 2. Add `movement::hits` (AABB overlap) + tests.
- [x] 3. Rewrite `game/beast.rs`: `BeastState`, knowledge (`known_mineable`),
      perception, decision loop, digging, re-plan; remove the old straight-line
      stub body. + tests.
- [x] 4. Add `game/level.rs` (`Level`, `LevelEvent`, `restart`) + tests.
- [x] 5. Rewire `app.rs` to own a `Level`; add `GameOver` state, lives counter,
      game-over overlay; spawn `beast_count` beasts.
- [x] 6. Point the default run at a map with beasts (or bump `level01`) for manual
      testing.
- [x] 7. Full `cargo test`; desktop run + `--screenshot` (verify beast chases/digs,
      catch→restart, lives decrement, game-over overlay); WASM build/boot check.

---

## 14. Out of Scope (later milestones)

- Sticky Smell / Super Pick consumables (M4) — the "random walk" beast state.
- Gold, score, shop, buying lives (M4).
- Real HUD, level-complete/game-over screens, pause (M5).
- Beast audio (M6).
