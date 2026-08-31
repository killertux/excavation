//! Scrolling camera: follows the player and clamps to the map bounds (pure).

use macroquad::prelude::Vec2;

/// A camera that centers on a target and clamps to a map.
///
/// `pos` is the top-left corner of the visible world region, in world pixels.
/// `zoom` scales world units to screen pixels (a `zoom` of 2.0 renders a 16 px
/// tile as 32 px on screen).
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub pos: Vec2,
    pub zoom: f32,
}

impl Camera {
    pub fn new(zoom: f32) -> Self {
        Camera {
            pos: Vec2::ZERO,
            zoom,
        }
    }

    /// Center on `target` (world px) and clamp to a map of `map_w`×`map_h` world
    /// px given a viewport of `view_w`×`view_h` screen px.
    ///
    /// If the map is smaller than the view on an axis, the map is centered on
    /// that axis (no single-sided empty space).
    pub fn follow(&mut self, target: Vec2, map_w: f32, map_h: f32, view_w: f32, view_h: f32) {
        // Visible world size for the given viewport and zoom.
        let view_world_w = view_w / self.zoom;
        let view_world_h = view_h / self.zoom;

        let half_w = view_world_w / 2.0;
        let half_h = view_world_h / 2.0;

        let x = if view_world_w >= map_w {
            // Map narrower than the view: center it.
            (map_w - view_world_w) / 2.0
        } else {
            (target.x - half_w).clamp(0.0, map_w - view_world_w)
        };

        let y = if view_world_h >= map_h {
            (map_h - view_world_h) / 2.0
        } else {
            (target.y - half_h).clamp(0.0, map_h - view_world_h)
        };

        self.pos = Vec2::new(x, y);
    }

    /// Map a world-space point to screen-space (relative to the view origin).
    ///
    /// Used by the camera tests (and available for input/UI coordinate work),
    /// but not yet called from the runtime binary.
    #[allow(dead_code)]
    pub fn world_to_screen(&self, p: Vec2) -> Vec2 {
        (p - self.pos) * self.zoom
    }

    /// Map a screen-space point (relative to the view origin) to world-space.
    ///
    /// Used by the camera tests (and available for input/UI coordinate work),
    /// but not yet called from the runtime binary.
    #[allow(dead_code)]
    pub fn screen_to_world(&self, p: Vec2) -> Vec2 {
        p / self.zoom + self.pos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 0.001
    }

    #[test]
    fn camera_centers_on_player() {
        let mut cam = Camera::new(2.0);
        // Player well inside the map so the camera is not clamped.
        cam.follow(Vec2::new(500.0, 400.0), 1000.0, 1000.0, 1280.0, 720.0);
        // Visible world size = 640x360; centered on (500,400).
        let expected = Vec2::new(500.0 - 640.0 / 2.0, 400.0 - 360.0 / 2.0);
        assert!(
            approx(cam.pos, expected),
            "got {:?} expected {:?}",
            cam.pos,
            expected
        );
        // The player maps to the screen center.
        let screen = cam.world_to_screen(Vec2::new(500.0, 400.0));
        assert!(approx(screen, Vec2::new(640.0, 360.0)));
    }

    #[test]
    fn camera_clamps_to_map_bounds() {
        let mut cam = Camera::new(2.0);
        // Player near the top-left corner of a large map.
        cam.follow(Vec2::new(10.0, 10.0), 1000.0, 1000.0, 1280.0, 720.0);
        // view_world = 640x360; clamped to top-left.
        assert_eq!(cam.pos, Vec2::new(0.0, 0.0));

        // Player near the bottom-right corner.
        cam.follow(Vec2::new(990.0, 990.0), 1000.0, 1000.0, 1280.0, 720.0);
        assert_eq!(cam.pos, Vec2::new(1000.0 - 640.0, 1000.0 - 360.0));
    }

    #[test]
    fn small_map_centers_without_gaps() {
        let mut cam = Camera::new(2.0);
        // Map is 200x200 world px; view is 640x360 world px (larger).
        cam.follow(Vec2::new(100.0, 100.0), 200.0, 200.0, 1280.0, 720.0);
        assert_eq!(
            cam.pos,
            Vec2::new((200.0 - 640.0) / 2.0, (200.0 - 360.0) / 2.0)
        );
    }

    #[test]
    fn world_to_screen_and_back_round_trip() {
        let mut cam = Camera::new(2.0);
        cam.follow(Vec2::new(500.0, 400.0), 1000.0, 800.0, 1280.0, 720.0);
        let world = Vec2::new(123.0, 456.0);
        let screen = cam.world_to_screen(world);
        let back = cam.screen_to_world(screen);
        assert!(approx(back, world));
        // world_to_screen = (p - pos) * zoom
        let expected = (world - cam.pos) * 2.0;
        assert!(approx(screen, expected));
    }
}
