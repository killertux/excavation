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

use super::TILE_SIZE;
use super::map::{Map, Tile};
use super::movement;
use super::pathfinding;
use crate::assets::ids::{BeastMotion, Direction, WALK_FRAMES};

/// Seconds per beast walk-frame (four-frame cycle, per atlas timing).
const WALK_FRAME_TIME: f32 = 0.1;

/// Seconds between random cardinal re-rolls while wandering (Sticky Smell).
const WANDER_RETIME: f32 = 0.5;

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
    /// Randomly wandering (Sticky Smell active): pathfinding disabled, no
    /// digging. Moves in `dir`, re-rolled on block or every ~[`WANDER_RETIME`].
    Wander { dir: Vec2, timer: f32 },
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
    /// Sticky Smell active this frame: wander instead of pathfinding/digging.
    /// Set by `Level` each frame from the active consumable effect.
    pub sticky: bool,
    speed: f32,
    mining_time: f32,
    replan_interval: f32,
    replan_timer: f32,
    walk_timer: f32,
    /// State for a small deterministic xorshift64* RNG (used for wander
    /// directions; the quad-rand `RandGenerator` is neither `Clone` nor `Copy`).
    wander_rng: u64,
    /// The grid cell excavated this frame, if any (taken by `Level` to drop gold).
    last_excavated: Option<(i32, i32)>,
}

impl Beast {
    pub fn new(pos: Vec2, speed: f32, mining_time: f32, replan_interval: f32) -> Self {
        let pos_bits = (pos.x as i64) as u64 ^ ((pos.y as i64) as u64).rotate_left(32);
        Beast {
            pos,
            facing: Vec2::new(0.0, 1.0),
            motion: BeastMotion::Idle,
            known_mineable: HashSet::new(),
            state: BeastState::Idle,
            sticky: false,
            speed,
            mining_time,
            replan_interval,
            replan_timer: 0.0,
            walk_timer: 0.0,
            wander_rng: 0x9E37_79B9_7F4A_7C15u64 ^ pos_bits,
            last_excavated: None,
        }
    }

    /// Advance the beast by `dt`. `map` is borrowed mutably because the beast
    /// digs (sets a cell to `Dirt` when it finishes mining a rock).
    ///
    /// When [`Beast::sticky`] is set the beast wanders randomly and never
    /// pathfinds or digs.
    pub fn update(&mut self, player_pos: Vec2, map: &mut Map, dt: f32) {
        if self.sticky {
            self.wander(map, dt);
            return;
        }

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
                self.last_excavated = Some(target);
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
            BeastState::Wander { .. } => Action::Idle, // surfaced above; unreachable
        };
        match action {
            Action::Charge => self.act_charge(player_pos, map, dt),
            Action::Follow(target) => self.act_follow(target, map, dt),
            Action::Idle => self.motion = BeastMotion::Idle,
        }
    }

    /// The grid cell excavated this frame, if a rock just broke. Consumes the
    /// value (one call per frame).
    pub fn take_excavated(&mut self) -> Option<(i32, i32)> {
        self.last_excavated.take()
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
        let target_rock =
            map.tile(target.0, target.1) == Tile::Mineable && self.known_mineable.contains(&target);

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
            self.state = BeastState::Dig {
                target,
                progress: 0.0,
            };
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
        self.step_dir(to / dist, map, dt)
    }

    /// Move by `speed*dt` along the unit `dir`, resolving collision. Returns
    /// true when blocked (moved far less than intended).
    fn step_dir(&mut self, dir: Vec2, map: &Map, dt: f32) -> bool {
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

    /// Wander (Sticky Smell): pick a random cardinal direction, move in it, and
    /// re-roll when blocked or every ~[`WANDER_RETIME`] seconds. Neither
    /// pathfinding nor digging happens while wandering.
    fn wander(&mut self, map: &mut Map, dt: f32) {
        // Begin wandering from any other state (discarding a stale dig/plan).
        if !matches!(self.state, BeastState::Wander { .. }) {
            self.state = BeastState::Wander {
                dir: Vec2::ZERO,
                timer: 0.0,
            };
        }
        let (dir, timer) = match self.state {
            BeastState::Wander { dir, timer } => (dir, timer),
            _ => unreachable!(),
        };
        let new_timer = timer - dt;
        let blocked = self.step_dir(dir, map, dt);
        if new_timer <= 0.0 || blocked || dir == Vec2::ZERO {
            let nd = self.random_cardinal();
            self.state = BeastState::Wander {
                dir: nd,
                timer: WANDER_RETIME,
            };
        } else {
            self.state = BeastState::Wander {
                dir,
                timer: new_timer,
            };
        }
    }

    /// A random cardinal direction, via the beast's inline RNG.
    fn random_cardinal(&mut self) -> Vec2 {
        const DIRS: [Vec2; 4] = [
            Vec2::new(0.0, -1.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(-1.0, 0.0),
            Vec2::new(1.0, 0.0),
        ];
        DIRS[(self.next_random() % 4) as usize]
    }

    /// Next value of the beast's xorshift64* PRNG state.
    fn next_random(&mut self) -> u64 {
        let mut x = self.wander_rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.wander_rng = x;
        x
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

    /// Whether a dirt-only (already-dug) A\* path exists from the beast's cell to
    /// the player's cell on the current map. Used to choose the "chase" music
    /// variant: the beast has a clear route to the player (it is really chasing,
    /// not digging/blocked). Passability is strictly `Tile::Dirt`.
    pub fn has_clear_path(&self, map: &Map, player: (i32, i32)) -> bool {
        pathfinding::has_path(self.cell(), player, |x, y| map.tile(x, y) == Tile::Dirt)
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
    // Prefer a clear, already-dug (dirt-only) route to the player. Walking an
    // existing tunnel is faster than digging, so the beast should use one even
    // when a path through known rocks is the same length (or slightly shorter)
    // — otherwise it would tunnel a pointless shortcut instead of following the
    // open path the player made.
    let dirt_only = |x: i32, y: i32| map.tile(x, y) == Tile::Dirt;
    if let Some(path) = pathfinding::astar(beast, player, dirt_only) {
        return Plan::Path(path);
    }
    // Otherwise allow digging through known mineable rock toward the player.
    let passable_to = |x: i32, y: i32| passable(x, y, map, known);
    if let Some(path) = pathfinding::astar(beast, player, passable_to) {
        return Plan::Path(path);
    }
    // Fallback: dig toward the known mineable rock nearest the player that is
    // actually reachable. Try candidates in increasing distance-to-player order
    // so a nearby rock that happens to be walled off (unreachable) can't strand
    // the beast in Idle — it falls through to the next reachable one.
    let mut candidates: Vec<(i32, i32)> = known
        .iter()
        .filter(|&&(x, y)| map.tile(x, y) == Tile::Mineable)
        .copied()
        .collect();
    candidates.sort_by_key(|&(x, y)| (manhattan((x, y), player), x, y));
    for target in candidates {
        if let Some(path) = pathfinding::astar(beast, target, passable_to) {
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
    Vec2::new(
        c.0 as f32 * TILE_SIZE + TILE_SIZE / 2.0,
        c.1 as f32 * TILE_SIZE + TILE_SIZE / 2.0,
    )
}

fn cell_of(pos: Vec2) -> (i32, i32) {
    (
        (pos.x / TILE_SIZE).floor() as i32,
        (pos.y / TILE_SIZE).floor() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_map() -> Map {
        let mut m = Map::new(5, 5, (0, 2), (4, 2));
        for y in 1..4 {
            for x in 1..4 {
                m.tiles[y * 5 + x] = Tile::Dirt;
            }
        }
        m
    }

    fn center_of(c: (i32, i32)) -> Vec2 {
        Vec2::new(
            c.0 as f32 * TILE_SIZE + TILE_SIZE / 2.0,
            c.1 as f32 * TILE_SIZE + TILE_SIZE / 2.0,
        )
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
        assert!(
            matches!(b.motion, BeastMotion::Walk(_)),
            "beast should be walking"
        );
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
        assert!(
            b.known_mineable.contains(&(2, 1)),
            "adjacent mineable is learned"
        );
        assert!(
            !b.known_mineable.contains(&(2, 3)),
            "unmineable is never learned"
        );
        assert!(
            !b.known_mineable.contains(&(1, 2)),
            "unbreakable is never learned"
        );
        assert!(
            !b.known_mineable.contains(&(2, 2)),
            "own cell is not perceived"
        );
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
        assert!(
            !passable(0, 0, &map, &known),
            "border unbreakable is blocked"
        );
        assert!(!passable(3, 1, &map, &known), "unperceived slab is blocked");
    }

    #[test]
    fn straight_line_charge_when_clear() {
        let map = open_map();
        let known = HashSet::new();
        assert_eq!(
            decide(&map, &known, (1, 2), (3, 2)),
            Plan::Charge,
            "clear horizontal"
        );
        assert_eq!(
            decide(&map, &known, (2, 1), (2, 3)),
            Plan::Charge,
            "clear vertical"
        );
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
        assert!(
            matches!(plan, Plan::Path(_)),
            "a diggable door gives a path"
        );
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
                assert_eq!(
                    *p.last().unwrap(),
                    (2, 1),
                    "carve toward the known rock nearest the player"
                );
            }
            other => panic!("expected a fallback path, got {other:?}"),
        }
    }

    #[test]
    fn digs_known_mineable_rock_after_mining_time() {
        let mut map = open_map();
        map.set_tile(2, 3, Tile::Mineable);
        let mut b = Beast::new(center_of((2, 2)), 140.0, 1.0, 0.25);
        b.state = BeastState::Dig {
            target: (2, 3),
            progress: 0.0,
        };
        // Dig for 1.0s (mining_time) + a margin.
        for _ in 0..70 {
            b.update(center_of((2, 1)), &mut map, 1.0 / 60.0);
        }
        assert_eq!(map.tile(2, 3), Tile::Dirt, "known rock was dug through");
        assert!(
            !b.known_mineable.contains(&(2, 3)),
            "dug cell leaves the known set"
        );
    }

    #[test]
    fn never_digs_unknown_or_unmineable() {
        let mut map = open_map();
        map.set_tile(2, 3, Tile::Unmineable);
        let mut b = Beast::new(center_of((2, 2)), 140.0, 1.0, 0.25);
        b.state = BeastState::Dig {
            target: (2, 3),
            progress: 0.0,
        };
        for _ in 0..70 {
            b.update(center_of((2, 1)), &mut map, 1.0 / 60.0);
        }
        assert_eq!(
            map.tile(2, 3),
            Tile::Unmineable,
            "unmineable rocks are never dug"
        );
        // Unknown cells are never *targeted* by the decision loop either.
        let known = HashSet::new();
        for y in 1..4 {
            map.set_tile(2, y, Tile::Mineable);
        }
        assert_eq!(decide(&map, &known, (1, 2), (3, 2)), Plan::Idle);
    }

    #[test]
    fn fallback_tries_next_reachable_if_nearest_is_walled_off() {
        let mut map = open_map();
        // Seal the player at (3,2) so it is unreachable.
        map.set_tile(3, 1, Tile::Unmineable);
        map.set_tile(3, 3, Tile::Unmineable);
        map.set_tile(2, 2, Tile::Unmineable);
        // Enclose the *nearest*-to-player known rock at (2,1) so it's unreachable.
        map.set_tile(1, 1, Tile::Unmineable);
        map.set_tile(2, 1, Tile::Mineable); // known, nearest to player, but walled off
        map.set_tile(1, 3, Tile::Mineable); // known, farther, reachable
        let known: HashSet<_> = [(2, 1), (1, 3)].into_iter().collect();
        match decide(&map, &known, (1, 2), (3, 2)) {
            Plan::Path(p) => {
                assert_eq!(
                    *p.last().unwrap(),
                    (1, 3),
                    "should path to a reachable known rock (not idle) when the nearest is walled off"
                );
            }
            other => panic!("expected a fallback path, got {other:?}"),
        }
    }

    #[test]
    fn beast_reaches_player_via_clear_l_dirt_path() {
        // An L-shaped dirt tunnel from the beast to the player, with the rest of
        // the interior sealed by unbreakable rock. The beast and player are not
        // aligned, so a non-straight A* along the tunnel is required.
        let (w, h) = (7usize, 7usize);
        let mut tiles = vec![Tile::Unbreakable; w * h];
        let cs = |x: i32, y: i32| y as usize * w + x as usize;
        for y in 1..=4 {
            tiles[cs(1, y)] = Tile::Dirt;
        }
        for x in 1..=5 {
            tiles[cs(x, 4)] = Tile::Dirt;
        }
        let mut map = Map {
            width: w,
            height: h,
            tiles,
            start: (1, 1),
            exit: (5, 4),
            gold: HashSet::new(),
        };
        let mut b = Beast::new(center_of((1, 1)), 140.0, 1.6, 0.25);
        let player = center_of((5, 4));

        // Up to ~10 s: the beast must A* along the clear dirt tunnel to reach the
        // player's cell (it should not dig elsewhere or idle).
        let mut reached = false;
        for _ in 0..600 {
            b.update(player, &mut map, 1.0 / 60.0);
            if b.cell() == (5, 4) {
                reached = true;
                break;
            }
        }
        assert!(
            reached,
            "beast should reach the player via the clear dirt tunnel"
        );
    }

    #[test]
    fn repro_clear_tunnel_real_map() {
        use crate::config::map::MapConfig;
        use crate::game::generation::generate;
        let cfg = MapConfig::from_toml(
            r#"
            width = 30
            height = 20
            unmineable_count = 20
            beast_count = 1
            start = { x = 15, y = 19 }
            exit  = { x = 5,  y = 0 }
        "#,
        )
        .unwrap();
        let mut map = generate(&cfg, 12345).unwrap();
        // Carve a full clear Dirt tunnel from the exit gap (beast spawn) down
        // column x=5 to the bottom, then across row y=19 to the start gap, as if
        // the player had dug straight to the beast.
        for y in 0..map.height as i32 {
            map.set_tile(5, y, Tile::Dirt);
        }
        for x in 0..=15 {
            map.set_tile(x, 19, Tile::Dirt);
        }
        let mut b = Beast::new(center_of((5, 1)), 140.0, 1.6, 0.25);
        let player = center_of((15, 19));
        let mut reached = false;
        for _ in 0..1200 {
            b.update(player, &mut map, 1.0 / 60.0);
            if b.cell() == (15, 19) {
                reached = true;
                break;
            }
        }
        assert!(reached, "beast should walk the clear tunnel to the player");
    }

    #[test]
    fn prefers_clear_dirt_tunnel_over_known_rock_shortcut() {
        // A clear dirt tunnel to the player exists, and a shorter route through
        // known-mineable rocks is also possible. The beast must prefer the
        // already-dug tunnel (walking beats digging), not tunnel a shortcut
        // through the rocks.
        let (w, h) = (7usize, 5usize);
        let mut tiles = vec![Tile::Unbreakable; w * h];
        let cs = |x: i32, y: i32| y as usize * w + x as usize;
        // Beast at (1,3), player at (5,3). Clear Dirt corridor via row 4, and a
        // shorter row-3 path through known-mineable rocks (a dig shortcut).
        for (x, y) in [(1, 3), (1, 4), (2, 4), (3, 4), (4, 4), (5, 4), (5, 3)] {
            tiles[cs(x, y)] = Tile::Dirt;
        }
        for (x, y) in [(2, 3), (3, 3), (4, 3)] {
            tiles[cs(x, y)] = Tile::Mineable;
        }
        let map = Map {
            width: w,
            height: h,
            tiles,
            start: (1, 3),
            exit: (5, 3),
            gold: HashSet::new(),
        };
        let known: HashSet<_> = [(2, 3), (3, 3), (4, 3)].into_iter().collect();
        match decide(&map, &known, (1, 3), (5, 3)) {
            Plan::Path(p) => {
                assert_eq!(*p.last().unwrap(), (5, 3));
                assert!(
                    p.iter().all(|&(x, y)| map.tile(x, y) == Tile::Dirt),
                    "beast should walk the clear dirt tunnel, got path {p:?}"
                );
            }
            other => panic!("expected a path, got {other:?}"),
        }
    }

    #[test]
    fn dig_frame_reports_target_and_progress_ratio() {
        let mut b = Beast::new(center_of((2, 2)), 140.0, 2.0, 0.25);
        b.state = BeastState::Dig {
            target: (2, 3),
            progress: 0.5,
        };
        let (target, ratio) = b.dig_frame().expect("is digging");
        assert_eq!(target, (2, 3));
        assert!((ratio - 0.25).abs() < 1e-5, "0.5 / 2.0 = 0.25, got {ratio}");
        // Not digging -> None.
        b.state = BeastState::Idle;
        assert!(b.dig_frame().is_none());
    }

    #[test]
    fn has_clear_path_detects_dug_tunnel() {
        let mut map = open_map();
        let b = beast_at((1, 2));
        // A straight dirt row between beast and player -> a clear path.
        assert!(b.has_clear_path(&map, (3, 2)));

        // Seal the row with unbreakable rock -> no dirt-only path.
        for y in 1..4 {
            map.set_tile(2, y, Tile::Unbreakable);
        }
        assert!(!b.has_clear_path(&map, (3, 2)));

        // A mineable door does NOT count as a clear path: chase stays off until
        // the beast actually digs the rock open (chase is dirt-only).
        map.set_tile(2, 2, Tile::Mineable);
        assert!(
            !b.has_clear_path(&map, (3, 2)),
            "dirt-only means no rock on the route"
        );
    }

    #[test]
    fn sticky_beast_wanders_and_never_digs_or_pathfinds() {
        let mut map = open_map();
        map.set_tile(2, 3, Tile::Mineable); // a nearby diggable rock
        let mut b = Beast::new(center_of((2, 2)), 140.0, 1.0, 0.25);
        b.sticky = true;
        for _ in 0..400 {
            b.update(center_of((2, 0)), &mut map, 1.0 / 60.0);
            assert!(
                matches!(b.state, BeastState::Wander { .. }),
                "sticky => always wandering"
            );
            assert!(b.dig_frame().is_none(), "never digs while sticky");
        }
        assert_eq!(
            map.tile(2, 3),
            Tile::Mineable,
            "the rock was never dug by the wanderer"
        );
    }

    #[test]
    fn sticky_beast_is_still_blocked_by_solid_rock() {
        let mut map = open_map();
        for y in 1..4 {
            map.set_tile(2, y, Tile::Unbreakable);
        }
        let mut b = Beast::new(center_of((1, 2)), 140.0, 1.6, 0.25);
        b.sticky = true;
        for _ in 0..300 {
            b.update(center_of((4, 2)), &mut map, 1.0 / 60.0);
            assert!(
                b.pos.x < 2.0 * TILE_SIZE,
                "wandering beast cannot pass the wall"
            );
        }
    }

    #[test]
    fn sticky_beast_resumes_pathfinding_when_it_wears_off() {
        let mut map = open_map();
        let mut b = Beast::new(center_of((1, 2)), 140.0, 1.6, 0.25);
        b.sticky = true;
        let player = center_of((3, 2));
        for _ in 0..10 {
            b.update(player, &mut map, 1.0 / 60.0);
        }
        assert!(matches!(b.state, BeastState::Wander { .. }));
        b.sticky = false;
        // Once sticky wears off the beast pathfinds toward the (reachable) player
        // again, so it must end up closer than when it was wandering.
        let start_dist = (b.pos - player).length();
        for _ in 0..120 {
            b.update(player, &mut map, 1.0 / 60.0);
        }
        let end_dist = (b.pos - player).length();
        assert!(
            end_dist < start_dist,
            "after sticky ends the beast moves toward the player (dist {end_dist} vs {start_dist})"
        );
    }
}
