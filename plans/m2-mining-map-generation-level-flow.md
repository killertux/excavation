# M2 — Mining, Map Generation, Level Flow (Detailed Plan)

> Scope: milestone **M2** from [`PLAN.md`](../PLAN.md).
> **Working binary =** you can mine a path from the start door to the exit door;
> the level completes when you reach the exit. Maps are generated from TOML with
> a fixed number of unmineable rocks and a guaranteed valid path. Unmineable
> rocks block you and look exactly like mineable ones.

---

## 1. Outcome (acceptance checklist)

- [ ] `assets/game.toml` is loaded; player speed and mining time come from it.
- [ ] `assets/maps/level01.toml` (and one more sample) load and generate a valid map.
- [ ] Generated maps have exactly `unmineable_count` unmineable rocks, correct
      border/doors/visible walls, and a guaranteed mineable path start→exit.
- [ ] The same `seed` reproduces the same map; a missing seed randomizes each run.
- [ ] The player can mine a mineable rock (hold the mine key while adjacent);
      mining is timed, stops movement, and turns the rock into an excavated cell.
- [ ] Unmineable rocks look identical and cannot be mined; walls/border never.
- [ ] Reaching the exit door ends the level (placeholder "LEVEL COMPLETE" state).
- [ ] All new unit tests pass (generation, mining, pathfinding, config).

---

## 2. Prerequisites

- M1 is done and merged (asset pipeline, movement, camera, input).
- No beasts, gold, score, shop, or menus in this milestone (those are M3–M5).

---

## 3. Dependencies

```
cargo add serde --features derive
cargo add toml
cargo add rand
```

- `serde`/`toml` — load `game.toml` and map TOMLs.
- `rand` — seeded, deterministic map generation (`StdRng` + `SeedableRng`).

---

## 4. Module Changes

```
src/
  config/
    mod.rs          loaders + a shared ConfigError
    game.rs         GameConfig (game.toml)
    map.rs          MapConfig + Pos (map toml)
  game/
    mod.rs
    map.rs          Tile split (Mineable/Unmineable) + generation entry point
    generation.rs   NEW: generate(config, seed) -> Map  (pure, seeded)
    pathfinding.rs  NEW: grid A* + has_path (pure)
    mining.rs       NEW: target selection + mining state (pure)
    player.rs       add facing + mining state; movement gated by mining
  app.rs            state machine (Playing / LevelComplete), load config + map
  input.rs          Input struct { move: Vec2, mine: bool }  (was move_intent())
```

`game.toml` and `assets/maps/*.toml` are new committed config files (see §6).

---

## 5. Config Files

### 5.1 `assets/game.toml` (new)

```toml
[player]
base_speed = 120.0        # px/s (replaces the M1 PLAYER_SPEED const)
base_mining_time = 0.8    # seconds to mine one rock
```

`config/game.rs` → `GameConfig { player: Player { base_speed, base_mining_time } }`.
`base_mining_time` must be `> 0` (validate; error otherwise).

### 5.2 `assets/maps/levelXX.toml` (new)

```toml
width = 30
height = 20

unmineable_count = 40          # fixed number of unmineable rocks
gold_count = 8                 # parsed now, used in M4
beast_count = 1                # parsed now, used in M3
beast_speed_multiplier = 1.0   # parsed now, used in M3
beast_mining_time_multiplier = 1.0

start_door = { x = 15, y = 19 }   # must be on the border
exit_door  = { x = 5,  y = 0  }   # must be on the border

seed = 12345                   # optional; omit to randomize each run

visible_walls = [[8, 5], [9, 5], [10, 5]]
```

`config/map.rs` → `MapConfig` with `#[serde(default)]` on all optional fields so
the schema is stable; M2 only *uses* `width/height/unmineable_count/doors/seed/visible_walls`
(the `gold_*`/`beast_*` fields are parsed but unused until M3/M4).

Validation (fail with a clear `ConfigError`):
- `width`, `height` > 0.
- `start_door`/`exit_door` present and **on the border**.
- `unmineable_count` ≤ available interior cells (so a valid path is possible).

---

## 6. Map Model Changes (`game/map.rs`)

Split `Tile::Rock` so mineability is data, not just looks:

```rust
enum Tile {
    Mineable,    // diggable rock (renders TileId::Rock)
    Unmineable,  // looks identical, blocks, not diggable
    Excavated,
    Wall,
    Border,
    StartDoor,
    ExitDoor,
}
```

- `solid()` → `Mineable | Unmineable | Wall | Border`.
- `mineable()` → `Mineable` only.
- `tile_id()` → both `Mineable` and `Unmineable` map to `TileId::Rock` (identical sprite).
- `Map::count(Tile)` helper (count a tile kind; used by generation/tests).

The M1 `placeholder_map()` and its tests are **removed** — generation from TOML
replaces it. (Keep `Map`'s accessor/`set_tile`/`is_solid` as-is.)

---

## 7. Map Generation (`game/generation.rs`)

`generate(config: &MapConfig, seed: u64) -> Result<Map, GenError>` — pure and
seeded so it is deterministic and testable.

Algorithm (matches `REQUIREMENTS.md` §6):

1. Allocate `width × height`, fill the ring with `Border` and the interior with
   `Mineable`.
2. Place `StartDoor` / `ExitDoor` at the configured border cells.
3. Place `visible_walls` at the configured cells (overwriting interior mineable).
4. **Guaranteed corridor:** run grid A\* from start to exit treating
   `Mineable | Excavated | doors` as passable and everything else blocked. If no
   path exists → return an error (invalid map config). Mark the corridor cells as
   `protected`.
5. **Randomize unmineable:** collect all interior cells that are `Mineable` and
   **not** protected and not a door; shuffle them with the seeded RNG; flip the
   first `unmineable_count` to `Unmineable`. If fewer candidates than
   `unmineable_count`, return an error.
6. Return the map.

**Seed:** `MapConfig.seed` when present; otherwise `rand::random::<u64>()` each run
(so "random at each execution" holds, and any run can be reproduced by setting the
seed). Expose the resolved seed so tests and logs can report it.

**Post-condition (also asserted in tests):** a mineable-only path from start to
exit always exists. Generation guarantees it by construction, and `has_path`
is used to verify.

---

## 8. Pathfinding (`game/pathfinding.rs`)

A small, pure grid A\* used by generation (corridor + validation) and later by
the beast AI (M3). Keep it generic so M3 reuses it:

```rust
fn astar(start: (i32,i32), goal: (i32,i32),
         is_passable: impl Fn(i32,i32) -> bool) -> Option<Vec<(i32,i32)>>
fn has_path(start: (i32,i32), goal: (i32,i32),
            is_passable: impl Fn(i32,i32) -> bool) -> bool
```

- 4-neighbor movement (grid cardinal directions).
- Deterministic tie-breaking (e.g. fixed neighbor order) so seeded generation is
  reproducible.
- Unit-test: finds a path around an obstacle; returns `None` when fully blocked.

---

## 9. Mining Mechanics (`game/mining.rs` + `game/player.rs`)

**Facing.** The player gets a `facing: Vec2` (one of the 8 directions), updated
from the last non-zero move intent; default `(0, 1)` (down) at spawn. It is
stable while mining because movement is ignored during mining.

**Target selection (pure):**
```
mine_target(pos, facing, map) -> Option<(i32,i32)>
```
- Compute the player's current cell `c = (floor(pos.x / 16), floor(pos.y / 16))`.
- Target cell `t = c + round(facing)` (nearest of the 8 neighbors).
- Return `Some(t)` only if `map.tile(t) == Mineable`; else `None`.

**Mining state (in `Player`, or a `Mining` struct owned by `App`):**
```
struct Mining { target: (i32, i32), progress: f32 }
```

**Update rule (each frame):**
- If `input.mine` is held **and** `mine_target` returns `Some(t)`:
  - If already mining `t`, advance `progress += dt`.
  - Else start mining `t` with `progress = 0`.
  - While mining, **ignore move intent** (player does not move).
  - When `progress >= base_mining_time`: `map.set_tile(t, Excavated)` and clear the
    mining state.
- Else (not mining): clear mining state and move normally.
- Holding mine while facing an `Unmineable`/`Wall`/`Border`/`Excavated` cell does
  **not** engage mining (nothing to dig), so the player moves normally — this is
  the intended "probe": face a rock and hold mine; mineable rocks dig, unmineable
  ones do nothing.

The `Mining` frame (`PlayerAnim::Mining`) is shown while mining (this also
resolves the M1 dead-code warning for that variant).

---

## 10. Input (`input.rs`)

Change `move_intent() -> Vec2` into:

```rust
struct Input { move_: Vec2, mine: bool }
fn collect() -> Input
```

- Move: WASD/arrows (unchanged).
- Mine: hold `Space` or `E`.
- Gamepad remains deferred (macroquad 0.4.16 has no gamepad API — see the M1
  note in `input.rs`); keep `Input` shaped so a gamepad source can be added later.

---

## 11. Level Flow & App State (`app.rs`)

- `App::new()` loads `assets/game.toml` + `assets/maps/level01.toml`, generates
  the map, spawns the player at the start door.
- Add `enum GameState { Playing, LevelComplete }` on `App`.
- Each `update()`:
  1. Collect input.
  2. Update player (movement + mining) against the map.
  3. If the player's current cell equals the exit door cell → `LevelComplete`.
- In `LevelComplete`, stop gameplay updates and draw a simple placeholder overlay
  (`"LEVEL COMPLETE"` text). The real level-complete screen is M5.
- Player speed now comes from `GameConfig.player.base_speed` (remove the
  `PLAYER_SPEED` const and its TODO).

---

## 12. Rendering

- Tiles render as in M1 (mineable and unmineable share the same `TileId::Rock`
  texture — no visual difference).
- Draw the `PlayerAnim::Mining` frame while mining.
- `LEVEL COMPLETE` overlay via `draw_text` when the state is `LevelComplete`.
- No other visual changes.

---

## 13. Sample Maps (commit these)

- `assets/game.toml` (as in §5.1).
- `assets/maps/level01.toml` — a small easy map (e.g. 30×20, few unmineable, a
  couple of visible walls).
- `assets/maps/level02.toml` — slightly harder (more unmineable) to prove the
  loader handles multiple files.

(These are placeholders; the final 10 are authored/balanced in M8.)

---

## 14. Tests (unit)

- `config/`:
  - `game.toml` and a map TOML parse into the expected structs.
  - Missing optional fields use defaults; invalid values (non-positive mining
    time, doors off-border, `unmineable_count` too large) error.
- `generation`:
  - Correct unmineable count, border ring, doors, and visible walls.
  - `has_path(start, exit)` is always true (assert across many seeds).
  - Same seed → identical map; different seeds → (almost always) different.
  - Visible wall placed on the corridor path still yields a valid map (walls are
    placed before the corridor is carved).
- `pathfinding`:
  - Finds a path around a wall; returns `None` when fully blocked.
- `mining`:
  - `mine_target` returns the facing mineable cell, and `None` for unmineable,
    wall, border, excavated, and out-of-bounds.
  - Mining a mineable rock for `base_mining_time` turns it `Excavated`.
  - Mining ignores movement; releasing the key or changing target resets progress.
  - Unmineable/wall/border never change.
- `app`/level flow:
  - Player cell == exit → `LevelComplete`.

---

## 15. Task List (ordered)

- [ ] 1. Add `serde` (derive), `toml`, `rand` via `cargo add`.
- [ ] 2. Create `config/` (`game.rs`, `map.rs`) + `ConfigError`; write `game.toml`.
- [ ] 3. Split `Tile` into `Mineable`/`Unmineable`; update `solid/tile_id`; add `count`.
- [ ] 4. Implement `pathfinding.rs` (A\* + `has_path`) + tests.
- [ ] 5. Implement `generation.rs` (corridor + seeded unmineable flip) + tests.
- [ ] 6. Write `assets/maps/level01.toml` and `level02.toml`.
- [ ] 7. Implement `mining.rs` (`mine_target`) + mining state + tests.
- [ ] 8. Extend `player.rs` (facing, mining state, gate movement) + tests.
- [ ] 9. Extend `input.rs` (`Input { move_, mine }`).
- [ ] 10. Wire `app.rs`: load config + map, `GameState`, exit detection, overlay.
- [ ] 11. Remove `placeholder_map()` and update/remove its tests.
- [ ] 12. Full `cargo test`; desktop run + `--screenshot` (verify mined rock →
       floor, `LEVEL COMPLETE` overlay when reaching exit); WASM build/boot check.

---

## 16. Risks & Decisions

1. **Facing-based mining** (vs "nearest adjacent rock") is a design choice for
   determinism and later directional visuals. Confirm if you'd rather mine
   automatically toward the nearest adjacent rock instead.
2. **Mining engages only on a mineable target.** Holding mine while facing an
   unmineable rock does nothing (the probe behavior) and does **not** lock the
   player in place. This is the intended "discover which rocks are mineable" loop.
3. **`rand` + seeded `StdRng`** chosen over a hand-rolled PRNG for correctness and
   reproducibility; it's a standard, well-tested crate.
4. **Deterministic corridor** (fixed A\* tie-breaking) with randomness only in the
   unmineable placement. This keeps maps reproducible while still feeling random.
   A randomized corridor is a possible later enhancement, not needed for M2.
5. **Doors must be on the border** (validated). Config with an off-border door is
   rejected rather than silently moved.
6. **`LevelComplete` is a placeholder** (text overlay only); the real screen with
   score/gold is M5.
