# Excavation — Game Requirements & Agent Spec

> This document is written to be followed directly by an AI coding agent.
> Every section states **what** must be built, the **rules**, and **how to verify it**.
> Where a decision was made without a source requirement, it is listed in
> [§18 Assumptions & Open Decisions](#18-assumptions--open-decisions) so it can be
> reviewed and changed cheaply.

---

## 1. Summary

**Excavation** is a small, 2D, top-down arcade game. An explorer found a precious
gem inside an excavation site; the moment he picked it up the ceiling collapsed,
and ancient lizard-like beasts (think small dinosaurs) started hunting him. The
player must dig a path to the surface and escape through the exit while dodging
the beasts.

The twist: **mineable and unmineable rocks look identical**. The player must
probe rocks to discover which ones can be dug, and find a route to the exit
before the beasts catch him.

**Genre:** top-down 2D action / chase / puzzle-escapade.
**First release scope:** 10 levels, a shop between levels, a developer map
editor, menus, save/settings, sounds, score.

---

## 2. Tech Stack & Hard Constraints

- Language: **Rust** (stable).
- Game framework: **[macroquad](https://macroquad.rs/)**.
- Must run on **desktop** and in the **browser (WASM)**.
- Every dependency is added with `cargo add <crate>` (ensures latest versions);
  never hand-edit a version number into `Cargo.toml` unless instructed.
- Graphics: **2D, top-down, pixel art, 16×16 px tiles**.
- Config: **TOML files** (see [§9 Config Files](#9-config-files)).
- Game logic must be **unit-tested**; visuals are verified manually (see
  [§16 Testing Strategy](#16-testing-strategy)).

---

## 3. Glossary

| Term | Meaning |
| --- | --- |
| **Cell / tile** | One grid unit of the map. 16×16 px. |
| **Map** | A rectangular grid of cells, plus doors and per-level tuning. |
| **Mineable rock** | A rock cell the player (or beast) can dig through. |
| **Unmineable rock** | Looks **identical** to a mineable rock, but cannot be dug (except during Super Pick). |
| **Visible wall** | A gray structure cell. **Never** mineable by anyone, not even Super Pick. Purely an obstacle / map decoration. |
| **Excavated cell** | A cell that has been dug out; open floor the player and beasts can walk through. |
| **Border** | The outer ring of the map. Impassable by everyone. |
| **Start door** | Where the player spawns. Sits on the border; exact position is defined per map. |
| **Exit door** | The goal. Sits on the border; exact position is defined per map. |
| **Gold** | Currency found inside some mineable rocks. |
| **Gem** | The story item that starts the chase. **No gameplay effect.** |
| **Beast** | A chasing enemy (lizard/dinosaur-like). |
| **Lives** | Number of times the player can be caught before game over. |

---

## 4. Core Entities & Rules

### 4.1 Player
- Moves **continuously** (pixel-based, not grid-locked) in 8 directions.
- Has a **walk speed** (px/s), upgradeable.
- Mines by standing next to a rock and holding the mine action. Mining is
  **timed**: the player **stops moving** while mining, and the rock breaks when
  the timer completes. Mining a rock turns that cell into an **excavated cell**.
- Base mining time is a tunable value (see [§9](#9-config-files)); reduced by the
  **Mining Speed** upgrade.
- Cannot enter: unmineable rocks, visible walls, or the border.
- Starts each level at the **start door**.
- Has a **lives** count (starts at the configured value, capped at a max).
- Collects **gold** by walking over gold pickups dropped from mined rocks.

### 4.2 Rocks
- Every non-wall, non-door, non-border cell is a rock at level start.
- Each rock is either **mineable** or **unmineable**; this is assigned randomly
  per run (see [§6 Map Generation](#6-map-generation)) and the two kinds are
  **visually indistinguishable**.
- Some mineable rocks contain **gold**. Gold is hidden; mining the rock drops a
  gold pickup on the excavated cell.
- Mining an **unmineable** rock is impossible normally. During **Super Pick**
  (see [§7](#7-upgrades--consumables)) it becomes mineable and instant.
- Visible walls and the border are **never** mineable, even with Super Pick.

### 4.3 Beasts
- Chase the player using the AI in [§5](#5-beast-ai).
- Move continuously; can walk through excavated cells and can **dig through
  mineable rocks** (digging takes time, configurable, slower than the player by
  default).
- Cannot enter unmineable rocks, visible walls, or the border.
- Catching the player (touching) costs the player **one life** and restarts the
  current level. At zero lives → game over.
- Beast **speed** and **dig speed** are per-level tunable.

### 4.4 Gold & the Gem
- **Gold** is the currency spent in the shop between levels.
- **Gem** exists only for story (intro / level-1 start). No mechanics.

### 4.5 Doors & win condition
- Player wins a level by **reaching the exit door**.
- Each map defines its own **start door** and **exit door** positions (both on
  the border); there is no default orientation.
- The exit door is unreachable until the player digs a path to it (the border is
  solid, so the exit must be entered through an excavated route).

### 4.6 Camera
- The camera **follows/scrolls with the player**. The whole map is **not**
  necessarily visible at once; larger maps scroll as the player moves.
- The camera is centered on the player (optionally with a lead offset in the
  movement direction) and clamped to the map bounds.

---

## 5. Beast AI

Beasts are grid-aware but move continuously. They re-plan on a timer (tunable,
e.g. every 250 ms), when they discover a new adjacent cell, and whenever a plan
becomes invalid.

**Knowledge & memory model:**
- A beast always knows the player's **current position**.
- A beast perceives only the cells **adjacent** to its current cell. Moving next
  to a cell reveals that cell's type (mineable rock / unmineable rock / visible
  wall / excavated).
- The beast maintains two growing structures as it walks:
  - `known_map` — every cell it has ever been adjacent to, with its type.
  - `known_mineable` — the subset of `known_map` that are **mineable rocks**
    (its digging candidates). This list grows as the beast moves.
- The beast does **not** know the whole map; **unknown cells** (never adjacent)
  are treated as blocked during planning.

**Decision loop (in priority order):**
1. **Straight-line charge.** If a clear straight path (horizontal or vertical)
   of **excavated cells** to the player exists — no rock, wall, or unmineable
   rock in between — move directly toward the player.
2. **A\* to the player.** Otherwise compute an A\* path on the **known map** to
   the player. Passable = excavated cells + `known_mineable` rocks; blocked =
   visible walls, unmineable rocks, and unknown cells. If a path exists, follow
   it, digging any mineable rocks along the way (each dig takes the beast's dig
   time), then continue.
3. **A\* to nearest known mineable rock.** If no path to the player exists,
   compute A\* to the nearest cell in `known_mineable`, dig it, and re-plan.
   This is how the beast carves toward the player when blocked by unmineable
   rock, and how its known list keeps growing.
4. **Idle.** If none of the above is possible, hold position until the next
   re-plan.

**Sticky Smell effect:** while active, **disable** steps 1–3 and have the beast
walk randomly (still blocked by unmineable rock, walls, and border).

**Implementation note:** A\* operates on the grid; the resulting cell path is
converted to a smooth pixel path (cell centers → interpolated movement). The
`known_map` persists across the whole level (it grows but never shrinks).

---

## 6. Map Generation

- A map is a **grid of cells** plus per-level tuning (see
  [§9.2](#92-map-toml--mapslevelxxxtoml)).
- At startup (or level load), rock cells are assigned **mineable/unmineable
  randomly**, subject to:
  1. The level defines a **fixed number of unmineable rocks** (`unmineable_count`).
  2. A **valid path from the start door to the exit door must always exist**
     (a path of mineable cells, no unmineable rocks or walls blocking it).
- Recommended generator approach (deterministic when a seed is set):
  1. Mark all interior cells mineable.
  2. Carve a guaranteed corridor from start to exit (mark those cells as
     "protected" — never unmineable, never walled).
  3. Randomly flip cells to unmineable until `unmineable_count` is reached,
     never touching protected cells.
  4. Place visible walls (decorative obstacles) without blocking the corridor.
- RNG is seeded per level (optional `seed` in map TOML) so a level can be
  reproduced; default is a random seed each run.
- The exit door must always be reachable via **mineable** cells only.

---

## 7. Upgrades & Consumables

Purchased in the **shop between levels** using gold. Everything below is
**configurable in TOML** (cost, effect magnitude, durations, max levels).

| Name | Type | Effect |
| --- | --- | --- |
| **Walk Speed** | Permanent upgrade (multi-level) | Increases player movement speed. |
| **Mining Speed** | Permanent upgrade (multi-level) | Reduces time to mine one rock. |
| **Lives** | Consumable purchase | Adds lives up to the configured max. |
| **Super Pick** | Consumable (in-level) | For `N` seconds (default 3s) the player mines **any** rock — including unmineable — **instantly**. Visible walls and the border stay unmineable. |
| **Sticky Smell** | Consumable (in-level) | For `M` seconds (default 5s) beast pathfinding is disabled and beasts walk randomly. |

**Rules:**
- Permanent upgrades persist across levels and in saves; each has a **max level**
  and an increasing **cost per level**.
- Consumables are bought in the shop and carried into levels; using one consumes
  it. At most one active consumable effect at a time (a new activation replaces
  the current effect — define this explicitly in code).
- Lives are lost only by being caught; bought lives raise the current count up
  to the max.

---

## 8. Score

- Score is per level, based primarily on **how fast** the player escapes.
- At level end, show the level score and the running total.
- Suggested formula (tunable in TOML):
  `score = max(0, par_time - elapsed) * time_multiplier + gold_collected * gold_multiplier`
- Store the running total in the save file.

---

## 9. Config Files

All game tuning (not player settings) lives in TOML files.

### 9.1 Game TOML — `assets/game.toml`
Example (values are tunable starting points):

```toml
[player]
base_speed = 120.0            # px/s
base_mining_time = 0.8        # seconds per rock
starting_lives = 3
max_lives = 9

[beast]
base_speed = 80.0             # px/s
base_mining_time = 1.6        # seconds per rock (slower than player)
replan_interval = 0.25        # seconds

[upgrades.walk_speed]
max_level = 5
cost_per_level = [50, 100, 200, 400, 800]
speed_increase_per_level = 15.0   # px/s added per level

[upgrades.mining_speed]
max_level = 5
cost_per_level = [50, 100, 200, 400, 800]
mining_time_multiplier_per_level = 0.85   # e.g. 0.8s -> 0.68s -> ...

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
files = ["assets/maps/level01.toml", "assets/maps/level02.toml", /* ... level10.toml */]
```

### 9.2 Map TOML — `assets/maps/levelXX.toml`
Example:

```toml
width = 40
height = 22

unmineable_count = 40        # fixed number of unmineable rocks
gold_count = 8               # how many mineable rocks contain gold
beast_count = 1
beast_speed_multiplier = 1.0
beast_mining_time_multiplier = 1.0

# Door positions are arbitrary and defined per map (must be on the border).
start_door = { x = 20, y = 21 }
exit_door  = { x = 8,  y = 0  }

# Optional, for reproducibility; omit for random each run.
seed = 12345

# Optional decorative visible walls; cells are (x, y).
# If omitted, none are placed.
visible_walls = [[5, 5], [6, 5], [7, 5]]
```

---

## 10. Menus & UI Flow

| Screen | Purpose / behavior |
| --- | --- |
| **Main menu** | Play, Continue (if save exists), Map Editor, Settings, Quit. |
| **Level select** | Shows the 10 levels; locked until reached. |
| **Shop (between levels)** | Spend gold on Walk Speed, Mining Speed, Lives, Super Pick, Sticky Smell. Show current gold, owned upgrade levels, lives, consumable count. |
| **In-level HUD** | Lives (hearts), active consumable timer, current level, gold collected this level, elapsed time / score. |
| **Pause menu** | Resume, Restart level, Settings, Save, Quit to menu. |
| **Level complete** | Show level score, running total, gold earned; button to go to shop / next level. |
| **Game over** | Shown at 0 lives; options to restart run or return to menu. |
| **Victory** | After level 10; show final score; return to menu. |

### Settings
- Music volume, SFX volume, fullscreen toggle, (optional) key/button remap.
- Settings persist in the save file.

---

## 11. Save / Load

- Saves **full progress**: level reached, gold, owned upgrade levels, current
  lives, consumable counts, running score, and settings.
- Desktop: save to a file in the user data directory (via macroquad/standard
  means).
- Web: save to `localStorage` (keyed under a fixed app key).
- "Continue" resumes from the save; if none exists it behaves like "Play".

---

## 12. Map Editor (developer tool)

- Launched via a CLI flag (e.g. `--editor`) in the same binary.
- Lets a developer create/edit the map TOML files:
  - Set width/height, unmineable count, gold count, beast count and multipliers.
  - Place start/exit doors and visible walls on a visual grid.
  - Save/load `assets/maps/levelXX.toml`.
- Must validate on save: start and exit doors present, dimensions valid.

---

## 13. Controls (defaults)

| Action | Keyboard | Gamepad |
| --- | --- | --- |
| Move | WASD / Arrow keys | Left stick / D-pad |
| Mine (hold) | Space (or E) | A / X (hold) |
| Use consumable 1 (Super Pick) | 1 (or Q) | LB |
| Use consumable 2 (Sticky Smell) | 2 (or E) | RB |
| Pause | Esc | Start |

(Keys/buttons are remappable if the optional remap feature is implemented.)

---

## 14. Audio

See the audio portion of the [asset list](#152-audio).

---

## 15. Asset List (Sprites & Audio)

All sprites are **16×16 px pixel art**. Reuse a single rock sprite for both
mineable and unmineable rocks **by design** (they must look identical).

### 15.1 Sprites & Tiles

| Asset | Type | Description / notes |
| --- | --- | --- |
| Player — idle | Sprite sheet | 1–4 frames; front/side as needed for top-down. |
| Player — walk | Sprite sheet | 4-directional walk animation (or 8-dir if cheap). |
| Player — mining | Sprite | Mining pose (pick swinging) while digging. |
| Beast — idle | Sprite sheet | Lizard/dino; 2–4 frames. |
| Beast — chase | Sprite sheet | Walk/run animation. |
| Beast — digging | Sprite | Digging pose (optional if beasts use same dig fx). |
| Rock | Tile | **Single** sprite for both mineable and unmineable rocks (identical). |
| Excavated floor | Tile | Dug-out dirt/stone floor. |
| Visible wall | Tile | Gray structure (the "decorative" wall). |
| Border | Tile | Outer impassable frame. |
| Start door | Tile | Underground entrance marker. |
| Exit door | Tile | Surface exit marker (optionally open/closed states). |
| Gold pickup | Sprite | Small gold nugget (1–4 sparkle frames). |
| Gem | Sprite | The story gem (intro / start-of-run). |
| Super Pick icon | Sprite | Consumable icon (e.g. a glowing pickaxe). |
| Sticky Smell icon | Sprite | Consumable icon (e.g. a smell cloud / jar). |
| Heart | Sprite | Life icon for HUD (full/empty states). |
| Walk Speed icon | Sprite | Shop icon (e.g. boots). |
| Mining Speed icon | Sprite | Shop icon (e.g. faster pickaxe). |
| Lives icon | Sprite | Shop icon (heart with "+"). |
| Particles | Sprite sheet | Dig dust / rock-break debris (few frames). |
| UI — button | Sprite | 9-slice button (normal/hover/pressed). |
| UI — panel | Sprite | 9-slice panel/background. |
| UI — slider | Sprite | Volume/fullscreen slider parts. |
| Cursor/selector | Sprite | Menu cursor (keyboard/gamepad selection). |
| Title/logo | Sprite | Game title graphic for the main menu. |
| Menu background | Sprite | Main menu backdrop (or generated from tiles). |

### 15.2 Audio

| Asset | Type | Notes |
| --- | --- | --- |
| Music — menu | Loop | Main menu / shop theme. |
| Music — level | Loop | Gameplay theme (optionally one per world). |
| Music — chase | Loop (optional) | Tense variant when a beast is near. |
| SFX — dig | One-shot | Mining loop / repeated digging. |
| SFX — rock break | One-shot | Rock shattering. |
| SFX — gold pickup | One-shot | Coin/nugget chime. |
| SFX — gem pickup | One-shot | Story sting (intro). |
| SFX — super pick | One-shot | Power-up activation. |
| SFX — sticky smell | One-shot | Power-up activation. |
| SFX — beast growl | One-shot | Beast nearby / aggro. |
| SFX — beast dig | One-shot | Beast digging (reuse rock-break if wanted). |
| SFX — footsteps | One-shot | Player (and optionally beast) steps. |
| SFX — caught | One-shot | Player hit / death. |
| SFX — level complete | One-shot | Exit reached fanfare. |
| SFX — game over | One-shot | Defeat sting. |
| SFX — purchase | One-shot | Shop buy. |
| SFX — UI click | One-shot | Menu navigation. |

---

## 16. Testing Strategy

- **Unit-test all game logic** (pure functions, no rendering):
  - Map generation: correct unmineable count; a valid mineable-only path from
    start to exit always exists.
  - Mining rules: mineable vs unmineable vs visible wall vs Super Pick behavior.
  - Beast AI: straight-line detection, A\* to player, A\* to nearest known
    mineable rock, growing `known_mineable` list, Sticky Smell random movement.
  - Upgrades/consumables: cost math, level caps, effect durations.
  - Score calculation.
  - Save/load round-trip (serialize → deserialize → equality).
  - Config loading (TOML) with sane fallbacks on missing/invalid fields.
- **Visual things are verified manually** by running the game (desktop and web)
  and checking: rendering, animation, menus, audio, feel, and the map editor.

---

## 17. Definition of Done (per feature checklist)

- [ ] Project boots on desktop **and** web (`cargo run`, WASM build serves in browser).
- [ ] 10 maps load from TOML; each guarantees a valid path and honors
      `unmineable_count`, `gold_count`, `beast_count`, and multipliers.
- [ ] Player mines (timed, stops movement); unmineable rocks are identical to
      mineable until probed; Super Pick mines anything instantly except walls.
- [ ] Beasts chase using straight-line → A\* → nearest-known-mineable fallback,
      build a growing known-mineable list from adjacent cells, can dig mineable
      rocks, and Sticky Smell disables their pathfinding.
- [ ] Catch → lose a life → restart level; 0 lives → game over.
- [ ] Shop (Walk Speed, Mining Speed, Lives, Super Pick, Sticky Smell) works and
      is fully TOML-configured.
- [ ] Gold hidden in mineable rocks, dropped, collected, spent.
- [ ] Score shown at level end; running total persisted.
- [ ] Menus (main, level select, shop, pause, level complete, game over, victory)
      all reachable and correct.
- [ ] Save/load full progress (desktop file, web localStorage).
- [ ] Settings (volumes, fullscreen) apply and persist.
- [ ] Map editor can create/edit/save valid map TOML files.
- [ ] Sounds and music play for the events in [§15.2](#152-audio).
- [ ] All logic tests pass; visuals manually validated on both platforms.

---

## 18. Assumptions & Open Decisions

These were decided during spec-writing; confirm or adjust if they don't match
your intent.

1. **Camera follows the player and scrolls**; the whole map is not necessarily
   shown. v1 has no zoom or manual camera controls — it just centers on (or
   leads) the player.
2. **Virtual resolution 1280×720 (16:9)**; 16×16 tiles scaled to fit. Maps may
   be larger than the screen (the camera scrolls); the suggested default is
   ~40×22 cells.
3. **Super Pick = instant mining of any rock** (not just "allowed to mine
   unmineable at normal speed"). Confirm if it should instead keep normal mining
   time.
4. **Consumable effects don't stack**; a new activation replaces the current one.
5. **Beasts always know the player's position** (they "smell" him) — this drives
   the straight-line charge and A\* targeting. Only the *mineability* knowledge
   is local and incremental (see [§5](#5-beast-ai)).
6. **Gold has no visual tell** inside a rock before mining (keeps it hidden, like
   the mineable/unmineable distinction). Confirm if gold rocks should sparkle.
7. **Lives are restored/purchased up to a cap** (default max 9); they are not a
   permanent "max lives" upgrade. Confirm if you'd rather buy a higher max-lives
   cap instead.
8. **Default numeric values** (speeds, mining times, costs, durations) in
   [§9](#9-config-files) are starting points; tune them freely in TOML.

---

## 19. Non-Goals (v1)

- No networked multiplayer.
- No procedural level *layout* generation (only rock mineability is randomized;
  layout/doors/walls come from the map TOML or editor).
- No animated cutscenes (a static intro screen / text is enough for the gem story).
- No mobile/touch controls (keyboard + gamepad only in v1).
