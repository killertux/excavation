# Excavation — Implementation Plan (High Level)

> This is the **high-level** plan to implement everything in `REQUIREMENTS.md`.
> It is divided into **milestones**. Each milestone ends with a **working,
> playable binary** on desktop **and** web.
>
> Before implementing a milestone, we write a separate detailed plan for it
> (e.g. `PLAN-M1.md`). This document only sets the ordering, scope, and exit
> criteria — it intentionally does **not** contain task-level detail.

---

## 1. Inputs & References

- **Spec:** [`REQUIREMENTS.md`](./REQUIREMENTS.md) — the source of truth.
- **Assets:** already committed under [`assets/`](./assets/) (images + audio).
  The exact path and frame/tile order for every asset is in **§15.3** of the
  requirements. Image sheets are high-resolution and must be **sliced and
  scaled to 16×16 px** at load time.
- **Definition of Done:** `REQUIREMENTS.md` §17 — the final acceptance checklist.

---

## 2. Architecture & Tech Decisions (high level)

These are the broad decisions; the exact details are finalized in each
milestone's detailed plan.

- **Language/framework:** Rust + [`macroquad`](https://macroquad.rs/), targeting
  desktop and WASM. All dependencies are added with `cargo add <crate>`.
- **Config & save:**
  - `serde` (+ derive) + `toml` for `game.toml` and `assets/maps/*.toml`.
  - Save file serialized with `serde` (format decided in the save milestone,
    e.g. JSON) — file on desktop, `localStorage` on web.
- **Pathfinding (beast A\*):** implemented in-house as a **pure, unit-testable**
  function (a small grid A\*). No heavy dependency required.
- **Testing:** all game logic lives in pure modules (no rendering) and is
  unit-tested. Rendering, animation, audio, and "feel" are verified manually.
- **Proposed module layout** (finalized in M1):
  ```
  src/
    main.rs            entry point; desktop/web glue; --editor switch
    app.rs             app state machine (menu/level/shop/editor/…)
    assets.rs          asset loading + atlas slicing/scaling
    config/            game.toml & map toml loaders (game.rs, map.rs)
    game/              level, map, map generation, player, beast,
                       pathfinding, mining, gold, upgrades, consumables, score
    ui/                hud, menu, shop, settings
    save.rs            save/load + settings persistence
    audio.rs           music + sfx
    editor.rs          map editor
  ```

---

## 3. Milestone Overview

| # | Milestone | Working binary = you can… |
| --- | --- | --- |
| **M1** | Foundation, asset pipeline, movement | Boot on desktop+web, load/slice assets, walk the player around a rendered map with a scrolling camera. |
| **M2** | Mining, map generation, level flow | Dig a path through rocks and reach the exit to complete a level; maps generate with a guaranteed path. |
| **M3** | Beasts & lives | Play the full chase loop — beasts hunt you, catch costs a life, 0 lives = game over. |
| **M4** | Gold, score, shop, upgrades | Mine gold, see your score, buy Walk Speed / Mining Speed / Lives / Super Pick / Sticky Smell between levels. |
| **M5** | Menus, HUD, save/load, settings | Navigate the full game, pause, save/load progress, adjust volume/fullscreen. |
| **M6** | Audio | Play the game with music and all sound effects. |
| **M7** | Map editor | Create and edit map TOML files via `--editor`. |
| **M8** | Polish, 10 levels, balance, release | Play all 10 balanced levels end-to-end; full test suite green on both platforms. |

---

## 4. Milestones

### M1 — Foundation, asset pipeline, player movement

**Goal:** a booting macroquad project (desktop + web) that loads and slices the
asset sheets, renders a level grid, and lets the player move with a scrolling
camera.

**Scope:**
- `cargo init`; add `macroquad`; wire desktop run + WASM build/serve.
- Module skeleton (see §2).
- Asset loader: load the PNG sheets and slice/scale frames and tiles to 16×16
  per `REQUIREMENTS.md` §15.3 (character sheets, terrain atlas, pickups atlas,
  UI atlas, particles, background, title).
- `Game` struct with a fixed update/render loop and a basic app-state enum.
- Load a single map (initially a hand-written placeholder) and render its tiles
  (rock, floor, wall, border, doors) using the terrain atlas.
- Render the player sprite; continuous 8-direction movement with walk speed.
- Camera that follows/scrolls with the player, clamped to map bounds (§4.6).
- Input: keyboard (WASD/arrows) and gamepad movement.

**Deps added:** `macroquad`.

**Tests:** asset slicing produces expected frame/tile counts and 16×16 sizes.

**Working binary:** window opens on desktop and in the browser; the player
sprite walks around a rendered map and the camera follows. (No mining yet.)

---

### M2 — Mining, map generation, level flow

**Goal:** the player can mine rocks and complete a level by reaching the exit,
with maps generated from TOML and always having a valid path.

**Scope:**
- Map data structures (grid of cell types) and map TOML loader (§9.2).
- Map generation (§6): random mineable/unmineable assignment with a fixed
  `unmineable_count`, a guaranteed mineable path from start to exit, optional
  `visible_walls`, optional `seed` for reproducibility.
- Mining mechanic (§4.1–4.2): stand adjacent + hold to mine; timed; player
  stops while mining; rock becomes an excavated cell. Unmineable rocks look
  identical and cannot be mined.
- Doors: spawn at start door; reaching the exit door completes the level.
- Level lifecycle: load → play → "level complete" placeholder (full screens in M5).
- Author 1–2 sample map TOMLs to play with.

**Deps added:** `serde`, `toml`.

**Tests:** generation guarantees (unmineable count, always-a-valid-path), mining
rules (mineable vs unmineable vs wall), config parsing with fallbacks.

**Working binary:** you can mine a path from the start door to the exit door;
the level completes when you reach the exit. Unmineable rocks block you and look
exactly like mineable ones.

---

### M3 — Beasts & lives

**Goal:** beasts chase the player using the spec'd AI; being caught costs a life
and restarts the level; zero lives ends the game.

**Scope:**
- Beast entity + movement (continuous) and per-level speed/dig-speed multipliers.
- Beast AI (§5): knowledge model (`known_map`, growing `known_mineable` list from
  adjacent cells) and decision loop (straight-line charge → A\* to player →
  A\* to nearest known mineable → idle).
- In-house grid A\* (pure function).
- Beasts can dig mineable rocks (with beast dig time).
- Catch detection → lose a life → restart level; 0 lives → game-over placeholder.
- Multiple beasts per level (`beast_count`).

**Deps added:** none (A\* in-house).

**Tests:** straight-line detection, A\* to player, A\* to nearest known
mineable, growing `known_mineable` list, catch/life/restart/game-over logic.

**Working binary:** a full playable chase loop — beasts hunt and dig toward you;
getting caught loses a life and restarts the level; running out of lives ends
the game.

---

### M4 — Gold, score, shop, upgrades & consumables

**Goal:** gold collection, per-level scoring, and a shop between levels with all
five purchases, fully driven by `game.toml`.

**Scope:**
- Gold: hide gold in random mineable rocks (`gold_count`); mining drops a gold
  pickup; walking over it collects it.
- Score (§8): per-level speed-based score + running total; shown at level end.
- Shop screen (between levels): Walk Speed, Mining Speed, Lives, Super Pick,
  Sticky Smell — all costs/effects/durations/max-levels from `game.toml` (§7, §9.1).
- Upgrade application: walk speed, mining speed, lives cap/refill.
- Consumables in-level: Super Pick (instant mine anything except walls, timed)
  and Sticky Smell (disable beast pathfinding, timed); one active at a time.

**Deps added:** none expected.

**Tests:** gold placement/collection, score formula, upgrade cost/level-cap math,
consumable durations, "one active effect" rule.

**Working binary:** mine gold, finish a level, see your score, spend gold in the
shop, and see upgrades/consumables take effect in the next level.

---

### M5 — Menus, HUD, save/load, settings

**Goal:** the full UI flow and persistence described in §10–§11.

**Scope:**
- Screens: main menu, level select (locked until reached), shop, pause, level
  complete, game over, victory.
- In-level HUD: lives, active consumable timer, level, gold this level, time/score.
- Save/load full progress (§11): level reached, gold, upgrade levels, lives,
  consumables, running score, settings. Desktop → file; web → `localStorage`.
- Settings: music/SFX volume, fullscreen (and optional remap). Persist in save.
- `Continue` on the main menu when a save exists.

**Deps added:** `serde_json` (or chosen save format crate).

**Tests:** save/load round-trip, settings persistence, menu state transitions
(logic-only), level-unlock logic.

**Working binary:** the complete game with all screens, persistence across
sessions, and settings.

---

### M6 — Audio

**Goal:** wire in all music loops and SFX events.

**Scope:**
- Audio manager: load the committed WAVs from `assets/audio/`.
- Music: menu / level / chase loops with appropriate switching.
- SFX: all events in §15.2 (dig, rock break, gold/gem pickup, super pick,
  sticky smell, beast growl/dig, footsteps, caught, level complete, game over,
  purchase, UI click).
- Respect settings volume (music vs SFX).

**Deps added:** as needed for audio playback (macroquad audio).

**Tests:** none (manual; verify each event plays and volumes apply).

**Working binary:** the complete game with music and sound effects, volume
controlled from Settings.

---

### M7 — Map editor

**Goal:** a developer tool to create/edit map TOML files (§12).

**Scope:**
- `--editor` launch mode in the same binary.
- Visual grid editor: set width/height, place start/exit doors and visible
  walls, set `unmineable_count`, `gold_count`, `beast_count`, multipliers, `seed`.
- Save/load `assets/maps/levelXX.toml`; validation on save (doors present,
  dimensions valid).

**Deps added:** none expected.

**Tests:** editor save/load validation logic.

**Working binary:** launching with `--editor` opens the editor; you can create,
edit, validate, and save map files that the game can load.

---

### M8 — Polish, 10 levels, balance, release

**Goal:** ship-ready build with all 10 levels authored and balanced, full test
coverage, and both platforms validated.

**Scope:**
- Author + balance the 10 level TOMLs (increasing unmineable count, beast speed,
  and beast count per §1 of the requirements).
- Tune `game.toml` numbers (speeds, costs, durations) for good feel.
- Complete the Definition-of-Done checklist (`REQUIREMENTS.md` §17).
- Full unit-test suite green; manual validation pass on desktop and web.
- Story/beats polish: gem intro, victory, game-over screens.

**Deps added:** none expected.

**Tests:** final full suite.

**Working binary:** the complete, polished game — all 10 levels playable
end-to-end on desktop and web.

---

## 5. Cross-Cutting Notes

- **Levels:** placeholder maps exist from M2 onward; the final 10 are authored in
  M8. The map editor (M7) can be used to build them.
- **Balancing:** numeric defaults in `game.toml` are tunable starting points;
  only M8 is expected to make them feel final.
- **Platform parity:** every milestone's "working binary" must run on **both**
  desktop and web. Only save/load is allowed to differ (file vs `localStorage`).
- **Definition of Done:** `REQUIREMENTS.md` §17 is the release gate for M8.
