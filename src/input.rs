//! Input: gather a move intent from the keyboard.
//!
//! This returns a raw (unnormalized) direction vector; the player normalizes it
//! internally so diagonals are not faster. Movement is the only input in M1.
//!
//! ## Gamepad
//!
//! The M1 plan calls for gamepad movement, but macroquad 0.4.16 (and its
//! miniquad 0.4.11) expose **no** gamepad API (the macroquad input module's own
//! header notes "gamepads soon"). Gamepad input is therefore deferred until a
//! backend is available; the `move_intent` shape below is intentionally easy to
//! extend with a gamepad source (combine vectors, then dead-zone).

use macroquad::prelude::*;

/// Combine WASD/arrows (keyboard) into a single move vector.
pub fn move_intent() -> Vec2 {
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

    v
}
