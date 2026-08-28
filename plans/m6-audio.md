# M6 — Audio (Detailed Plan)

> Scope: milestone **M6** from [`PLAN.md`](../PLAN.md).
> **Working binary =** the complete game with music (menu / level / chase) and all
> sound effects, respecting the Settings volume controls.

---

## 0. Current Baseline (post-M5)

- Full game loop + menus/HUD/save/settings are in place. 193 tests.
- `app.rs` owns a `GameState { MainMenu, LevelSelect, Settings, Paused, Playing,
  LevelComplete, Shop, GameOver, Victory }` and routes update/draw through it.
- `Settings { music_volume, sfx_volume, fullscreen }` already exists and persists;
  volume values are stored but **do nothing yet** (the M5 note says "takes effect
  on audio in M6").
- The pure sim exposes coarse events already: `RunEvent { Playing, Caught,
  LevelCompleted{score}, GameOver, Victory }` and `LevelEvent { None, Completed,
  Caught }`. Finer moments (rock breaks, gold pickup, consumable use) happen
  inside `Level`/`Run` and are not yet surfaced.
- All 18 audio files are committed under `assets/audio/` (3 music loops + 15 SFX).

---

## 1. Outcome (acceptance checklist)

- [ ] Music loops play and switch: menu on menus/shop/outcomes, level while
      playing, chase when a beast has a clear (open) path to the player.
- [ ] All in-scope SFX fire at the right moments (see the trigger map in §6).
- [ ] `Settings.music_volume` and `Settings.sfx_volume` control playback live.
- [ ] No audio crashes on desktop **or** web (incl. the browser autoplay rule).
- [ ] `cargo test` still green; audio is verified manually on both platforms.

---

## 2. Dependencies

None — macroquad ships audio (`macroquad::audio`). All files are already present.

---

## 3. Audio Module (`src/audio.rs`, new)

```rust
pub enum Music { Menu, Level, Chase }
pub enum Sfx { Dig, BeastDig, RockBreak, GoldPickup, SuperPick, StickySmell,
               BeastGrowl, Footstep, Caught, LevelComplete, GameOver, Purchase, UiClick }

pub struct Audio {
    music: [Sound; 3],        // indexed by Music
    sfx: HashMap<Sfx, Sound>, // or a fixed struct/enum-indexed array
    music_volume: f32,
    sfx_volume: f32,
    current_music: Option<Music>,
    dig_playing: bool,        // loop-state tracking (avoid re-triggering each frame)
    beast_dig_playing: bool,
}
```

- `Audio::load() -> Audio` — `load_sound("assets/audio/…")` for all 18 files.
- `play_music(&mut self, m: Music)` — stop the current loop, start `m` at
  `music_volume` (`PlaySoundParams { looped: true, volume }`); no-op if already
  playing `m`.
- `play(&mut self, s: Sfx)` — one-shot at `sfx_volume`.
- `start_loop(Sfx)` / `stop_loop(Sfx)` — for the two continuous loops (dig, beast
  dig), tracked so they aren't restarted every frame.
- `set_music_volume(&mut self, v)` / `set_sfx_volume(&mut self, v)` — apply live
  (`set_sound_volume` on the current music; store `sfx_volume` for future plays).
  If macroquad lacks `set_sound_volume`, fall back to restarting the current
  music loop with the new volume.

The `Music`/`Sfx` enums are plain data (no macroquad), so game modules can push
`Sfx` values without a render/audio dependency in the *logic* itself.

---

## 4. Surfacing Fine-Grained Sound Events (game layer)

Add a per-frame `Vec<Sfx>` sound queue so the pure sim can report one-shot events
without knowing how they're played:

- `Level` gains `sound_events: Vec<Sfx>` (cleared each `update`):
  - push `Sfx::RockBreak` when a cell is excavated (in `drop_gold`/after the
    `take_excavated` drain — every rock break, player or beast);
  - push `Sfx::GoldPickup` when `collect_pickups` collects gold.
- `Run` gains `sound_events: Vec<Sfx>`:
  - push `Sfx::SuperPick` / `Sfx::StickySmell` when `try_use_consumable`
    successfully spends one.
- `Run::drain_sounds() -> Vec<Sfx>` returns its own events plus
  `level.drain_sounds()`.

`LevelEvent::Caught` / `Completed` and the `RunEvent` variants stay as-is; `App`
maps those directly (no need to duplicate them into the queue).

---

## 5. Music Switching Logic

`App` decides the music each frame and calls `play_music` on change:

| State | Music |
| --- | --- |
| MainMenu, LevelSelect, Settings, Paused, LevelComplete, Shop, GameOver, Victory | `Menu` |
| Playing, no beast has a clear path to the player | `Level` |
| Playing, any beast has a clear (dirt-only) path to the player | `Chase` |

- "Clear path" = a dirt-only A\* path from the beast's cell to the player's cell
  exists (the same passability rule the beast already uses for its "walk the
  clear tunnel" branch in `decide`). Add a pure, testable
  `Beast::has_clear_path(player_pos, map) -> bool` helper.

---

## 6. SFX Trigger Map

| Sfx | When it fires | Source |
| --- | --- | --- |
| `Dig` | continuous loop while the player is mining (skip during Super Pick's instant mine) | `app.rs` from `level.player.mining` |
| `BeastDig` | continuous loop while any beast is digging | `app.rs` from `beast.dig_frame()` |
| `RockBreak` | one-shot when a rock becomes dirt (player or beast) | `Level` sound queue |
| `GoldPickup` | one-shot when the player collects a gold pickup | `Level` sound queue |
| `SuperPick` | one-shot when Super Pick is activated | `Run` sound queue |
| `StickySmell` | one-shot when Sticky Smell is activated | `Run` sound queue |
| `BeastGrowl` | one-shot when a beast becomes "near" the player (edge, with a cooldown) | `app.rs` proximity edge |
| `Footstep` | (optional) periodic (~0.25 s) while the player walks | `app.rs` timer |
| `Caught` | `RunEvent::Caught` | `app.rs` |
| `LevelComplete` | `RunEvent::LevelCompleted` **and** `Victory` | `app.rs` |
| `GameOver` | `RunEvent::GameOver` | `app.rs` |
| `Purchase` | successful `run.buy(item)` (Ok) | `app.rs` shop |
| `UiClick` | menu selection move and activation | `app.rs` menu |

Deferred (no game object yet): `Sfx::GemPickup` (story-only gem, M8). The
`gem_pickup.wav` file stays unused for now.

---

## 7. App Integration

- `App` gains `audio: Audio` (loaded in `App::new` after assets).
- Each `update`:
  1. drain `run.drain_sounds()` and play each;
  2. map `RunEvent` → `Caught` / `LevelComplete` / `GameOver` / `Victory` SFX;
  3. play `Purchase` on a successful shop buy; `UiClick` on menu navigation/activation;
  4. drive the continuous loops (`dig`, `beast_dig`, optional `footstep`);
  5. update music (`play_music`) from state + chase proximity;
  6. after any volume change in `apply_action`, call
     `audio.set_music_volume(…)` / `audio.set_sfx_volume(…)`.
- Add a `--mute`-style escape hatch? Not required; volumes are already settable
  via Settings.

---

## 8. Settings Wiring

- On boot and on every `VolumeUp/Down` action, push the value into `audio`
  (music volume also re-applies to the current loop).
- The existing `maybe_persist_settings` path already saves the values; no change
  to persistence.

---

## 9. Tests (unit)

Audio playback itself is **manual** (macroquad/mixer state). What is unit-tested:

- `Level`: `drain_sounds` reports `RockBreak`/`GoldPickup` at the right moments
  (e.g. mining a rock emits one `RockBreak`; collecting gold emits `GoldPickup`).
- `Run`: `drain_sounds` reports `SuperPick`/`StickySmell` on successful use, and
  nothing when none are owned.
- `audio.rs` (pure parts only): the `Music`/`Sfx` → file-path mapping (e.g. a
  `fn path(Sfx) -> &'static str` helper) is correct and complete for the 18 files.

Visual/auditory pass: run on desktop and web; confirm each row of §6 by playing.

---

## 10. Task List (ordered)

- [ ] 1. Add `src/audio.rs` (enums, `Audio::load`, `play_music`, `play`, loop
      start/stop, volume setters) + the path-mapping test.
- [ ] 2. Add the `sound_events` queue to `Level` and `Run` (`drain_sounds`) + tests.
- [ ] 3. Wire `App`: load `Audio`, drain/play the queue, map `RunEvent`/shop/menu
      → SFX, drive dig/beast-dig/footstep loops, switch music, apply volumes.
- [ ] 4. Full `cargo test`; desktop manual pass (each trigger); WASM build/boot +
      confirm audio after first input (autoplay policy).

---

## 11. Decisions & Risks (confirm if they differ)

1. **Chase music = the beast has a clear (dirt-only) path to the player** (not
   proximity). `Beast::has_clear_path` reuses the dirt-only A\* check. While a
   beast is digging/blocked, the music stays at `Level`.
2. **Victory reuses the `level_complete` fanfare** (there is no dedicated victory
   SFX in the asset pack).
3. **`Footstep` is optional polish** — a periodic tick while walking; skip it if
   it feels noisy.
4. **`BeastGrowl` is edge-triggered with a cooldown** (~2 s) so multiple/nearby
   beasts don't spam it.
5. **Web autoplay policy**: browsers require a user gesture before audio; music
   won't start until the first keypress/click on web. Acceptable; verify in M6's
   web pass.
6. **Super Pick instant mining skips the `dig` loop** (only `RockBreak` fires) —
   it's instant by design.

---

## 12. Out of Scope (later milestones)

- Gem intro / `gem_pickup` sound (M8 story).
- Map editor audio, if any (M7).
- Final audio polish / mixing (M8).
