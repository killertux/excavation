# Excavation

A small real-time mining / escape game built with [macroquad](https://macroquad.rs)
in Rust, targeting **desktop** (native window) and **web** (WASM).

You play a miner who dug too deep and found a priceless gem. The moment you lift
it, the ceiling gives way and the ancient lizard-beasts below wake. **Dig your way
to the surface before they reach you** — and remember, not every rock will break.

The game is developed in milestones (see `plans/`), and is currently feature-complete
across the full 10-level story. `REQUIREMENTS.md` §17 is the Definition of Done.

---

## Getting started

### Desktop (native)

Build and run the game:

```sh
cargo run
```

_Note: this project sets a dedicated cargo home in `CARGO_HOME`; if your toolchain
is set up differently, simply run `cargo run` from the repo root as usual._

### Web (WASM)

The `build-web.sh` script builds the wasm binary, stages it alongside a vendored
macroquad JS loader, and copies `assets/` into `web/`:

```sh
./build-web.sh            # release (default)
./build-web.sh debug      # debug build
```

Then serve the `web/` directory with any static server:

```sh
python3 -m http.server -d web
# open http://localhost:8000
```

Browser audio autoplay requires a first user input (click/keypress) before sounds
play — this is expected browser behavior, not a bug.

---

## Controls

### In-game

| Action | Keys |
| --- | --- |
| Move | `WASD` or arrow keys |
| Mine a rock (hold while facing it) | walk into it |
| Super Pick (consumable, instant mine) | `1` or `Q` |
| Sticky Smell (consumable, disables beast pathfinding) | `2` or `E` |
| Pause | `Esc` |

Gameplay is keyboard-only; **mouse is supported throughout the GUI** (menus,
shop, and the text overlays). In the menus and shop, hovering a row highlights
it and clicking it activates it (same as pressing Enter). The settings sliders
can be set by clicking anywhere along a track.

### Menus

| Action | Keys | Mouse |
| --- | --- | --- |
| Move selection | `W/Up`, `S/Down` | hover a row |
| Activate / continue | `Enter` or `Space` | click a row |
| Back / resume | `Esc` | click the Back row |
| Settings: adjust volume | `A/Left`, `D/Right` | click anywhere on the slider track |
| Settings: toggle fullscreen | `Enter` on the fullscreen row | click the fullscreen button |

### Shop

| Action | Keys | Mouse |
| --- | --- | --- |
| Move selection | `W/Up`, `S/Down` | hover a row |
| Buy / continue to next level | `Enter` or `Space` | click a row |
| Skip to next level | `Esc` | click the Continue row |

The intro, level-complete, game-over and victory screens also accept a click
anywhere to continue.

---

## Developer tools

The following flags are **desktop-only** (compiled out for the wasm target).

### Map editor — `--editor [path]`

Open the built-in map editor to create, edit, validate and save map TOML files:

```sh
# Open the existing level 1 file
cargo run -- --editor assets/maps/level01.toml

# Start a fresh editor bound to a new file
cargo run -- --editor assets/maps/new_map.toml

# Unbound: a fresh editor that saves to assets/maps/<default>.toml
cargo run -- --editor
```

Editor keys (grid mode):

| Action | Keys |
| --- | --- |
| Move cursor | Arrow keys |
| Select tool (Start / Exit / Structure) | `1` / `2` / `3` |
| Place / toggle at cursor | `Space` or `Enter` |
| Edit numeric fields | `Tab` |
| Save | `S` |
| Load | `L` |
| Edit file name | `F` |
| Quit | `Esc` |

In **Fields** mode, `Up/Down` selects a field and `Left/Right` adjusts it
(`Backspace` clears the seed); `Tab` returns to the grid. The editor **never writes
an invalid config** — save validates first and reports the error in the status bar.

### Screenshot — `--screenshot <path>`

Render a few frames and save a PNG for headless/CI visual checks:

```sh
cargo run -- --screenshot target/shots/menu.png
cargo run -- --editor assets/maps/level01.toml --screenshot target/shots/editor.png
```

### Forced screen — `DSH_SCREEN=<name>`

On desktop, the environment variable `DSH_SCREEN` forces the game into a given
screen at boot (used for debugging/screenshots). Supported values: `mainmenu`,
`levelselect`, `intro`, `settings`, `playing`, `pause`, `shop`, `levelcomplete`,
`gameover`, `victory`.

```sh
DSH_SCREEN=levelselect cargo run -- --screenshot target/shots/levelselect.png
```

---

## Levels

There are **10 levels** (`assets/maps/level01.toml` … `level10.toml`), listed in
`assets/game.toml` under `[map_order]`. Difficulty increases monotonically: the
share of dig-look rock that is **unmineable** (a fake-out you cannot dig) ramps
linearly from ~1/4 on level 1 to ~3/4 on level 10, with more (and faster) beasts,
bigger maps, and more unbreakable structures. `start`/`exit` vary per map.

Each TOML sets the map size, the fixed `unmineable_count` (chosen to hit that
ramp against the interior-minus-structures rock pool), `gold_count`, `beast_count`,
the `beast_speed_multiplier`/`beast_mining_time_multiplier`, the `start`/`exit`
gaps (both on the border), an optional `seed`, and interior `structures`. A level
is valid when `gold_count + unmineable_count + interior_structures <= interior`;
maps without a `seed` randomize every run.

`cargo test` includes an authoring guard
(`game::generation::tests::all_map_order_levels_generate`) that loads every level
in `[map_order]`, validates it, generates it, and asserts it is solvable — so a
broken level fails the suite.

---

## Configuration

All gameplay tuning lives in `assets/game.toml`: player/beast speeds and mining
times, upgrade cost curves, shop prices/lives, consumable costs and durations, and
scoring (`[score]`). `[map_order]` lists the level files in play order. The values
are a playtest-informed starting point — tune them freely; they require no code
changes.

Gold balance: each gold pickup is worth **15** gold (`game::level::GOLD_PER_PICKUP`,
in code), so a level's ~8–18 nuggets are worth 120–270 gold — enough to buy the
first upgrade early and pace progression across the run, while a perfect run that
grabbed every nugget lands just under the total upgrade cost (so upgrades don't
trivialize). `[score].gold_multiplier` was lowered to `1.0` (1 point per gold) so
the richer gold rewards don't swamp the time-based score.

---

## Tests

```sh
cargo test
```

The suite covers map parsing/validation, generation (incl. the all-levels
solvability guard), player/beast movement and mining, pathfinding, shop/economy,
scoring, save/load round-trips, the menu state machine, and the story-intro
routing.
