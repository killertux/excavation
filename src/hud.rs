//! The in-game HUD: lives, gold, level, elapsed time, consumable counts, and the
//! active-effect timer. This is a thin drawing layer over the atlas icons + text
//! (the `App` resolves what to show; it stays independent of the game state).

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::assets::ids::IconId;
use crate::game::consumables::ConsumableKind;
use crate::game::run::Run;

const GOLD: Color = Color::new(1.0, 0.84, 0.0, 1.0);

/// Draw the HUD over the scene.
///
/// - `gold_display` is the gold figure to show (live in-attempt gold during play,
///   banked total on outcome screens — chosen by the caller).
/// - `show_timer` gates the active-effect timer (only meaningful while Playing).
/// - `view_w`/`view_h` are the pixel dimensions of the view (screen or RT).
#[allow(clippy::too_many_arguments)]
pub fn draw_hud(
    assets: &Assets,
    run: &Run,
    gold_display: u32,
    show_timer: bool,
    view_w: f32,
    _view_h: f32,
) {
    let y = 30.0;

    // Lives as a row of heart icons.
    for i in 0..run.lives {
        let tex = assets.icon(IconId::Heart);
        draw_texture_ex(
            tex,
            14.0 + i as f32 * 24.0,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(20.0, 20.0)),
                ..Default::default()
            },
        );
    }

    // Left column: gold, level, elapsed time.
    draw_text(format!("Gold: {gold_display}"), 14.0, 66.0, 24.0, GOLD);
    draw_text(
        format!("Level: {}/{}", run.level_index() + 1, run.level_count()),
        14.0,
        94.0,
        24.0,
        WHITE,
    );
    draw_text(
        format!("Time: {}", fmt_time(run.level.elapsed())),
        14.0,
        122.0,
        22.0,
        LIGHTGRAY,
    );

    // Right column: consumable counts (icon + xN).
    let cx = view_w - 230.0;
    let sp = run.consumables.count(ConsumableKind::SuperPick);
    draw_icon(assets, IconId::SuperPick, cx, y - 2.0);
    draw_text(format!("x{sp}"), cx + 28.0, y + 16.0, 20.0, WHITE);

    let ss = run.consumables.count(ConsumableKind::StickySmell);
    draw_icon(assets, IconId::StickySmell, cx, y + 28.0);
    draw_text(format!("x{ss}"), cx + 28.0, y + 46.0, 20.0, WHITE);

    // Active-effect timer (only meaningful while actually playing).
    if show_timer && let Some(e) = run.level.active_effect {
        let name = match e.kind {
            ConsumableKind::SuperPick => "Super Pick",
            ConsumableKind::StickySmell => "Sticky Smell",
        };
        draw_text(
            format!("{name}: {:.1}s", e.remaining.max(0.0)),
            14.0,
            150.0,
            20.0,
            YELLOW,
        );
    }
}

/// Draw a 24×24 HUD icon at `(x, y)` scaled to 22 px.
fn draw_icon(assets: &Assets, id: IconId, x: f32, y: f32) {
    let tex = assets.icon(id);
    draw_texture_ex(
        tex,
        x,
        y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(Vec2::new(22.0, 22.0)),
            ..Default::default()
        },
    );
}

/// Format `secs` as `mm:ss` (flooring to whole seconds).
fn fmt_time(secs: f32) -> String {
    let total = secs.max(0.0) as u32;
    format!("{:02}:{:02}", total / 60, total % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_formats_as_mm_ss() {
        assert_eq!(fmt_time(0.0), "00:00");
        assert_eq!(fmt_time(59.4), "00:59");
        assert_eq!(fmt_time(60.0), "01:00");
        assert_eq!(fmt_time(125.0), "02:05");
        assert_eq!(fmt_time(-3.0), "00:00");
    }
}
