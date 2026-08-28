# M7 — Map Editor (Detailed Plan)

> Scope: milestone **M7** from [`PLAN.md`](../PLAN.md).
> **Working binary =** `--editor` opens a developer tool that can create, edit,
> validate, and save map TOML files, which the game then loads.

---

## 0. Current Baseline (post-M6)

- Full game + audio done. 204 tests.
- `main.rs` parses `--screenshot <path>` (desktop-only) and runs `App`.
- `config/map.rs` `MapConfig` is **deserialize-only** and `validate()` is
  **private**; the map TOML schema is:
  `width, height, unmineable_count, gold_count, beast_count,
  beast_speed_multiplier, beast_mining_time_multiplier, start {x,y},
  exit {x,y}, seed?, structures [[x,y],…]`.
- `Assets` loads the terrain atlas (`Assets::tile(TerrainTile)`), so the editor
  can reuse the real tiles for a preview.
- `MapConfig` validation already enforces: positive dims, start/exit on the
  border and distinct, and `gold + unmineable + interior-structures ≤ interior`.

---

## 1. Outcome (acceptance checklist)

- [ ] `cargo run -- --editor` opens the editor (desktop-only).
- [ ] `--editor <path>` opens an existing map (or a new one bound to that path).
- [ ] Developer can move a cursor over a grid and place the start gap, exit gap,
      and unbreakable structures.
- [ ] Developer can edit every numeric field (width, height, unmineable_count,
      gold_count, beast_count, speed/mining multipliers, seed).
- [ ] Save serializes the config to TOML, validates it first, and rejects invalid
      maps with a visible error (never writes a bad file).
- [ ] Load reads a map TOML back into the editor.
- [ ] The editor's pure logic (cursor, tools, fields, validate-on-save, round-trip)
      is unit-tested.

---

## 2. Dependencies

None. (`toml` and `serde` with the `derive` feature are already present.)

---

## 3. Config Changes (`config/map.rs`)

- Add `Serialize` to `Pos` and `MapConfig` (`use serde::{Deserialize, Serialize}`).
- Add `#[serde(skip_serializing_if = "Option::is_none")]` on `seed` so a "random"
  map omits the key.
- Make `validate()` `pub`.
- Add `MapConfig::to_toml(&self) -> Result<String, ConfigError>` (wrap
  `toml::to_string`; the existing `from_toml` covers the read side, so the
  round-trip `from_toml(&to_toml(cfg))` is a natural save-validation check).

---

## 4. Editor Module (`src/editor.rs`, new)

```rust
enum Tool { Start, Exit, Structure }
enum Mode { Grid, Fields }

struct Editor {
    cfg: MapConfig,
    cursor: (i32, i32),
    tool: Tool,          // active grid tool
    mode: Mode,          // Grid (place things) vs Fields (edit numbers)
    field_index: usize,  // selected row in Fields mode
    file_name: String,   // save target (without ".toml")
    status: Option<String>, // transient message/error
    assets: Assets,      // terrain tiles for the preview
}
```

**Pure, testable methods** (no rendering):
- `move_cursor(dx, dy)` — clamp to the grid.
- `apply_tool()` — place start / place exit / toggle structure at `cursor`.
- `adjust_field(delta)` — increment/decrement the selected numeric field.
- `set_dimensions(w, h)` — resize and re-clamp start/exit/structures to stay valid.
- `to_toml()` / `load_toml(text)` and `validate()` (delegates to `MapConfig`).

**Field list** (Fields mode rows, in order):
`width, height, unmineable_count, gold_count, beast_count, beast_speed_multiplier,
beast_mining_time_multiplier, seed`. `seed` is displayed as "random" when `None`.

---

## 5. Editor Interactions (keyboard)

| Key | Grid mode | Fields mode |
| --- | --- | --- |
| Arrows / WASD | move cursor | Up/Down = select field; Left/Right = adjust value |
| `1` / `2` / `3` | select tool (start / exit / structure) | — |
| Space / Enter | apply the active tool at the cursor | — |
| `Tab` | switch to Fields | switch to Grid |
| `S` | save (validate → write) | save |
| `L` | load (read `file_name`) | load |
| `Esc` | quit | quit |

Rules enforced by the pure methods:
- **Start/Exit** only place on the border; each is a single cell (placing one
  moves it); start ≠ exit.
- **Structure** toggles unbreakable rock on an **interior** cell only (never on
  the border ring, start, or exit).
- **Width/height** clamp to a sane floor (e.g. ≥ 5×5) and re-clamp start/exit/
  structures when resized.

---

## 6. Rendering (editor preview)

- Draw the grid with real tiles: border ring and structures as `Unbreakable`,
  start/exit gaps as `Dirt`, interior as `Mineable` (a placeholder — the actual
  mineable/unmineable split is randomized at runtime and isn't edited here).
- Highlight the cursor cell, and tint the start/exit cells distinctly (green/red).
- A right-hand panel lists the 8 fields with values, the active tool/mode, the
  save filename, and any `status` message; a footer shows the key bindings.
- Reuse `Assets::tile(TerrainTile)` (autotiling not needed — draw plain fills).

---

## 7. Save / Load

- **Save path:** the `--editor <path>` argument if given, else
  `assets/maps/<file_name>.toml` (append `.toml` if missing).
- **Save:** `cfg.to_toml()` → `cfg.validate()` (via a round-trip or the now-public
  `validate`); on success write the file and set `status = "saved <path>"`; on
  failure set `status` to the validation error and **do not write**.
- **Load:** read + `MapConfig::from_toml`; on error, surface it in `status`.
- `file_name` is typed with `get_char_pressed` (backspace supported); a minimal
  text field, no IME requirements.

---

## 8. CLI (`main.rs`)

- Add `--editor [path]` parsing next to `--screenshot` (both `#[cfg(not(wasm32))]`).
- Branch the `#[macroquad::main]` loop: if `--editor`, construct `editor::Editor`
  and run `editor.update(dt)` / `editor.draw()`; otherwise run `App` as today.
- The editor never runs on web (the flag is compiled out there).

---

## 9. Tests (unit)

- `config/map.rs`:
  - `to_toml → from_toml` round-trips a full config (and a minimal one).
  - `seed = None` omits the `seed` key in serialized output.
- `editor.rs` (pure):
  - cursor stays in bounds;
  - start/exit only place on the border and can't overlap;
  - structure toggles add/remove and ignore border/start/exit cells;
  - `set_dimensions` clamps start/exit/structures into the new bounds;
  - `adjust_field` clamps values (e.g. counts can't go negative; dims floor at 5);
  - `validate()` rejects off-border gaps / over-capacity counts;
  - filename normalization appends `.toml`.

Visual pass: run `--editor`, place a start/exit + structures, adjust fields, save,
then confirm the file loads in the editor and the game (add it to a temporary
`map_order` entry).

---

## 10. Decisions & Risks (confirm if they differ)

1. **Editor is CLI-launched only** (`--editor`), matching `REQUIREMENTS.md` §12.
   The "Map Editor" main-menu entry (§10) is **deferred** (the editor is a
   separate loop from `App`; wiring it into the menu is extra plumbing). If you
   want it in the menu, flag it and we'll add it in M8.
2. **The editor edits the *config*, not the generated layout** — the mineable/
   unmineable split is random per run and is not hand-edited (matches §6/§12).
   An optional "generate preview" (run `generation::generate` to *see* one random
   result) can be added if useful.
3. **Filename is a small text field** via `get_char_pressed`; no file browser.
4. **Desktop-only** (developer tool); no web editor.
5. **Min map size floor 5×5** in the editor (sane default; not a hard requirement).

---

## 11. Task List (ordered)

- [ ] 1. `config/map.rs`: add `Serialize`, `#[serde(skip_serializing_if)]` on
      `seed`, `pub validate`, `to_toml` + tests.
- [ ] 2. Add `src/editor.rs`: `Editor` state + pure methods + tests.
- [ ] 3. Add `editor` rendering (grid preview + field panel + status).
- [ ] 4. Wire `--editor [path]` in `main.rs` and branch the loop.
- [ ] 5. Manual pass: create/edit/save a map, reload it, and load it in the game.

---

## 12. Out of Scope (later milestones)

- Main-menu "Map Editor" entry (if desired — M8).
- Generation preview inside the editor (optional).
- Final 10-level authoring/balancing (M8).
