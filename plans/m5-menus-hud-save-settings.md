# M5 — Menus, HUD, Save/Load, Settings (Detailed Plan)

> Scope: milestone **M5** from [`PLAN.md`](../PLAN.md).
> **Working binary =** the complete game with all screens, persistence across
> sessions, and settings — main menu, level select, pause, level-complete, game
> over, victory, shop, HUD, save/load (desktop file / web localStorage), and
> volume/fullscreen settings.

---

## 0. Current Baseline (post-M4)

- The simulation is complete and split cleanly: `Run` (cross-level state) +
  `Level` (per-level) + `shop`/`upgrades`/`consumables`/`score`/`pickup`. 160 tests.
- `app.rs` drives a `GameState { Playing, LevelComplete, Shop, GameOver, Victory }`
  with **text-based** overlays/HUD and a keyboard shop. No main menu, level
  select, pause, save/load, or settings yet.
- `config/game.rs` already loads `map_order`; `App` loads all maps at boot.
- Input is keyboard-only (`Input { move_, use_super_pick, use_sticky_smell }`).
  Gamepad is still unavailable (macroquad 0.4.16 has no gamepad API).
- The combined atlas + two standalone PNGs already contain all M5 UI assets (see §10).

---

## 1. Outcome (acceptance checklist)

- [ ] Main menu (Play, Continue when a save exists, Level Select, Settings, Quit).
- [ ] Level select shows the levels, locked until reached; selecting one starts it.
- [ ] Pause menu (Resume, Restart Level, Save, Settings, Quit to Menu) via Esc.
- [ ] Save/load full progress + settings (desktop file, web localStorage).
- [ ] Continue resumes the run at the saved level with gold/upgrades/lives/
      consumables/score intact.
- [ ] Settings screen: music volume, SFX volume, fullscreen (persisted).
- [ ] Full HUD: hearts, gold (live), level, elapsed time, consumable counts,
      active-effect timer.
- [ ] All screens reachable and correct on desktop **and** web.
- [ ] Save/load round-trip and menu state-machine tests pass.

---

## 2. Dependencies

```
cargo add serde_json
cargo add quad-storage
```

- `serde_json` — serialize the save to JSON.
- `quad-storage` — one persistent-storage API across desktop (file) and web
  (localStorage). (Verify its exact `storage::{get,set}` API against its docs; if
  unsuitable, fall back to a `#[cfg]` split: `std::fs` on desktop, a tiny JS
  bridge on web.)

---

## 3. Save / Load (`src/save.rs`, new)

### 3.1 Data model

```rust
#[derive(Serialize, Deserialize)]
struct SaveData {
    version: u32,                       // = 1; reject unknown versions
    run: RunSnapshot,
    settings: Settings,
}

#[derive(Serialize, Deserialize)]
struct RunSnapshot {
    gold: u32,
    upgrades: Upgrades,
    consumables: Consumables,
    lives: u32,
    score_total: u64,
    level_index: usize,
    unlocked: usize,                    // highest selectable 1-based level
}
```

- `Upgrades` and `Consumables` gain `Serialize`/`Deserialize` derives.
- The save holds **run-level** state only — **not** the in-level sim (map,
  player/beast positions, pickups, elapsed, active effect). On load, the level at
  `level_index` is rebuilt fresh (see §5). This matches `REQUIREMENTS.md` §11.
- `Settings` (music/sfx volume, fullscreen) lives inside `SaveData` (see §4).

### 3.2 Storage

- Key: `"excavation_save_v1"`.
- Desktop → a file; web → `localStorage`, both through `quad-storage`
  (or the `#[cfg]` fallback). The save is a JSON string.
- `save::save(&SaveData)` / `save::load() -> Option<SaveData>` (plus
  `save::clear()` for "Play" starting a fresh run).
- Corrupt/absent/version-mismatched saves are treated as "no save" (never crash).

---

## 4. Settings (`src/settings.rs`, new)

```rust
struct Settings {
    music_volume: f32,   // 0.0..=1.0
    sfx_volume: f32,     // 0.0..=1.0
    fullscreen: bool,
}
```

- Defaults `1.0 / 1.0 / false`. Stored in the save.
- Volume values persist now and take effect in **M6** (audio). Fullscreen is read
  at startup from the save (`window_conf`) and toggled at runtime if macroquad
  supports it (best-effort — see §13).

---

## 5. Run Changes (`game/run.rs`)

- Add `unlocked: usize` (init `1`); bump it when the player advances past the
  furthest unlocked level. Serialized in `RunSnapshot`.
- `Run::snapshot() -> RunSnapshot` and `Run::resume(cfg, map_cfgs, snapshot) ->
  Result<Run, …>` (build the level at `snapshot.level_index` with the saved
  upgrades).
- `Run::start_level(index)` — build a fresh `Level` at `index` with the current
  run state (used by level select).
- `Run::restart_current_level()` — `level.restart(fresh_random_seed())` for the
  pause "Restart Level" action (no life cost).
- Keep `update`, `buy`, `begin_next_level`, `item_cost`, etc. unchanged.

---

## 6. Menus (`src/menu.rs`, new — pure state machines)

Menu logic is **pure** (testable, no macroquad): each screen is a struct/enum with
a selection index and an `update(MenuInput) -> MenuAction` that returns an action
the `App` executes. `MenuInput` is a plain struct of edge-triggered booleans
(up/down/left/right/enter/esc/…) that `app.rs` fills from macroquad keys.

**Screens & behavior:**
- **MainMenu** — `Play` (fresh run + clear save), `Continue` (only shown when a
  save exists), `Level Select`, `Settings`, `Quit`.
- **LevelSelect** — list `1..=level_count`; entries `> unlocked` are locked and
  unselectable. Selecting an unlocked level calls `Run::start_level(i)` and plays.
- **Settings** — music volume (left/right), SFX volume (left/right), fullscreen
  (enter to toggle), `Back`.
- **Pause** — `Resume`, `Restart Level`, `Save`, `Settings`, `Quit to Menu`.

`MenuAction` enum: `None`, `NewGame`, `Continue`, `OpenLevelSelect`, `OpenSettings`,
`Back`, `StartLevel(usize)`, `ToggleFullscreen`, `VolumeUp/Music`, `VolumeDown/Music`,
`VolumeUp/Sfx`, `VolumeDown/Sfx`, `Resume`, `RestartLevel`, `Save`, `SaveAndQuitToMenu`,
`Quit`.

---

## 7. UI Rendering (`src/ui.rs`, new)

A thin drawing layer over the atlas UI sprites (keyboard-only navigation; mouse
not required):

- **Buttons** — 4-frame `Button` atlas sprite (normal / hover / pressed / disabled).
- **Panels** — `Base GUI panel` sprite behind menus.
- **Sliders** — `Adjustable Bar` (4 frames) + `ScrollBar fill` for volume.
- **Main menu backdrop** — `menu_background.png` scaled to the window +
  `title_logo.png` scaled to a banner.
- A small `Ui` helper: `draw_button(label, rect, state)`, `draw_panel(rect)`,
  `draw_slider(value, rect)`. Keep it minimal; no full 9-slice framework.

---

## 8. HUD (`src/hud.rs`, new — or keep in app.rs)

Replace the ad-hoc `draw_hud` with a full HUD:
- **Lives** as heart icons (existing `IconId::Heart`).
- **Gold** gathered this level (live), and banked gold on outcome screens.
- **Level** `n / total`.
- **Elapsed time** this level (mm:ss) and a live score estimate (optional).
- **Consumable counts** — `Super Pick xN`, `Sticky Smell xN`.
- **Active-effect timer** when a consumable is running (existing).

---

## 9. App State Machine (`app.rs`)

`GameState` expands to a full flow (Play/Shop/LevelComplete/GameOver/Victory move
under a menu layer):

```
Boot → load config + maps → load save (if any)
  → MainMenu
      ├─ Play        → clear save, Run::new, → Playing
      ├─ Continue    → Run::resume(snapshot) → Playing
      ├─ LevelSelect → pick → Run::start_level(i) → Playing
      ├─ Settings    → Settings screen
      └─ Quit        → exit
Playing
  ├─ Esc → Paused
  ├─ LevelCompleted → LevelComplete → (Enter) → Shop → Continue → next / Victory
  ├─ GameOver → GameOver screen → MainMenu
  └─ Victory  → Victory screen → MainMenu
Paused → Resume | Restart Level | Save | Settings | Quit to Menu (auto-save)
```

- `App` owns `Settings` alongside `Run`/`Assets`/`Camera`.
- `Quit to Menu` and explicit `Save` both persist the run snapshot + settings.
- `--screenshot` still captures the game world; add a way to capture the menus too
  (e.g. a `--screenshot-menu` flag or render the current state) for visual checks.

---

## 10. Assets (already committed — slice/load only)

UI sprites in the atlas (from `My project atlas.json`):

| Sprite | Rects (x, y, w, h) | Use |
| --- | --- | --- |
| Button | (0,571,48×16), (49,571,48×16), (98,571,48×16), (147,571,48×16) | normal/hover/pressed/disabled |
| Base GUI panel | (0,588,64×48) | menu backdrop panel |
| Adjustable Bar | (0,637,96×24), (97,637,96×24), (194,637,96×24), (291,637,96×24) | slider track states |
| ScrollBar fill | (0,662,32×32) | slider fill/knob |

Standalone PNGs (scale to the 1280×720 window):
- `assets/images/ui/title_logo.png` (2172×724) — title banner.
- `assets/images/backgrounds/menu_background.png` (1672×941) — menu backdrop.

Extend `assets/mod.rs` with `ui_button(state)`, `ui_panel()`, `ui_slider(state)`,
`ui_scroll_fill()`, `title_logo()`, `menu_background()` accessors (scale the large
PNGs down at load). No new art is required.

---

## 11. Input

- Keep `input.rs` as-is for gameplay (`move_`, consumables).
- Add a `menu_input()` helper in `app.rs` (or `input.rs`) that maps keys to the
  pure `MenuInput` struct (`Up/Down/W/S`, `Left/Right/A/D`, `Enter/Space`, `Esc`).
- Gamepad remains deferred (no macroquad gamepad API).

---

## 12. Tests (unit)

- `save.rs`: round-trip `SaveData → JSON → SaveData` equality; unknown version /
  corrupt JSON → `None`; `RunSnapshot` round-trips through `Run::snapshot` /
  `Run::resume`.
- `settings.rs`: defaults; clamp volumes to `0..=1`.
- `run.rs`: `snapshot`/`resume` preserve gold/upgrades/lives/consumables/score/
  level_index; `unlocked` increments on advancing; `start_level(i)` builds the
  requested level; `restart_current_level` regenerates the map (no life change).
- `menu.rs`: each screen's navigation — selection wraps, locked levels are
  unselectable, actions fire on Enter, `Continue` hidden when no save (state
  field), etc.

Visual things (menu rendering, HUD layout, fullscreen) are verified manually.

---

## 13. Decisions & Risks (confirm if they differ)

1. **Save is run-level, not mid-level.** Saving mid-level stores progress but not
   the map/positions; `Continue` restarts that level fresh. Matches §11 (no
   mention of in-level state).
2. **`Quit to Menu` auto-saves** so the player never silently loses progress.
3. **`Restart Level` (pause) is free** (no life cost) — it just regenerates a fresh
   map.
4. **Level select replay uses the current run state** (gold/upgrades/lives), which
   can be farmed by replaying easy levels. Flag for M8 balancing (e.g. disable
   gold/score re-banking on replay).
5. **Fullscreen** is read at startup; runtime toggle is best-effort (macroquad
   0.4.16 may not expose a reliable runtime toggle — verify; otherwise apply on
   next launch).
6. **Volume settings persist now but take effect in M6** (no audio yet).
7. **Mouse is out of scope** (keyboard-only menus); gamepad remains deferred.

---

## 14. Task List (ordered)

- [ ] 1. `cargo add serde_json quad-storage`.
- [ ] 2. Add `src/settings.rs` (Settings + defaults + clamp) + tests.
- [ ] 3. Add serde derives to `Upgrades`/`Consumables`; add `Run::snapshot`/`resume`/
      `start_level`/`restart_current_level` + `unlocked` + tests.
- [ ] 4. Add `src/save.rs` (SaveData, save/load/clear via storage) + tests.
- [ ] 5. Add `src/menu.rs` (pure menu state machines + `MenuAction`) + tests.
- [ ] 6. Slice/load the UI sprites + title/background in `assets/`; add `src/ui.rs`.
- [ ] 7. Add `src/hud.rs` (full HUD).
- [ ] 8. Rewire `app.rs`: menu layer, pause, settings, save/load, "Continue",
      level select, game-over/victory return-to-menu.
- [ ] 9. Full `cargo test`; desktop run + screenshots of each screen; WASM
      build/boot + save/load across reload.

---

## 15. Out of Scope (later milestones)

- Audio and volume actually affecting sound (M6).
- Map editor (M7) — the main-menu entry appears then.
- Final 10-level balancing, replay anti-farming (M8).
