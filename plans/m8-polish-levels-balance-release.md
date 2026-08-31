# M8 — Polish, 10 Levels, Balance & Release (Detailed Plan)

> Scope: milestone **M8** from [`PLAN.md`](../PLAN.md).
> **Working binary =** the complete, polished game — all 10 balanced levels playable
> end-to-end on desktop and web, full test suite green, Definition-of-Done
> (`REQUIREMENTS.md` §17) satisfied.

This milestone is different from M1–M7: there are almost no *new mechanics* to
add. It is **content authoring + balancing + a story intro + release validation**.

---

## 0. Current Baseline (post-M7)

- Feature-complete: mining, generation, beasts, gold/score/shop/upgrades/
  consumables, menus/HUD/save/settings, audio, map editor. **227 tests.**
- Only **2 maps** exist (`level01.toml`, `level02.toml`); `game.toml`
  `[map_order]` lists just those two.
- No story intro screen yet (the "gem" story is §1 / §19 non-goals: a static
  text screen is enough). The `sfx_gem_pickup.wav` is still unused.
- No `README.md`.

---

## 1. Outcome (release checklist)

- [ ] 10 levels authored and listed in `map_order`, each harder than the last and
      each generating a solvable map.
- [ ] `game.toml` tuned (speeds, mining times, costs, durations, score).
- [ ] A static story intro (the gem) plays before a fresh run; `gem_pickup` SFX wired.
- [ ] `REQUIREMENTS.md` §17 Definition-of-Done is fully verified.
- [ ] Desktop `cargo run` and web build both validated end-to-end.
- [ ] All tests green; `README.md` documents build/run/controls/tools.

---

## 2. Content: 10 Levels

Author `assets/maps/level03.toml` … `level10.toml` (8 new files). Difficulty
increases by **more unmineable rocks, more/faster beasts, bigger maps, more
structures**. Vary the start/exit positions (they are per-map by design).

Reference curve (tune by playtesting — the exact numbers are a starting point):

| Lvl | size | unmineable | gold | beasts | speed × | mine × | structures |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 30×20 | 20 | 8 | 1 | 1.0 | 1.0 | small |
| 2 | 30×20 | 60 | 8 | 1 | 1.0 | 1.0 | medium |
| 3 | 30×20 | 90 | 10 | 1 | 1.05 | 0.95 | medium |
| 4 | 32×22 | 130 | 10 | 1 | 1.10 | 0.90 | large |
| 5 | 32×22 | 170 | 12 | 2 | 1.10 | 0.90 | large |
| 6 | 34×24 | 220 | 12 | 2 | 1.15 | 0.85 | large |
| 7 | 34×24 | 270 | 14 | 2 | 1.20 | 0.80 | large |
| 8 | 36×26 | 330 | 14 | 3 | 1.20 | 0.80 | large |
| 9 | 36×26 | 390 | 16 | 3 | 1.25 | 0.75 | large |
| 10 | 38×28 | 460 | 18 | 3 | 1.30 | 0.70 | large |

Rules when authoring (the map editor enforces these; use it — `--editor <path>`):
- `start`/`exit` on the border and distinct.
- `gold_count + unmineable_count + interior structures ≤ interior` (else invalid).
- Keep a seed on a few levels for reproducible testing; leave others seedless for
  "random each run".
- Each level must actually generate + be solvable (run the `generation` test path).

Then update `game.toml`:

```toml
[map_order]
files = ["assets/maps/level01.toml", /* … */ "assets/maps/level10.toml"]
```

---

## 3. Balance (`assets/game.toml`)

Tune the existing values by playing the full run a few times:

- **Player** `base_speed` (240) and `base_mining_time` (0.8) vs the beast curve —
  the player should usually outrun but always feel pressure.
- **Upgrade/consumable costs** so the shop economy works across 10 levels (gold
  income from §2 should buy a meaningful mix, not everything trivially).
- **Score** `par_time` per average level length (maps grow, so par time may need
  to rise with level — consider a per-level `par_time` if the flat value feels
  wrong; otherwise keep the flat `[score]` and tune it).
- **Lives** `starting_lives`/`max_lives` and `[lives].cost` for the difficulty.

No new config surface is required; this is a numbers pass.

---

## 4. Story Intro (gem)

- Add a `GameState::Intro` screen (text only, per §19 non-goals).
- Flow: **Main menu → Play (new game) → Intro → Playing (level 1)**. "Continue"
  and "Level Select" skip the intro.
- The intro shows the story in a few lines and "Press Enter to begin"; it plays
  `sfx_gem_pickup` once when shown (the "you found the gem" beat).
- Suggested copy (a static panel):
  > Deep in the excavation you found a priceless gem. The moment you lifted it,
  > the ceiling gave way — and the ancient lizard-beasts below woke. Dig your way
  > to the surface before they reach you. Not every rock will break…
- Reuse the `menu_background.png` / `title_logo.png` and a panel. No new art is
  required (an optional gem sprite can be generated via GameLab later, but a text
  panel satisfies the requirement).

---

## 5. Definition-of-Done Verification (`REQUIREMENTS.md` §17)

Walk the §17 checklist and fix any gaps. Expected status (verify, don't assume):

- [ ] Boots desktop + web.
- [ ] 10 maps load; each honors counts/multipliers and guarantees a path.
- [ ] Mining (timed, stops movement) + identical unmineable + Super Pick.
- [ ] Beast AI (charge → A\* → nearest-known-mineable → idle) + Sticky Smell.
- [ ] Catch → life → restart(new map); 0 lives → game over.
- [ ] Shop (5 items) fully TOML-configured.
- [ ] Gold hidden → dropped → collected → spent.
- [ ] Score at level end + running total persisted.
- [ ] All screens reachable (main, level select, pause, level complete, game over,
      victory, shop, settings).
- [ ] Save/load full progress (desktop file, web localStorage).
- [ ] Settings apply + persist.
- [ ] Map editor create/edit/validate/save.
- [ ] Music + SFX.
- [ ] Tests green + manual visual pass on both platforms.

---

## 6. Optional Polish (decide — see §9)

1. **`README.md`** — build/run instructions (desktop + `build-web.sh`), controls,
   `--editor` / `--screenshot` tools, level-authoring notes.
2. **Level-select replay anti-farming** — replaying an already-completed level
   currently re-banks gold/score. If desired, disable gold/score banking when the
   selected level is already behind the run's progress.
3. **Main-menu "Map Editor" entry** (`REQUIREMENTS.md` §10) — the editor is
   CLI-launched (§12); adding it to the menu needs App↔Editor hand-off. Leave as
   CLI-only unless explicitly wanted.
4. **Version bump** — `Cargo.toml` `0.1.0` → `1.0.0` for release.

---

## 7. Release Validation

- `cargo test` — all green.
- Desktop: `cargo run` — full 10-level playthrough (or a debug shortcut to jump
  levels), every screen, save/continue, settings, editor smoke test.
- Web: `./build-web.sh`, serve `web/`, confirm boot + a level + save/load across
  reload (autoplay requires a first input).
- Screenshots via `--screenshot` (and `DSH_SCREEN` for menus) for a visual record.
- Confirm the §1 release checklist.

---

## 8. Tests (unit)

- Level-authoring guard: a test that loads **every** `map_order` file, validates
  it, generates it, and asserts `unmineable_count` + solvability (extend the
  existing `loads_and_generates_the_committed_sample_maps` test to iterate the
  whole `map_order`, or add a new `all_map_order_levels_generate` test).
- Intro flow: the `Menu` state machine's `NewGame` action and the intro skip
  rules (Continue/Level Select bypass) — logic-only.

---

## 9. Decisions & Risks (confirm if they differ)

1. **Difficulty curve numbers (§2) are a starting point** — final tuning is
   playtest-driven; the curve just needs to be monotonic.
2. **Story intro is text-only**, before level 1 on a new run; "Continue"/"Level
   Select" skip it.
3. **Optional polish items (§6) are opt-in** — I recommend at minimum (1) README
   and (4) version bump; (2) anti-farming and (3) menu editor entry are nice-to-have.
4. **No new art** is required this milestone (intro is text; the gem sprite is
   optional and can be generated later).
5. **`par_time` is a flat global** — if the longer maps make speed scoring unfair,
   a per-level `par_time` is a possible follow-up (not built now unless asked).

---

## 10. Task List (ordered)

- [ ] 1. Author `level03.toml` … `level10.toml` with the editor; vary start/exit.
- [ ] 2. Update `game.toml` `[map_order]` to all 10 files.
- [ ] 3. Add the `all_map_order_levels_generate` test; fix any level that fails.
- [ ] 4. Add the `GameState::Intro` story screen + gem SFX + flow wiring.
- [ ] 5. Balance `game.toml` (play the full run).
- [ ] 6. Write `README.md` (build/run/controls/tools).
- [ ] 7. (Optional) anti-farming + version bump.
- [ ] 8. Full release validation (§7) on desktop + web; complete §1 checklist.

---

## 11. Out of Scope / Future

- Animated cutscenes (explicitly a non-goal).
- Mobile/touch controls, gamepad (waiting on macroquad's gamepad API).
- Networked play.
