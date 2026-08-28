//! Beast entity: a grid-aware chaser that hunts and digs toward the player
//! (pure, no rendering).
//!
//! M3 turns the M2 straight-line stub into the full AI. The beast sees the
//! physical map (open `Dirt` vs rock vs unbreakable) but only learns a rock's
//! **mineability** locally: a rock becomes diggable once the beast has been
//! adjacent to it (added to the growing `known_mineable` set). Its decision
//! loop, in priority order, is:
//!
//! 1. **Straight-line charge** when a clear line of open floor to the player
//!    exists (no rock/wall between).
//! 2. **A\* to the player** over open floor + known mineable rocks.
//! 3. **A\* to the known mineable rock nearest the player** — how it carves
//!    toward the player when the direct route is blocked.
//! 4. **Idle**, waiting for the next re-plan.
//!
//! It re-plans on a timer, when the path is exhausted, or after finishing a dig.

use std::collections::HashSet;

use macroquad::prelude::Vec2;

use super::map::{Map, Tile};
use super::movement;
use super::pathfinding;
use super::TILE_SIZE;
use crate::assets::ids::{BeastMotion, Direction, WALK_FRAMES};

/// Seconds per beast walk-frame (four-frame cycle, per atlas timing).
const WALK_FRAME_TIME: f32 = 0.1;

/// The beast's current behaviour state (drives its actions each frame).
#[derive(Debug, Clone, PartialEq)]
pub enum BeastState {
    /// Hold position until the next re-plan.
    Idle,
    /// Move in a straight line directly at the player.
    Charge,
    /// Follow a grid path; `next` is the index into `path`.
    Follow { path: Vec<(i32, i32)>, next: usize },
    /// Digging the known mineable rock at `target`.
    Dig { target: (i32, i32), progress: f32 },
}

/// A beast entity.
#[derive(Debug, Clone)]
pub struct Beast {
    /// Center of the hitbox, in world pixels.
    pub pos: Vec2,
    /// Facing direction (normalized), updated from movement.
    pub facing: Vec2,
    /// Current animation motion.
    pub motion: BeastMotion,
    /// Rocks confirmed diggable (grows as the beast perceives adjacent cells).
    pub known_mineable: HashSet<(i32, i32)>,
    /// The current behaviour state.
    pub state: BeastState,
    speed: f32,
    mining_time: f32,
    replan_interval: f32,
    replan_timer: f32,
    walk_timer: f32,
}

impl Beast {
    pub fn new(pos: Vec2, speed: f32, mining_time: f32, replan_interval: f32) -> Self {
        Beast {
            pos,
            facing: Vec2::new(0.0, 1.0),
            motion: BeastMotion::Idle,
            known_mineable: HashSet::new(),
            state: BeastState::Idle,
            speed,
            mining_time,
            replan_interval,
            replan_timer: 0.0,
            walk_timer: 0.0,
        }
    }

    /// Advance the beast by `dt`. `map` is borrowed mutably because the beast
    /// digs (sets a cell to `Dirt` when it finishes mining a rock).
    pub fn update(&mut self, player_pos: Vec2, map: &mut Map, dt: f32) {
        self.perceive(map);
        self.replan_timer -= dt;

        // Finish an in-progress dig before doing anything else. On completion,
        // break the rock and step into the freed cell so the next cell beyond it
        // becomes perceptible, then re-plan from there.
        let mut dig_done: Option<(i32, i32)> = None;
        if let BeastState::Dig { target, progress } = &mut self.state {
            *progress += dt;
            self.facing = (cell_center(*target) - self.pos).normalize_or_zero();
            self.motion = BeastMotion::Idle;
            if *progress >= self.mining_time {
                dig_done = Some(*target);
            }
        }
        if let Some(target) = dig_done {
            // Only actually break the rock if it is still diggable (it may have
            // been mined by the player mid-dig, or was never mineable).
            if map.tile(target.0, target.1) == Tile::Mineable {
                map.set_tile(target.0, target.1, Tile::Dirt);
            }
            self.known_mineable.remove(&target);
            self.state = BeastState::Follow {
                path: vec![self.cell(), target],
                next: 1,
            };
            self.replan_timer = self.replan_interval;
            return;
        }
        if matches!(self.state, BeastState::Dig { .. }) {
            return; // still digging
        }

        // Re-plan on a timer or when the current plan is broken.
        if self.replan_timer <= 0.0 || self.plan_invalid() {
            self.replan(player_pos, map);
            self.replan_timer = self.replan_interval;
        }

        // Act per the (possibly fresh) state. Extract the follow target first so
        // the immutable borrow of `self.state` ends before the action mutates.
        let action = match &self.state {
            BeastState::Charge => Action::Charge,
            BeastState::Follow { path, next } => Action::Follow(path.get(*next).copied()),
            BeastState::Idle => Action::Idle,
            BeastState::Dig { .. } => Action::Idle, // surfaced above; unreachable
        };
        match action {
            Action::Charge => self.act_charge(player_pos, map, dt),
            Action::Follow(target) => self.act_follow(target, map, dt),
            Action::Idle => self.motion = BeastMotion::Idle,
        }
    }

    /// Learn each cardinal neighbour of the current cell. Mineable neighbours
    /// join `known_mineable` (the beast's digging candidates).
    fn perceive(&mut self, map: &Map) {
        let cell = self.cell();
        for (dx, dy) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
            let n = (cell.0 + dx, cell.1 + dy);
            if map.tile(n.0, n.1) == Tile::Mineable {
                self.known_mineable.insert(n);
            }
        }
    }

    /// Choose the next behaviour state via [`decide`].
    fn replan(&mut self, player_pos: Vec2, map: &Map) {
        let beast_cell = self.cell();
        let player_cell = cell_of(player_pos);
        let plan = decide(map, &self.known_mineable, beast_cell, player_cell);
        self.state = match plan {
            Plan::Charge => BeastState::Charge,
            Plan::Path(p) => {
                // Skip the beast's own starting cell (it's already standing on
                // it); head straight for the next node.
                let next = 1.min(p.len().saturating_sub(1));
                BeastState::Follow { path: p, next }
            }
            Plan::Idle => BeastState::Idle,
        };
    }

    /// True when the current plan can no longer be followed.
    fn plan_invalid(&self) -> bool {
        match &self.state {
            BeastState::Follow { path, next } => *next >= path.len(),
            _ => false,
        }
    }

    fn act_charge(&mut self, player_pos: Vec2, map: &Map, dt: f32) {
        let blocked = self.step_toward(player_pos, map, dt);
        if blocked {
            // The straight path is no longer clear; re-plan next frame.
            self.replan_timer = 0.0;
        }
    }

    fn act_follow(&mut self, target: Option<(i32, i32)>, map: &mut Map, dt: f32) {
        let Some(target) = target else {
            // Path exhausted; re-plan next frame.
            self.replan_timer = 0.0;
            self.motion = BeastMotion::Idle;
            return;
        };

        let cell = self.cell();
        let target_rock = map.tile(target.0, target.1) == Tile::Mineable
            && self.known_mineable.contains(&target);

        if cell == target {
            // Stepped into an open (dirt) cell; head for the next node.
            if let BeastState::Follow { path, next } = &mut self.state {
                *next = (*next + 1).min(path.len());
            }
            return;
        }

        let blocked = self.step_toward(cell_center(target), map, dt);

        if target_rock && blocked && adjacent(cell, target) {
            // Flush against a known diggable rock: start digging it.
            self.state = BeastState::Dig { target, progress: 0.0 };
        } else if blocked {
            // Hit an unmineable/unknown slab the plan couldn't foresee: re-plan
            // soon rather than grind against it.
            self.replan_timer = 0.0;
        }
    }

    /// Move toward `target` (world px) by up to `speed*dt`, resolving collision
    /// against solids. Returns true when the beast was blocked (moved far less
    /// than it intended to).
    fn step_toward(&mut self, target: Vec2, map: &Map, dt: f32) -> bool {
        let to = target - self.pos;
        let dist = to.length();
        if dist <= 0.001 {
            self.motion = BeastMotion::Idle;
            return false;
        }
        let dir = to / dist;
        self.facing = dir;
        let intended = self.speed * dt;
        let step = dir * intended;
        let before = self.pos;
        movement::move_axis(&mut self.pos, map, true, step.x);
        movement::move_axis(&mut self.pos, map, false, step.y);
        let moved = (self.pos - before).length();

        self.walk_timer += dt;
        let frame = (self.walk_timer / WALK_FRAME_TIME) as usize % WALK_FRAMES;
        self.motion = BeastMotion::Walk(frame as u8);

        moved < intended * 0.25
    }

    fn cell(&self) -> (i32, i32) {
        cell_of(self.pos)
    }

    /// The beast's current dig, as `(target_cell, progress_ratio in 0..1)`, to
    /// drive the rock-breaking (excavation) effect. `None` when not digging.
    pub fn dig_frame(&self) -> Option<((i32, i32), f32)> {
        match &self.state {
            BeastState::Dig { target, progress } => Some((*target, *progress / self.mining_time)),
            _ => None,
        }
    }

    /// Direction for animation (dominant axis of the facing vector).
    pub fn dir(&self) -> Direction {
        Direction::from_vec2(self.facing)
    }
}

/// A transient "what to do next" summary extracted from the state each frame, so
/// the state can be mutated by the action without holding a borrow.
enum Action {
    Charge,
    Follow(Option<(i32, i32)>),
    Idle,
}

/// A beast decision (the pure output of [`decide`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Charge directly at the player.
    Charge,
    /// Follow this grid path (includes both endpoints).
    Path(Vec<(i32, i32)>),
    /// Hold position.
    Idle,
}

/// The beast decision loop, in priority order:
/// 1. Straight-line charge when a clear line of open floor to the player exists.
/// 2. A\* to the player over open floor + known mineable rocks.
/// 3. A\* to the known mineable rock nearest the player (carves toward the
///    player when the direct route is blocked).
/// 4. Idle.
pub fn decide(
    map: &Map,
    known: &HashSet<(i32, i32)>,
    beast: (i32, i32),
    player: (i32, i32),
) -> Plan {
    if straight_line_clear(map, beast, player) {
        return Plan::Charge;
    }
    let passable_to = |x: i32, y: i32| passable(x, y, map, known);
    if let Some(path) = pathfinding::astar(beast, player, passable_to) {
        return Plan::Path(path);
    }
    // Fallback: dig toward the known mineable rock nearest the player.
    let dig_target = known
        .iter()
        .filter(|&&(x, y)| map.tile(x, y) == Tile::Mineable)
        .min_by_key(|&&(x, y)| manhattan((x, y), player));
    if let Some(target) = dig_target {
        if let Some(path) = pathfinding::astar(beast, *target, passable_to) {
            return Plan::Path(path);
        }
    }
    Plan::Idle
}

/// Whether a cell is passable to the beast: open floor, or a rock known to be
/// mineable. Unknown/unmineable/unbreakable cells are blocked.
pub fn passable(x: i32, y: i32, map: &Map, known: &HashSet<(i32, i32)>) -> bool {
    match map.tile(x, y) {
        Tile::Dirt => true,
        Tile::Mineable => known.contains(&(x, y)),
        _ => false,
    }
}

/// Clear straight horizontal/vertical line of open floor between `a` and `b`.
fn straight_line_clear(map: &Map, a: (i32, i32), b: (i32, i32)) -> bool {
    if a == b {
        return false;
    }
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    if dx != 0 && dy != 0 {
        return false; // diagonal — not a charge line
    }
    if dx != 0 {
        let step = dx.signum();
        let mut x = a.0 + step;
        while x != b.0 {
            if map.tile(x, a.1) != Tile::Dirt {
                return false;
            }
            x += step;
        }
    } else {
        let step = dy.signum();
        let mut y = a.1 + step;
        while y != b.1 {
            if map.tile(a.0, y) != Tile::Dirt {
                return false;
            }
            y += step;
        }
    }
    true
}

fn adjacent(a: (i32, i32), b: (i32, i32)) -> bool {
    manhattan(a, b) == 1
}

fn manhattan(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

fn cell_center(c: (i32, i32)) -> Vec2 {
    Vec2::new(c.0 as f32 * TILE_SIZE + TILE_SIZE / 2.0, c.1 as f32 * TILE_SIZE + TILE_SIZE / 2.0)
}

fn cell_of(pos: Vec2) -> (i32, i32) {
    ((pos.x / TILE_SIZE).floor() as i32, (pos.y / TILE_SIZE).floor() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_map() -> Map {
        let mut m = Map { width: 5, height: 5, tiles: vec![Tile::Dirt; 25], start: (0, 2), exit: (4, 2) };
        for y in 0..5 {
            m.tiles[y * 5 + 0] = Tile::Unbreakable;
            m.tiles[y * 5 + 4] = Tile::Unbreakable;
        }
        for x in 0..5 {
            m.tiles[0 * 5 + x] = Tile::Unbreakable;
            m.tiles[4 * 5 + x] = Tile::Unbreakable;
        }
        m
    }

    fn center_of(c: (i32, i32)) -> Vec2 {
        Vec2::new(c.0 as f32 * TILE_SIZE + TILE_SIZE / 2.0, c.1 as f32 * TILE_SIZE + TILE_SIZE / 2.0)
    }

    /// A beast in the old M2 configuration (walk speed only): helpers here use
    /// the new 4-arg constructor.
    fn beast_at(c: (i32, i32)) -> Beast {
        Beast::new(center_of(c), 140.0, 1.6, 0.25)
    }

    #[test]
    fn beast_moves_toward_player_on_open_floor() {
        let mut map = open_map();
        let mut b = beast_at((1, 2));
        let start = b.pos;
        let player = center_of((3, 2));
        b.update(player, &mut map, 1.0 / 60.0);
        assert!(b.pos.x > start.x, "beast should move right toward player");
        assert!(b.pos.x <= player.x);
        assert!(matches!(b.motion, BeastMotion::Walk(_)), "beast should be walking");
    }

    #[test]
    fn beast_is_blocked_by_solid_cells() {
        // A rock column separates the beast (left) from the player (right).
        let mut map = open_map();
        for y in 1..4 {
            map.set_tile(2, y, Tile::Unbreakable);
        }
        let mut b = beast_at((1, 2));
        let player = center_of((3, 2));
        for _ in 0..120 {
            b.update(player, &mut map, 1.0 / 60.0);
        }
        assert!(b.pos.x < 2.0 * TILE_SIZE, "beast must not pass the rock");
    }

    #[test]
    fn facing_updates_toward_player() {
        let mut map = open_map();
        let mut b = beast_at((2, 3));
        let player = center_of((2, 1)); // above
        b.update(player, &mut map, 1.0 / 60.0);
        assert!(b.facing.y < 0.0, "beast should face up");
        assert_eq!(b.dir(), Direction::Up);
    }

    #[test]
    fn perceives_mineable_neighbours_only() {
        let mut map = open_map();
        map.set_tile(2, 1, Tile::Mineable);
        map.set_tile(2, 3, Tile::Unmineable);
        map.set_tile(1, 2, Tile::Unbreakable);
        let mut b = beast_at((2, 2));
        b.perceive(&map);
        assert!(b.known_mineable.contains(&(2, 1)), "adjacent mineable is learned");
        assert!(!b.known_mineable.contains(&(2, 3)), "unmineable is never learned");
        assert!(!b.known_mineable.contains(&(1, 2)), "unbreakable is never learned");
        assert!(!b.known_mineable.contains(&(2, 2)), "own cell is not perceived");
    }

    #[test]
    fn passable_rules() {
        let mut map = open_map();
        map.set_tile(2, 1, Tile::Mineable);
        map.set_tile(2, 3, Tile::Unmineable);
        map.set_tile(3, 1, Tile::Mineable); // an *unknown* mineable (not in `known`)
        let known: HashSet<_> = [(2, 1)].into_iter().collect();
        assert!(passable(2, 2, &map, &known), "open floor is passable");
        assert!(passable(2, 1, &map, &known), "known mineable is passable");
        assert!(!passable(2, 3, &map, &known), "unmineable is blocked");
        assert!(!passable(0, 0, &map, &known), "border unbreakable is blocked");
        assert!(!passable(3, 1, &map, &known), "unperceived slab is blocked");
    }

    #[test]
    fn straight_line_charge_when_clear() {
        let map = open_map();
        let known = HashSet::new();
        assert_eq!(decide(&map, &known, (1, 2), (3, 2)), Plan::Charge, "clear horizontal");
        assert_eq!(decide(&map, &known, (2, 1), (2, 3)), Plan::Charge, "clear vertical");
        // Adjacent cells on the same row also charge (no cells in between).
        assert_eq!(decide(&map, &known, (1, 2), (2, 2)), Plan::Charge);
    }

    #[test]
    fn no_charge_when_rock_in_between() {
        let mut map = open_map();
        map.set_tile(2, 2, Tile::Mineable);
        let known: HashSet<_> = [(2, 2)].into_iter().collect();
        let plan = decide(&map, &known, (1, 2), (3, 2));
        assert_ne!(plan, Plan::Charge, "a rock in the line blocks a charge");
        assert!(matches!(plan, Plan::Path(_)), "a diggable door gives a path");
    }

    #[test]
    fn astar_to_player_through_known_mineable() {
        let mut map = open_map();
        // Rock wall at column 2 with a known-mineable door at (2,2).
        for y in 1..4 {
            map.set_tile(2, y, Tile::Mineable);
        }
        let known: HashSet<_> = [(2, 2)].into_iter().collect();
        match decide(&map, &known, (1, 2), (3, 2)) {
            Plan::Path(p) => {
                assert_eq!(*p.first().unwrap(), (1, 2));
                assert_eq!(*p.last().unwrap(), (3, 2));
            }
            other => panic!("expected a path, got {other:?}"),
        }
    }

    #[test]
    fn astar_to_player_fails_and_idles_with_no_known_rocks() {
        let mut map = open_map();
        for y in 1..4 {
            map.set_tile(2, y, Tile::Mineable);
        }
        // (2,2) is unknown => the wall is impassable and there's nothing to dig.
        let known = HashSet::new();
        assert_eq!(decide(&map, &known, (1, 2), (3, 2)), Plan::Idle);
    }

    #[test]
    fn fallback_targets_known_mineable_nearest_to_player() {
        let mut map = open_map();
        // Seal the player at (3,2) with unmineable rock so it's unreachable.
        map.set_tile(3, 1, Tile::Unmineable);
        map.set_tile(3, 3, Tile::Unmineable);
        map.set_tile(2, 2, Tile::Unmineable);
        // A single reachable, known-mineable "door" at (2,1).
        map.set_tile(2, 1, Tile::Mineable);
        let known: HashSet<_> = [(2, 1)].into_iter().collect();
        match decide(&map, &known, (1, 2), (3, 2)) {
            Plan::Path(p) => {
                assert_eq!(*p.last().unwrap(), (2, 1), "carve toward the known rock nearest the player");
            }
            other => panic!("expected a fallback path, got {other:?}"),
        }
    }

    #[test]
    fn digs_known_mineable_rock_after_mining_time() {
        let mut map = open_map();
        map.set_tile(2, 3, Tile::Mineable);
        let mut b = Beast::new(center_of((2, 2)), 140.0, 1.0, 0.25);
        b.state = BeastState::Dig { target: (2, 3), progress: 0.0 };
        // Dig for 1.0s (mining_time) + a margin.
        for _ in 0..70 {
            b.update(center_of((2, 1)), &mut map, 1.0 / 60.0);
        }
        assert_eq!(map.tile(2, 3), Tile::Dirt, "known rock was dug through");
        assert!(!b.known_mineable.contains(&(2, 3)), "dug cell leaves the known set");
    }

    #[test]
    fn never_digs_unknown_or_unmineable() {
        let mut map = open_map();
        map.set_tile(2, 3, Tile::Unmineable);
        let mut b = Beast::new(center_of((2, 2)), 140.0, 1.0, 0.25);
        b.state = BeastState::Dig { target: (2, 3), progress: 0.0 };
        for _ in 0..70 {
            b.update(center_of((2, 1)), &mut map, 1.0 / 60.0);
        }
        assert_eq!(map.tile(2, 3), Tile::Unmineable, "unmineable rocks are never dug");
        // Unknown cells are never *targeted* by the decision loop either.
        let known = HashSet::new();
        for y in 1..4 {
            map.set_tile(2, y, Tile::Mineable);
        }
        assert_eq!(decide(&map, &known, (1, 2), (3, 2)), Plan::Idle);
    }

    #[test]
    fn dig_frame_reports_target_and_progress_ratio() {
        let mut b = Beast::new(center_of((2, 2)), 140.0, 2.0, 0.25);
        b.state = BeastState::Dig { target: (2, 3), progress: 0.5 };
        let (target, ratio) = b.dig_frame().expect("is digging");
        assert_eq!(target, (2, 3));
        assert!((ratio - 0.25).abs() < 1e-5, "0.5 / 2.0 = 0.25, got {ratio}");
        // Not digging -> None.
        b.state = BeastState::Idle;
        assert!(b.dig_frame().is_none());
    }
}
