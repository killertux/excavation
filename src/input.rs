//! Input: gather a move vector and the mine action each frame.
//!
//! The move vector is raw (unnormalized); the player normalizes it internally so
//! diagonals are not faster. `Input` is shaped so a gamepad source can be added
//! later without changing the update loop.
//!
//! ## Gamepad
//!
//! macroquad 0.4.16 (and its miniquad 0.4.11) expose **no** gamepad API (the
//! macroquad input module's own header notes "gamepads soon"). Gamepad input is
//! therefore deferred until a backend is available.

use macroquad::prelude::*;

/// A snapshot of player-relevant input for one frame.
#[derive(Debug, Clone, Copy)]
pub struct Input {
    /// Movement intent (WASD/arrows), unnormalized. Zero when idle.
    pub move_: Vec2,
    /// Edge-triggered: the Super Pick consumable was activated this frame (Key1
    /// / Q). The run spends a consumable if one is owned.
    pub use_super_pick: bool,
    /// Edge-triggered: the Sticky Smell consumable was activated this frame
    /// (Key2 / E).
    pub use_sticky_smell: bool,
}

/// Collect the current frame's input.
pub fn collect() -> Input {
    let mut v = Vec2::ZERO;

    if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) {
        v.y -= 1.0;
    }
    if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) {
        v.y += 1.0;
    }
    if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
        v.x -= 1.0;
    }
    if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
        v.x += 1.0;
    }

    let use_super_pick = is_key_pressed(KeyCode::Key1) || is_key_pressed(KeyCode::Q);
    let use_sticky_smell = is_key_pressed(KeyCode::Key2) || is_key_pressed(KeyCode::E);

    Input {
        move_: v,
        use_super_pick,
        use_sticky_smell,
    }
}
