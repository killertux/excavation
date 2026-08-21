---
name: visual-test
description: "Use when visually verifying or checking what the Excavation game actually renders (or a macroquad/2D game's renderer generally). Keywords: visual test, visual verification, screenshot, --screenshot, render check, headless screenshot, macroquad render, game screenshot, verify rendering, check the result, 可视化测试, 截图, 渲染验证, 看效果"
user-invocable: true
---

# Visual Test (Excavation)

How to see what the game actually draws — headlessly or in a browser — and what to
look for. Use this whenever you need to "check the result" of a milestone, or confirm
a render change didn't break the visuals. Visuals are the one thing unit tests cannot
cover; this is the manual/scripted pass that does.

## `--screenshot` flag (desktop, headless)

The game renders a few frames into an offscreen `RenderTarget`, saves a PNG, and exits.
Because it renders to a texture (not the live screen buffer), it works even without a
window/display compositing — it falls back to software GL. This is the fastest way to
visually check a build in an automated/headless setting:

```bash
cargo run -- --screenshot out.png
```

- Saves to `out.png` relative to the CWD (or use an absolute path).
- Desktop-only: the flag is `#[cfg(not(target_arch = "wasm32"))]`-gated, and macroquad's
  `Image::export_png` is not supported on web.
- Capture is taken after ~3 frames. It is a single still (idle pose) — do not use it to
  verify walk-animation timing (that's covered by unit tests).
- If no file appears, confirm the process printed `screenshot saved to ...`. If it exited
  silently before frame 3, an asset failed to load (e.g. a missing/mis-detected sheet).

### What to look for in the screenshot

- **Map tiles** render from `assets/images/tiles/terrain_atlas.png`: the border ring,
  excavated (brown) floor, rock clusters, wall cluster, the green exit door at the top,
  and the start door at the bottom.
- **Player sprite** is present at the spawn (the start door) and is not missing, black, or
  off-screen.
- The whole `placeholder_map()` is visible and **centered** (it is smaller than the
  viewport, so the camera centers it instead of scrolling).
- **Not** a flat single-colour field. If the screen is one solid block of colour, the
  camera transform is broken — see the zoom gotcha below.

## Camera zoom gotcha (macroquad)

macroquad's `Camera2D::zoom` is **`2 / visible_world_size`** (a clip-space scale), **not**
"screen px per world px". Passing a magnification directly (e.g. `zoom = 2.0`) collapses the
visible world to ~1px, so a single tile fills the screen — symptom: a flat, featureless
colour field. Convert properly (see `mq_zoom` in `src/app.rs`):

```
zoom = (2.0 * mag / view_w, 2.0 * mag / view_h)      # view = screen size in px
```

When rendering to a `RenderTarget`, the Y component must be **negated** (macroquad flips the
Y scale for render targets internally, but not for the default screen). The conversion is
unit-tested in `src/app.rs::tests`, so a change that regresses it is caught.

## Verifying in the browser (web / wasm)

A headless `--screenshot` of the wasm build is unreliable — the async boot + WebGL context
often aren't ready when the capture happens. Instead, confirm the wasm **boots and runs** by
serving `web/` and checking the server access log for the asset fetches `Assets::load` issues
after `main()` runs:

```bash
python3 -m http.server 8099 --directory web > /tmp/http.log 2>&1 &
# load the page in a browser (or headless Firefox), then:
grep -oE '"GET /assets/images/[^"]+"' /tmp/http.log
# Expect "terrain_atlas.png" and "player_sheet.png" to appear.
```

If those two sprite-sheet requests appear in the log, the wasm booted and started asset
loading. Rebuild/publish the web bundle with:

```bash
./build-web.sh
```

## Manual pass checklist (desktop)

Run `cargo run` and verify by eye/feel:

- **Movement** — WASD/arrow keys move the player in 8 directions; diagonals are not faster.
- **Collision** — the player stops at rocks, walls, and the border (no walking through).
- **Animation** — idle vs walk frames while moving.
- **Camera** — follows the player, or centers a small map, and never shows empty space past
  the map edges.

## Related

- `src/main.rs` — `--screenshot` arg parsing + `RenderTarget` capture.
- `src/app.rs` — `render_to`, `draw_scene`, `scene_camera`, `mq_zoom` (+ tests).
- `src/game/camera.rs` — pure follow/clamp and world↔screen math.
- `plans/m1-foundation-asset-pipeline-movement.md` §15 — the verification / implementation
  notes that this skill documents.
