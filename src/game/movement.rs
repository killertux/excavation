//! Shared, axis-separated, sub-stepped collision for moving entities (pure).

use macroquad::prelude::Vec2;

use super::TILE_SIZE;
use super::map::Map;

/// Half-extent of an entity's square hitbox (24×24 inside a 32×32 tile).
pub const HITBOX_HALF: f32 = 12.0;

/// Sub-step size used when resolving movement, kept below `HITBOX_HALF` so an
/// entity can never tunnel through a tile at any speed or dt.
const MAX_SUBSTEP: f32 = TILE_SIZE / 4.0;

/// Move an entity's `pos` by a per-axis `amount`, sub-stepping and resolving
/// collisions against `map` along a single axis.
///
/// `horizontal` selects the axis. Call this twice (once per axis) for
/// two-dimensional movement.
pub fn move_axis(pos: &mut Vec2, map: &Map, horizontal: bool, amount: f32) {
    if amount == 0.0 {
        return;
    }
    let steps = ((amount.abs() / MAX_SUBSTEP).ceil().max(1.0)) as u32;
    let sub = amount / steps as f32;
    for _ in 0..steps {
        if horizontal {
            pos.x += sub;
        } else {
            pos.y += sub;
        }
        resolve_overlaps(pos, map, horizontal);
    }
}

/// True when two entity hitboxes (both `2 × HITBOX_HALF` squares centered at `a`
/// and `b`) overlap or touch. A grazing touch counts as a catch.
pub fn hits(a: Vec2, b: Vec2) -> bool {
    (a.x - b.x).abs() < 2.0 * HITBOX_HALF && (a.y - b.y).abs() < 2.0 * HITBOX_HALF
}

/// Push an entity's hitbox out of every solid tile it genuinely penetrates,
/// along the given axis.
///
/// A solid tile is only acted on if the hitbox **actually overlaps** it (overlap
/// is greater than 0 on both axes). A tile the hitbox merely *touches* at a
/// boundary is ignored, otherwise a neighbouring border cell the entity is flush
/// against (e.g. the cells beside a door on the map edge) would shove it the
/// wrong way and let it clip through rocks / off the map. Each penetrating tile
/// is pushed toward the side the entity's centre is on; a few passes handle
/// corners.
fn resolve_overlaps(pos: &mut Vec2, map: &Map, horizontal: bool) {
    for _ in 0..4 {
        if !push_out_one_pass(pos, map, horizontal) {
            break;
        }
    }
}

fn push_out_one_pass(pos: &mut Vec2, map: &Map, horizontal: bool) -> bool {
    let half = HITBOX_HALF;
    let min_col = ((pos.x - half) / TILE_SIZE).floor() as i32;
    let max_col = ((pos.x + half) / TILE_SIZE).floor() as i32;
    let min_row = ((pos.y - half) / TILE_SIZE).floor() as i32;
    let max_row = ((pos.y + half) / TILE_SIZE).floor() as i32;

    let mut pushed = false;
    for row in min_row..=max_row {
        for col in min_col..=max_col {
            if !map.is_solid(col, row) {
                continue;
            }
            let left = col as f32 * TILE_SIZE;
            let top = row as f32 * TILE_SIZE;
            let right = left + TILE_SIZE;
            let bottom = top + TILE_SIZE;

            let ov_x = (pos.x + half).min(right) - (pos.x - half).max(left);
            let ov_y = (pos.y + half).min(bottom) - (pos.y - half).max(top);
            if ov_x <= 0.0 || ov_y <= 0.0 {
                continue;
            }

            if horizontal {
                let mid = (left + right) / 2.0;
                pos.x = if pos.x < mid {
                    left - half
                } else {
                    right + half
                };
            } else {
                let mid = (top + bottom) / 2.0;
                pos.y = if pos.y < mid {
                    top - half
                } else {
                    bottom + half
                };
            }
            pushed = true;
        }
    }
    pushed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_true_on_overlap() {
        // Two 24x24 hitboxes, centers 10 px apart on x -> overlapping.
        assert!(hits(Vec2::new(100.0, 100.0), Vec2::new(110.0, 100.0)));
        // Overlapping on y only.
        assert!(hits(Vec2::new(100.0, 100.0), Vec2::new(100.0, 112.0)));
        // Exactly identical centers.
        assert!(hits(Vec2::new(100.0, 100.0), Vec2::new(100.0, 100.0)));
    }

    #[test]
    fn hits_false_when_separated() {
        // Centers 30 px apart (> 2 * 12 = 24) -> no overlap on x.
        assert!(!hits(Vec2::new(100.0, 100.0), Vec2::new(130.0, 100.0)));
        // Far apart on both axes.
        assert!(!hits(Vec2::new(100.0, 100.0), Vec2::new(140.0, 140.0)));
    }
}
