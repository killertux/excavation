//! Player entity: continuous movement + grid-aware collision + timed mining
//! (pure, no rendering or input polling).

use macroquad::prelude::Vec2;

use super::map::{Map, Tile};
use super::mining::{self, Mining};
use super::movement;
use crate::assets::ids::{IDLE_FRAMES, MINE_FRAMES, PlayerMotion, WALK_FRAMES};

/// Seconds per idle-frame (two-frame breathing cycle, per atlas timing).
const IDLE_FRAME_TIME: f32 = 0.125;

/// Seconds per walk-frame (four-frame cycle, per atlas timing).
const WALK_FRAME_TIME: f32 = 0.125;

/// Seconds per mining-frame (raise/impact pickaxe, four-frame cycle).
const MINE_FRAME_TIME: f32 = 0.125;

/// Seconds of continuous contact with a mineable rock before mining begins.
const MINE_PUSH_TIME: f32 = 0.2;

#[derive(Debug, Clone)]
pub struct Player {
    /// Center of the hitbox, in world pixels.
    pub pos: Vec2,
    /// Walk speed, px/s (from `game.toml`).
    pub speed: f32,
    /// Current animation motion (idle/walk/mining). The direction comes from
    /// [`Player::facing`] at render time.
    pub motion: PlayerMotion,
    /// Facing direction (one of the 8 compass directions, normalized). Updated
    /// from the last non-zero move intent; stable while mining.
    pub facing: Vec2,
    /// In-progress mine, if any.
    pub mining: Option<Mining>,
    /// Super Pick active this frame: any rock (except unbreakable) is dug
    /// instantly. Set by `Level` each frame from the active consumable effect.
    pub super_pick: bool,
    walk_anim_timer: f32,
    idle_anim_timer: f32,
    /// How long the player has been continuously pushing into a mineable rock,
    /// plus the cell being pushed into. Reset whenever the contact target
    /// changes or contact is lost.
    push_timer: f32,
    push_target: Option<(i32, i32)>,
    /// The grid cell excavated this frame, if any (taken by `Level` to drop gold).
    last_excavated: Option<(i32, i32)>,
}

impl Player {
    pub fn new(pos: Vec2, speed: f32) -> Self {
        Player {
            pos,
            speed,
            motion: PlayerMotion::Idle(0),
            facing: Vec2::new(0.0, 1.0),
            mining: None,
            super_pick: false,
            walk_anim_timer: 0.0,
            idle_anim_timer: 0.0,
            push_timer: 0.0,
            push_target: None,
            last_excavated: None,
        }
    }

    /// Advance the player by one frame.
    ///
    /// `intent` is an unnormalized move direction. Mining is triggered by
    /// walking into a mineable rock: while the player is flush against (and
    /// pressing into) the same mineable cell continuously for
    /// [`MINE_PUSH_TIME`] seconds, mining begins. An in-progress mine continues
    /// **only while the player keeps pressing into that same rock**; releasing
    /// the direction key (or pushing elsewhere) cancels it and discards the
    /// progress — rocks have no "life", so the player must mine it again from
    /// scratch. When `progress` reaches `mining_time` seconds the cell becomes
    /// `Dirt`. If nothing is being mined the player moves normally.
    ///
    /// While [`Player::super_pick`] is set, mining is instant and also works on
    /// `Unmineable` rocks (still never `Unbreakable`).
    ///
    /// The excavated cell (if a rock broke this frame) is later read via
    /// [`Player::take_excavated`].
    pub fn update(&mut self, intent: Vec2, map: &mut Map, mining_time: f32, dt: f32) {
        if intent.length_squared() > 0.0 {
            self.facing = intent.normalize();
        }
        // The active push direction — zero when the key isn't held, so contact
        // is truly "pressing into" rather than the stale facing.
        let push_dir = if intent.length_squared() > 0.0 {
            intent.normalize()
        } else {
            Vec2::ZERO
        };

        // Cancel an in-progress mine once the player stops pushing into it.
        if let Some(m) = self.mining {
            let still_pushing = if self.super_pick {
                mining::pushed_target_ex(self.pos, push_dir, map, movement::HITBOX_HALF, true)
                    == Some(m.target)
            } else {
                mining::pushed_target(self.pos, push_dir, map, movement::HITBOX_HALF)
                    == Some(m.target)
            };
            if !still_pushing {
                self.mining = None;
            }
        }

        // If still mining, keep digging the same target (movement ignored).
        if let Some(m) = &self.mining {
            let target = m.target;
            self.mine(target, map, mining_time, dt);
            return;
        }

        // Super Pick: break a rock the frame the player pushes into it.
        if self.super_pick
            && let Some(t) =
                mining::pushed_target_ex(self.pos, push_dir, map, movement::HITBOX_HALF, true)
        {
            self.mine(t, map, mining_time, dt);
            return;
        }

        // Otherwise walk, then see if we're pressing into a mineable rock.
        self.move_free(intent, map, dt);

        // Track continuous contact with a mineable cell; start mining after the
        // contact interval elapses.
        let contact = mining::pushed_target(self.pos, push_dir, map, movement::HITBOX_HALF);
        match contact {
            Some(t) => {
                if self.push_target == Some(t) {
                    self.push_timer += dt;
                } else {
                    self.push_target = Some(t);
                    self.push_timer = dt;
                }
                if self.push_timer >= MINE_PUSH_TIME {
                    self.mine(t, map, mining_time, dt);
                }
            }
            None => {
                self.push_timer = 0.0;
                self.push_target = None;
            }
        }
    }

    /// The grid cell excavated this frame, if a rock just broke. Consumes the
    /// value (one call per frame).
    pub fn take_excavated(&mut self) -> Option<(i32, i32)> {
        self.last_excavated.take()
    }

    /// Whether the player is currently walking (a walk animation is active),
    /// i.e. it moved this frame. Used to tick the footstep sound.
    pub fn is_walking(&self) -> bool {
        matches!(self.motion, PlayerMotion::Walk(_))
    }

    /// Advance mining of `target` (or begin it), ignoring movement. Under
    /// [`Player::super_pick`] the mine completes instantly. On completion the
    /// cell becomes `Dirt` and is recorded in `last_excavated`.
    fn mine(&mut self, target: (i32, i32), map: &mut Map, mining_time: f32, dt: f32) {
        let continuing = self.mining.map(|m| m.target) == Some(target);
        let progress = if self.super_pick {
            mining_time
        } else if continuing {
            self.mining.as_ref().unwrap().progress + dt
        } else {
            0.0
        };
        self.mining = Some(Mining { target, progress });
        self.motion =
            PlayerMotion::Mine(((progress / MINE_FRAME_TIME) as usize % MINE_FRAMES) as u8);
        self.walk_anim_timer = 0.0;
        self.idle_anim_timer = 0.0;
        self.push_timer = 0.0;
        self.push_target = Some(target);

        if progress >= mining_time {
            map.set_tile(target.0, target.1, Tile::Dirt);
            self.mining = None;
            self.last_excavated = Some(target);
        }
    }

    /// Move without mining: apply `intent`, resolve collision, run the anim.
    fn move_free(&mut self, intent: Vec2, map: &Map, dt: f32) {
        let moving = intent.length_squared() > 0.0;
        if moving {
            let step = intent.normalize() * (self.speed * dt);
            movement::move_axis(&mut self.pos, map, true, step.x);
            movement::move_axis(&mut self.pos, map, false, step.y);

            // Four-frame walk animation.
            self.walk_anim_timer += dt;
            let frame = (self.walk_anim_timer / WALK_FRAME_TIME) as usize % WALK_FRAMES;
            self.motion = PlayerMotion::Walk(frame as u8);
            self.idle_anim_timer = 0.0;
        } else {
            // Two-frame idle breathing animation.
            self.idle_anim_timer += dt;
            let frame = (self.idle_anim_timer / IDLE_FRAME_TIME) as usize % IDLE_FRAMES;
            self.motion = PlayerMotion::Idle(frame as u8);
            self.walk_anim_timer = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::movement::HITBOX_HALF;
    use super::*;
    use crate::game::TILE_SIZE;

    const TEST_SPEED: f32 = 120.0;

    /// 5x5 grid with a solid rock at (3,2) and a dirt interior.
    fn test_map() -> Map {
        let mut m = Map::new(5, 5, (0, 2), (4, 2));
        for y in 1..4 {
            for x in 1..4 {
                m.tiles[y * 5 + x] = Tile::Dirt;
            }
        }
        m.tiles[2 * 5 + 3] = Tile::Mineable; // rock to the right of center
        m
    }

    fn open_map() -> Map {
        let mut map = Map::new(5, 5, (0, 2), (4, 2));
        for y in 1..4 {
            for x in 1..4 {
                map.tiles[y * 5 + x] = Tile::Dirt;
            }
        }
        map
    }

    fn center_of(cell: (i32, i32)) -> Vec2 {
        Vec2::new(
            cell.0 as f32 * TILE_SIZE + TILE_SIZE / 2.0,
            cell.1 as f32 * TILE_SIZE + TILE_SIZE / 2.0,
        )
    }

    /// Player centred in cell (2,2) but flush against its east wall, i.e. the
    /// player's right edge touches the shared boundary with cell (3,2).
    fn flush_east_of(cell: (i32, i32)) -> Vec2 {
        Vec2::new(
            cell.0 as f32 * TILE_SIZE - HITBOX_HALF,
            cell.1 as f32 * TILE_SIZE + TILE_SIZE / 2.0,
        )
    }

    fn player_at_center() -> Player {
        Player::new(center_of((2, 2)), TEST_SPEED)
    }

    #[test]
    fn moving_into_solid_tile_is_blocked() {
        let mut map = test_map();
        let mut p = player_at_center();
        // One small step to the right against the rock; the contact interval has
        // not elapsed (dt < 0.2s) so the player is blocked, not yet mining.
        p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, 0.05);
        // The right edge must stop exactly at the rock's left edge (x = 3*32).
        assert!((p.pos.x + HITBOX_HALF - 3.0 * TILE_SIZE).abs() < 0.001);
        assert!(p.pos.x <= 3.0 * TILE_SIZE - HITBOX_HALF + 0.001);
        assert!(p.mining.is_none(), "no mining after one short step");
    }

    #[test]
    fn moving_through_open_dirt_is_allowed() {
        let mut map = open_map();
        let mut p = Player::new(center_of((2, 2)), TEST_SPEED);
        let before = p.pos;
        p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, 1.0 / 60.0);
        assert!(p.pos.x > before.x, "should move through open floor");
        assert!((p.pos.x - before.x - TEST_SPEED * (1.0 / 60.0)).abs() < 0.001);
    }

    #[test]
    fn diagonal_input_is_normalized() {
        let dt = 1.0 / 60.0;
        let (mut map1, mut map2) = (test_map(), test_map());
        let (mut p1, mut p2) = (player_at_center(), player_at_center());
        let (b1, b2) = (p1.pos, p2.pos);
        p1.update(Vec2::new(1.0, 0.0), &mut map1, 0.8, dt);
        p2.update(Vec2::new(1.0, 1.0), &mut map2, 0.8, dt);
        let d1 = (p1.pos - b1).length();
        let d2 = (p2.pos - b2).length();
        assert!((d1 - d2).abs() < 0.001, "diagonal must not be faster");
        assert!((d1 - TEST_SPEED * dt).abs() < 0.001);
    }

    #[test]
    fn walk_animation_cycles_while_moving_and_idles() {
        let mut map = open_map();
        let mut p = player_at_center();
        let dt = 1.0 / 60.0;
        let (mut saw0, mut saw3) = (false, false);
        for _ in 0..120 {
            p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, dt);
            match p.motion {
                PlayerMotion::Walk(_) => {
                    saw0 |= p.motion == PlayerMotion::Walk(0);
                    saw3 |= p.motion == PlayerMotion::Walk(3);
                }
                other => panic!("expected a walk phase, got {other:?}"),
            }
        }
        assert!(saw0 && saw3, "walk cycle must visit frame 0 and frame 3");
        for _ in 0..5 {
            p.update(Vec2::ZERO, &mut map, 0.8, dt);
        }
        assert!(
            matches!(p.motion, PlayerMotion::Idle(_)),
            "should idle when still"
        );
    }

    #[test]
    fn idle_animation_advances_whilst_stationary() {
        let mut map = open_map();
        let mut p = player_at_center();
        let dt = 1.0 / 60.0;
        let (mut saw0, mut saw1) = (false, false);
        for _ in 0..60 {
            p.update(Vec2::ZERO, &mut map, 0.8, dt);
            match p.motion {
                PlayerMotion::Idle(f) => {
                    saw0 |= f == 0;
                    saw1 |= f == 1;
                }
                other => panic!("expected idle when stationary, got {other:?}"),
            }
        }
        assert!(saw0 && saw1, "idle should cycle through both frames");
    }

    #[test]
    fn mining_completes_and_turns_rock_to_dirt() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        // Player flush against the rock's west face, pushing east.
        let mut p = Player::new(flush_east_of((3, 2)), TEST_SPEED);
        let dt = 1.0 / 60.0;
        // Held push: ~0.2s contact interval, then 0.8s mining -> ~62 frames,
        // so run a full second to be safe.
        for _ in 0..70 {
            p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, dt);
        }
        assert_eq!(map.tile(3, 2), Tile::Dirt, "rock was mined through");
        // After the rock breaks the player advances into the freed cell (beyond
        // the old flush boundary at the rock's left edge).
        assert!(
            p.pos.x > 3.0 * TILE_SIZE - HITBOX_HALF,
            "player should walk into the dug cell"
        );
    }

    #[test]
    fn mining_needs_continuous_contact_to_start() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        let mut p = Player::new(flush_east_of((3, 2)), TEST_SPEED);
        let dt = 1.0 / 60.0;
        // A single frame of contact (dt < 0.2s) does not begin mining.
        p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, dt);
        assert!(
            p.mining.is_none(),
            "mining must not begin before 0.2s of contact"
        );
        assert_eq!(map.tile(3, 2), Tile::Mineable);
        // Stop pushing; the contact timer must reset.
        p.update(Vec2::ZERO, &mut map, 0.8, dt);
        assert!(p.mining.is_none());
        // Pushing again for 0.2s+ then mining through still works.
        let mut total = 0f32;
        while p.mining.is_none() {
            let step = 1.0 / 120.0;
            p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, step);
            total += step;
            assert!(total < 1.0, "should start mining within a second");
        }
    }

    #[test]
    fn unmineable_rock_cannot_be_mined() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Unmineable);
        let mut p = Player::new(flush_east_of((3, 2)), TEST_SPEED);
        for _ in 0..60 {
            p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, 1.0 / 60.0);
        }
        assert_eq!(map.tile(3, 2), Tile::Unmineable, "unmineable rock stays");
        assert!(
            p.mining.is_none(),
            "no mining engaged against a non-diggable cell"
        );
    }

    #[test]
    fn mining_ignores_unbreakable_rock() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Unbreakable);
        let mut p = Player::new(flush_east_of((3, 2)), TEST_SPEED);
        // Pushing against unbreakable rock never engages mining (nothing to dig).
        for _ in 0..60 {
            p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, 1.0 / 60.0);
        }
        assert!(p.mining.is_none());
        assert_eq!(map.tile(3, 2), Tile::Unbreakable);
    }

    #[test]
    fn super_pick_breaks_unmineable_instantly() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Unmineable);
        let mut p = Player::new(flush_east_of((3, 2)), TEST_SPEED);
        p.super_pick = true;
        // The mine completes the first frame the player pushes into the rock.
        p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, 1.0 / 60.0);
        assert_eq!(
            map.tile(3, 2),
            Tile::Dirt,
            "Super Pick breaks an unmineable rock instantly"
        );
        assert_eq!(
            p.take_excavated(),
            Some((3, 2)),
            "the excavated cell is reported"
        );
    }

    #[test]
    fn super_pick_never_breaks_unbreakable() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Unbreakable);
        let mut p = Player::new(flush_east_of((3, 2)), TEST_SPEED);
        p.super_pick = true;
        for _ in 0..60 {
            p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, 1.0 / 60.0);
        }
        assert_eq!(
            map.tile(3, 2),
            Tile::Unbreakable,
            "Super Pick cannot break unbreakable rock"
        );
        assert!(p.take_excavated().is_none());
    }

    #[test]
    fn update_reports_excavated_cell_only_when_it_breaks() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        let mut p = Player::new(flush_east_of((3, 2)), TEST_SPEED);
        // Mine through: once the rock breaks, `take_excavated` reports the cell.
        let mut dug = None;
        for _ in 0..70 {
            p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, 1.0 / 60.0);
            if let Some(c) = p.take_excavated() {
                dug = Some(c);
                break;
            }
        }
        assert_eq!(dug, Some((3, 2)));
        assert_eq!(map.tile(3, 2), Tile::Dirt);
    }

    #[test]
    fn mining_continues_while_still_pushing_same_rock() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        let mut p = Player::new(flush_east_of((3, 2)), TEST_SPEED);
        let dt = 1.0 / 60.0;
        // Begin mining the rock to the right (contact interval + initial frames).
        for _ in 0..20 {
            p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, dt);
        }
        assert_eq!(
            p.mining.map(|m| m.target),
            Some((3, 2)),
            "mining the east rock"
        );
        let before = p.mining.unwrap().progress;
        assert!(before > 0.0);
        // Keep pushing the SAME direction: mining continues and accrues.
        p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, dt);
        assert_eq!(
            p.mining.map(|m| m.target),
            Some((3, 2)),
            "same-direction push keeps mining"
        );
        assert!(
            p.mining.unwrap().progress > before,
            "progress keeps accruing"
        );
        // The player did not move despite the direction press (movement ignored).
        assert_eq!(p.pos, flush_east_of((3, 2)));
    }

    #[test]
    fn releasing_move_cancels_mining_and_resets_progress() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        let mut p = Player::new(flush_east_of((3, 2)), TEST_SPEED);
        let dt = 1.0 / 60.0;
        // Begin mining the rock to the right.
        for _ in 0..20 {
            p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, dt);
        }
        assert_eq!(p.mining.map(|m| m.target), Some((3, 2)));
        let mid_progress = p.mining.unwrap().progress;
        assert!(mid_progress > 0.0);
        // Releasing the direction key cancels mining; the rock keeps no progress
        // (it is still mineable, and a fresh mine starts from zero).
        p.update(Vec2::ZERO, &mut map, 0.8, dt);
        assert!(p.mining.is_none(), "releasing the key cancels mining");
        assert_eq!(
            map.tile(3, 2),
            Tile::Mineable,
            "rock has no life; still mineable"
        );
    }

    #[test]
    fn pushing_other_direction_does_not_hold_the_mine() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        map.set_tile(1, 2, Tile::Mineable);
        let mut p = Player::new(flush_east_of((3, 2)), TEST_SPEED);
        let dt = 1.0 / 60.0;
        for _ in 0..20 {
            p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, dt);
        }
        assert_eq!(p.mining.map(|m| m.target), Some((3, 2)));
        // Press the opposite direction: the mine is not held onto, so it resets
        // (the east rock is still mineable, no progress kept).
        p.update(Vec2::new(-1.0, 0.0), &mut map, 0.8, dt);
        assert!(p.mining.is_none(), "pushing elsewhere cancels the mine");
        assert_eq!(map.tile(3, 2), Tile::Mineable);
    }

    #[test]
    fn restarting_a_mine_starts_from_zero() {
        let mut map = open_map();
        map.set_tile(3, 2, Tile::Mineable);
        let mut p = Player::new(flush_east_of((3, 2)), TEST_SPEED);
        let dt = 1.0 / 60.0;
        // Mine a bit, then release.
        for _ in 0..20 {
            p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, dt);
        }
        assert_eq!(p.mining.map(|m| m.target), Some((3, 2)));
        p.update(Vec2::ZERO, &mut map, 0.8, dt);
        // Re-push: mining restarts but progress begins from 0 (fresh mine).
        let mut restarted = false;
        for _ in 0..30 {
            p.update(Vec2::new(1.0, 0.0), &mut map, 0.8, dt);
            if let Some(m) = p.mining {
                assert!(
                    m.progress < 0.15,
                    "restarted mine resets progress, got {}",
                    m.progress
                );
                restarted = true;
                break;
            }
        }
        assert!(restarted, "mining should restart on a new push");
        assert_eq!(map.tile(3, 2), Tile::Mineable, "rock still unbroken");
    }

    /// Fuzz: random movement (incl. rapid reversals, diagonals, variable dt)
    /// against a real generated map must never tunnel through a solid tile or
    /// leave the map. Reproduces the "player walks over rocks / off screen" bug.
    #[test]
    fn fuzz_never_tunnels_or_leaves_map_with_real_map() {
        use crate::config::map::MapConfig;
        use crate::game::generation::generate;
        use macroquad::rand::RandGenerator;

        let cfg = MapConfig::from_toml(
            r#"
                width = 30
                height = 20
                unmineable_count = 20
                start = { x = 15, y = 19 }
                exit  = { x = 5,  y = 0 }
                structures = [[8, 5], [9, 5], [10, 5]]
            "#,
        )
        .expect("valid config");
        let mut map = generate(&cfg, 12345).expect("generates");
        let start = map.start_pos();
        let mut p = Player::new(center_of((start.0 as i32, start.1 as i32)), 120.0);

        let map_w = map.width as f32 * TILE_SIZE;
        let map_h = map.height as f32 * TILE_SIZE;
        let rng = RandGenerator::new();
        rng.srand(0xDEADBEEF);

        for frame in 0..50_000 {
            // Random direction incl. (0,0), diagonals, and cardinals.
            let dx = rng.gen_range(-1i32, 2i32);
            let dy = rng.gen_range(-1i32, 2i32);
            // dt from 1ms to 100ms (also stress the sub-stepper).
            let dt = rng.gen_range(1.0f32, 100.0) / 1000.0;

            p.update(Vec2::new(dx as f32, dy as f32), &mut map, 0.8, dt);

            let (x, y) = (p.pos.x, p.pos.y);
            assert!(
                x.is_finite() && y.is_finite(),
                "NaN at frame {frame}: ({x},{y})"
            );
            // Center must never sit inside a solid cell (rock/wall/border).
            let cx = (x / TILE_SIZE).floor() as i32;
            let cy = (y / TILE_SIZE).floor() as i32;
            assert!(
                !map.is_solid(cx, cy),
                "player centered inside solid cell ({cx},{cy}) at frame {frame} pos ({x},{y})"
            );
            // Hitbox must not overlap a solid tile by more than a hair either.
            let in_bounds = x - HITBOX_HALF >= -0.01
                && x + HITBOX_HALF <= map_w + 0.01
                && y - HITBOX_HALF >= -0.01
                && y + HITBOX_HALF <= map_h + 0.01;
            assert!(in_bounds, "player left the map at frame {frame}: ({x},{y})");
        }
    }
}
