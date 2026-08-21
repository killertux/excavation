# M1 — Foundation, Asset Pipeline, Player Movement (Detailed Plan)

> Scope: milestone **M1** from [`PLAN.md`](../PLAN.md).
> **Working binary =** the game boots on desktop **and** web, loads and slices the
> asset sheets, and the player walks around a rendered map with a scrolling camera.

---

## 1. Outcome (acceptance checklist)

- [ ] `cargo run` opens a 1280×720 window on desktop; WASM build serves and plays in a browser.
- [ ] Terrain tiles (rock, floor, wall, border, start/exit door) render from the terrain atlas.
- [ ] The player sprite renders and animates (idle/walk) while moving.
- [ ] The player moves continuously in 8 directions with keyboard **and** gamepad.
- [ ] Collision blocks movement through rocks, walls, and the border.
- [ ] The camera follows the player and is clamped to the map bounds.
- [ ] Asset slicing unit tests pass (frame counts + 16×16 output).

---

## 2. Prerequisites

- `rustup target add wasm32-unknown-unknown`
- Assets are committed under `assets/` (see `REQUIREMENTS.md` §15.3 for paths/frame order).
- No mining, beasts, config, or menus in this milestone.

---

## 3. Setup & Dependencies

- `cargo init --name excavation` (in the repo root).
- `cargo add macroquad`  — game framework.
- `cargo add image`      — PNG decode + resizing in the **pure** (testable) layer.
- Extend `.gitignore` with Rust/web build artifacts (`/target`, `*.wasm`, `web/dist/`, etc.).

> `serde` / `toml` are **not** added here — config loading is M2.

---

## 4. Module Skeleton

```
src/
  main.rs          entry point; window_conf; routes to App
  app.rs           App struct; state; update()/draw(); owns Assets/Map/Player/Camera
  input.rs         Input snapshot -> normalized move intent (keyboard + gamepad)
  assets/
    mod.rs         Assets struct; runtime texture upload; lookup by id
    layout.rs      PURE: detect frames/tiles from a decoded image (no GPU)
    ids.rs         TileId + PlayerAnim enums
  game/
    mod.rs
    map.rs         Tile enum; Map struct; placeholder_map()
    player.rs      Player struct; movement + collision vs Map
    camera.rs      Camera; follow + clamp; world<->screen transform
```

All **game logic** (movement, collision, camera math, asset layout) lives in pure
functions where possible, so it can be unit-tested without a window/GPU.

---

## 5. Asset Pipeline

### 5.1 Measured layout facts (use these to guide detection)

All sheets are **one row of frames, left-to-right, separated by transparent
gutters**, with transparent vertical margins. Measured from the committed files:

| Sheet | Size | Frames | Notes |
| --- | --- | --- | --- |
| `terrain_atlas.png` | 2126×740 | 7 tiles | ~294×245 content each; roughly square. |
| `player_sheet.png` | 2172×724 | 4 frames | Portrait frames (~267–365 × ~560 content); idle, walk×2, mining. |
| `beast_sheet.png` | 2172×724 | 4 frames | Portrait (deferred to M3). |
| `pickups_and_shop_icons_atlas.png` | 1659×948 | 8 sprites | Single row, ~190×249 each (deferred to M4). |
| `dig_particles_sheet.png` | 2172×724 | 8 frames | Sparse; needs per-frame handling (deferred). |
| `ui_atlas.png` | 1672×941 | 4 regions | 9-slice UI (deferred to M5). |
| `title_logo.png`, `menu_background.png` | — | 1 each | Whole images (deferred to M5). |

Frame order per sheet is authoritative from `REQUIREMENTS.md` §15.3.

### 5.2 Loader design

Two layers:

1. **Pure layer (`assets/layout.rs`)** — decodes the PNG with `image`, detects
   frames/tiles, and resizes to 16×16. No macroquad/GPU. Fully unit-tested.
2. **Runtime layer (`assets/mod.rs`)** — uploads the resulting 16×16 RGBA
   buffers to macroquad `Texture2D` and exposes lookups by id.

**Detection algorithm (pure):**
1. Decode to `RGBA8`.
2. Build a per-column opacity profile (fraction of opaque pixels per column).
3. Split into **content regions** at columns that are (near) fully transparent
   (the gutters), left-to-right.
4. For each region, compute the **tight content bounding box** (min/max opaque
   pixel), trimming the vertical margins and intra-frame padding.
5. **Resize** each bbox to 16×16 using `image`'s resize (default filter).
   - `ScaleMode::Stretch` (default) — non-uniform, fills 16×16.
   - `ScaleMode::Fit` — aspect-preserving, centered with padding.
6. Return `Vec<Frame { id, rgba16x16 }>`.

A `SheetSpec` table drives this: `{ path, expected_frames, scale_mode }`, with an
optional `explicit_rects` override for any sheet auto-detection can't handle
(e.g. the particles sheet, later). Loading asserts `expected_frames` matches what
detection found, so a mis-detected sheet fails loudly.

### 5.3 Sheets to load in M1

| Sheet | Id enum | Frames |
| --- | --- | --- |
| `terrain_atlas.png` | `TileId` | Rock, Floor, Wall, Border, StartDoor, ExitDoorClosed, ExitDoorOpen |
| `player_sheet.png` | `PlayerAnim` | Idle, Walk1, Walk2, Mining |

Other sheets are added in their milestones by adding a `SheetSpec` entry — the
loader is already generic.

---

## 6. Core Game Types

### 6.1 `Tile` (game/map.rs)
```rust
enum Tile { Rock, Excavated, Wall, Border, StartDoor, ExitDoor }
```
- `Tile::solid()` → `Rock | Wall | Border` (blocks movement).
- `Tile::tile_id()` → the terrain atlas frame.

### 6.2 `Map` (game/map.rs)
- `{ width, height, tiles: Vec<Tile> }` (row-major).
- `tile(x, y) -> Tile`, `in_bounds(x, y) -> bool`.
- `placeholder_map() -> Map` for M1: a hand-written grid (≈30×20) with a border,
  a start door, an exit door, a pre-excavated walkable area/corridor, and a few
  rocks + visible walls to exercise collision. (No generation, no TOML yet.)

### 6.3 `Player` (game/player.rs)
- `{ pos: Vec2 /* px, world */, speed: f32, anim: PlayerAnim }`.
- Hitbox: an AABB slightly smaller than a tile (e.g. 12×12 inside a 16×16 tile).
- `update(&mut self, move_intent: Vec2, map: &Map, dt: f32)`.

### 6.4 `Camera` (game/camera.rs)
- `{ pos: Vec2 /* px, top-left */, zoom: f32 }`.
- Follows the player (center-on, optional lead offset).
- Clamped to map bounds; if the map is smaller than the view, it centers the map.
- `world_to_screen(p) -> Vec2` and `screen_to_world(p) -> Vec2`.

### 6.5 `App` (app.rs)
- Owns `Assets`, `Map`, `Player`, `Camera`, `Input`.
- `update(dt)`: gather input → move player → update camera.
- `draw()`: clear, set camera transform, draw tiles then player.

---

## 7. Movement & Collision Rules

- **Speed:** constant for M1 (e.g. 120 world px/s) as a named constant with a
  `// TODO: move to game.toml in M2`.
- **Input → intent:** keyboard (WASD/arrows) + gamepad (left stick/D-pad)
  combine into a `Vec2`, then **normalized** so diagonals aren't faster.
- **Collision (axis-separated):**
  1. Apply X movement; for each solid tile the hitbox now overlaps, clamp X.
  2. Apply Y movement; for each solid tile the hitbox now overlaps, clamp Y.
- Only solid tiles (`Rock | Wall | Border`) collide. Doors and excavated floor
  are walkable.
- Determine overlap by the tile indices the hitbox spans (world px → tile via
  `floor(p / 16)`), so it works for any speed/dt.

---

## 8. Camera Behavior

- Logical tile = 16 px; world units are px. Camera `zoom` (default 2.0) makes a
  16 px tile render at 32 px on screen for a playable view at 1280×720.
- Center the camera on the player each frame.
- Clamp so the visible world region stays within `[0, map_w] × [0, map_h]`;
  if the map is smaller than the view, center the map (no black gaps).
- `world_to_screen(p) = (p - camera.pos) * zoom`.

---

## 9. Rendering

- Window config: 1280×720, title "Excavation".
- Each frame: clear background → for every visible tile, draw its 16×16 texture
  scaled by `zoom` at its world position (via the camera transform) → draw the
  player texture (idle when still, walk frames while moving).
- Animate walk frames by time (alternate `Walk1`/`Walk2` while moving).

---

## 10. Controls (M1 subset)

| Action | Keyboard | Gamepad |
| --- | --- | --- |
| Move | WASD / Arrow keys | Left stick / D-pad |

Mining, consumables, and pause arrive in later milestones. `input.rs` reads both
sources via macroquad and returns one `Vec2` move intent.

---

## 11. Web (WASM)

- Build: `cargo build --release --target wasm32-unknown-unknown`.
- Add a `web/` folder with an `index.html` that loads the compiled
  `excavation.wasm` using macroquad's JS loader (vendor the loader file locally;
  no CDN). Follow the current macroquad web docs for the exact glue.
- Serve locally with a static server (e.g. `python3 -m http.server` from `web/`)
  and verify movement in the browser.
- Optionally add a tiny `build-web.sh` that builds and copies the wasm + index
  into `web/`.

---

## 12. Tests (unit)

- `assets/layout.rs`:
  - Terrain sheet detects exactly 7 tiles; each output is 16×16.
  - Player sheet detects exactly 4 frames; each output is 16×16.
  - `ScaleMode::Stretch` vs `Fit` produce expected sizes.
  - Wrong `expected_frames` in a spec → error (fail loud).
- `game/map.rs`:
  - `Tile::solid()` returns true for Rock/Wall/Border, false otherwise.
  - `placeholder_map()` has a border on all edges and a start + exit door.
- `game/player.rs`:
  - Moving into a solid tile is blocked (axis-separated collision).
  - Moving through excavated/doors is allowed.
  - Diagonal input is normalized (no speed-up).
- `game/camera.rs`:
  - Camera centers on player.
  - Camera clamps to map bounds; small map centers without gaps.

---

## 13. Task List (ordered)

- [ ] 1. `cargo init --name excavation`; add `macroquad` + `image`; extend `.gitignore`.
- [ ] 2. Create module skeleton and get an empty 1280×720 window running.
- [ ] 3. Implement `assets/layout.rs` (decode → gutter detection → bbox trim → 16×16 resize) + unit tests.
- [ ] 4. Implement `assets/mod.rs` texture upload + `TileId`/`PlayerAnim` lookups.
- [ ] 5. Implement `game/map.rs` (`Tile`, `Map`, `placeholder_map`) + tests.
- [ ] 6. Implement `game/player.rs` (movement + collision) + tests.
- [ ] 7. Implement `game/camera.rs` (follow + clamp) + tests.
- [ ] 8. Implement `input.rs` (keyboard + gamepad → move intent).
- [ ] 9. Wire `App::update/draw`: render tiles + player, move, camera transform.
- [ ] 10. Verify player walk animation (idle vs walk frames).
- [ ] 11. Set up WASM build (`index.html`, loader, build/serve) and verify in browser.
- [ ] 12. Run full test suite + manual desktop/web pass against §1 checklist.

---

## 14. Risks & Decisions

1. **Character frame aspect ratio.** The character sheets are portrait
   (~267–365 × ~560 content). `ScaleMode::Stretch` to 16×16 will squash them;
   `Fit` will make them narrow. **Decision:** start with `Stretch` for
   determinism, then visually check and switch the character sheets to `Fit`
   (or a square center-crop) if it looks wrong. Do this **during** task 4/9, not
   at the end.
2. **Particles sheet is sparse** (many internal gaps). Auto gutter-detection is
   unreliable there. It is deferred (not needed until later); when added, give it
   `explicit_rects` rather than auto-detection.
3. **macroquad web glue.** The exact `index.html`/loader changes between
   macroquad versions. Follow the installed version's docs and **vendor** the
   loader locally; verify early (task 11) rather than last.
4. **Speed constant.** Hard-coded in M1 and moved into `game.toml` in M2; keep it
   in one named constant to make that move trivial.

---

## 15. Verification & Implementation Notes (recorded during M1)

> Notes captured while building M1. Read these before starting later milestones —
> they document reusable tooling and a framework gotcha that is easy to re-hit.

### 15.1 Headless visual verification (`--screenshot`)

The game can render a single frame to a PNG and exit, so the result can be
**checked without a window/display** (works under software GL too):

```
cargo run -- --screenshot shot.png    # renders ~3 frames, saves shot.png, exits
```

This is desktop-only (screenshot export is not supported on web); the flag is
`#[cfg(not(target_arch = "wasm32"))]`-gated. It renders the scene into a
macroquad `RenderTarget` (not the live screen buffer) and reads it back with
`fb.texture.get_texture_data().export_png(path)`. Use it to visually verify each
milestone's output and to catch render regressions.

### 15.2 macroquad `Camera2D::zoom` is NOT a magnification factor

macroquad's `Camera2D::zoom` is a **clip-space scale** equal to
`2 / visible_world_size` (see `Camera2D::from_display_rect`), **not** "screen px
per world px". Passing a magnification like `0.2 = 2.0` directly collapses the
visible world to ~1 px, so a single tile fills the screen (symptom: a flat,
featureless colour field). To show the world at magnification `mag`, use:

```
zoom = (2.0 * mag / view_w, 2.0 * mag / view_h)   // view = screen size, in px
```

and set `target` to the centre of the visible world rect. When rendering to a
`RenderTarget`, the Y component must be **negated** (macroquad flips the Y scale
for render targets internally, but not for the default screen). See
`src/app.rs::mq_zoom` and its unit tests.

### 15.3 Open items / decisions

- **Player sprite scale mode.** `player_sheet.png` frames are portrait
  (~267–365 × ~560). M1 uses `ScaleMode::Fit`, so the sprite is aspect-preserving
  but narrow (~8–10 px wide in a 16 px tile). Revisit `Stretch` vs `Fit` (or a
  square centre-crop) during a visual pass — this was plan §14 risk #1.
- **Gamepad deferred.** macroquad 0.4.16 (and miniquad 0.4.11) expose **no**
  gamepad API; `input.rs` is keyboard-only and structured so a gamepad source can
  be added later.
- **Sparse sheets** (e.g. `dig_particles_sheet.png`) must use
  `SheetSpec::explicit_rects` rather than auto-detection; auto-gutter detection
  fragments on sheets with internal transparent columns (see `layout.rs`).

