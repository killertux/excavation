//! Minimal UI drawing helpers over the atlas UI sprites (keyboard-only menus).
//!
//! This is intentionally small — no full 9-slice framework, just the helpers the
//! menu/app drawing code needs: buttons, panels, and volume sliders. The 9-slice
//! rendering keeps the stretched buttons/panels crisp at any width.

use macroquad::prelude::*;

use crate::assets::Assets;

/// The visual state of a button/slider (normal / hover / pressed / disabled).
///
/// `Pressed` is used only when a mouse/menu can also depress a button; the
/// keyboard-only menus (`M5`) use `Hover` for the current selection, so `Pressed`
/// is currently unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ButtonState {
    Normal,
    Hover,
    Pressed,
    Disabled,
}

impl ButtonState {
    /// The atlas frame index for this state.
    fn frame(self) -> usize {
        match self {
            ButtonState::Normal => 0,
            ButtonState::Hover => 1,
            ButtonState::Pressed => 2,
            ButtonState::Disabled => 3,
        }
    }
}

/// Draw one 9-slice region: the source sub-region `src` of `tex` stretched to the
/// target `dst`.
fn draw_slice(tex: &Texture2D, src: Rect, dst: Rect) {
    draw_texture_ex(
        tex,
        dst.x,
        dst.y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(Vec2::new(dst.w, dst.h)),
            source: Some(src),
            ..Default::default()
        },
    );
}

/// Render `tex` as a 9-slice into `rect`, given the border widths
/// `[left, right, top, bottom]` in source pixels.
fn draw_nine_slice(tex: &Texture2D, rect: Rect, borders: [f32; 4]) {
    let [bl, br, bt, bb] = borders;
    let tw = tex.width();
    let th = tex.height();
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);

    // Center/side region sizes (clamped so a tiny rect never goes negative or 0).
    let cm = (w - bl - br).max(0.0);
    let cn = (h - bt - bb).max(0.0);
    let sm = (tw - bl - br).max(1.0);
    let sn = (th - bt - bb).max(1.0);

    // Corners (source regions are exactly the border widths).
    draw_slice(tex, Rect::new(0.0, 0.0, bl, bt), Rect::new(x, y, bl, bt));
    draw_slice(tex, Rect::new(tw - br, 0.0, br, bt), Rect::new(x + w - br, y, br, bt));
    draw_slice(tex, Rect::new(0.0, th - bb, bl, bb), Rect::new(x, y + h - bb, bl, bb));
    draw_slice(tex, Rect::new(tw - br, th - bb, br, bb), Rect::new(x + w - br, y + h - bb, br, bb));
    // Horizontal edges (stretch the source middle horizontally).
    draw_slice(tex, Rect::new(bl, 0.0, sm, bt), Rect::new(x + bl, y, cm, bt));
    draw_slice(tex, Rect::new(bl, th - bb, sm, bb), Rect::new(x + bl, y + h - bb, cm, bb));
    // Vertical edges (stretch the source middle vertically).
    draw_slice(tex, Rect::new(0.0, bt, bl, sn), Rect::new(x, y + bt, bl, cn));
    draw_slice(tex, Rect::new(tw - br, bt, br, sn), Rect::new(x + w - br, y + bt, br, cn));
    // Center.
    draw_slice(tex, Rect::new(bl, bt, sm, sn), Rect::new(x + bl, y + bt, cm, cn));
}

/// Draw a button (a 48×16 sprite whose 9-slice center is 40×8) into `rect`.
pub fn draw_button(assets: &Assets, rect: Rect, state: ButtonState) {
    draw_nine_slice(assets.ui_button(state.frame()), rect, [4.0, 4.0, 4.0, 4.0]);
}

/// Draw a GUI panel (a 64×48 sprite whose 9-slice center is 44×28) into `rect`.
pub fn draw_panel(assets: &Assets, rect: Rect) {
    draw_nine_slice(assets.ui_panel(), rect, [10.0, 10.0, 10.0, 10.0]);
}

/// Draw a volume slider: a track (adjustable-bar) into `rect` with a scrollbar
/// knob at `value` (0.0..=1.0).
pub fn draw_slider(assets: &Assets, rect: Rect, value: f32, state: ButtonState) {
    // Track: the adjustable bar, 9-sliced (its centre is 78x6).
    draw_nine_slice(assets.ui_slider(state.frame()), rect, [9.0, 9.0, 9.0, 9.0]);
    let knob = assets.ui_scroll();
    let t = value.clamp(0.0, 1.0);
    let kw = rect.h; // square knob
    let inset = 2.0;
    let knob_x = rect.x + inset + t * (rect.w - 2.0 * inset - kw);
    draw_texture_ex(
        knob,
        knob_x,
        rect.y + (rect.h - kw) / 2.0,
        WHITE,
        DrawTextureParams {
            dest_size: Some(Vec2::new(kw, kw)),
            ..Default::default()
        },
    );
}
