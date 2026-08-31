//! Pure grid pathfinding (4-neighbour A\*).
//!
//! Used by map generation to guarantee a corridor from the start door to the
//! exit door, and reused later (M3) by the beast AI. It is generic over the
//! passability rule and fully deterministic (fixed neighbour order and stable
//! tie-breaking), so a seeded generation is reproducible.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

/// Fixed 4-neighbour order. Kept constant so A\* tie-breaking is stable.
const NEIGHBORS: [(i32, i32); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];

/// A node in the open list. `f` is the A\* score; `seq` breaks ties in
/// insertion order so the same `is_passable` field always yields the same path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Node {
    f: i32,
    x: i32,
    y: i32,
    seq: u64,
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max-heap; reverse `f` so the smallest score pops first,
        // then reverse `seq` so earlier-inserted nodes win ties.
        other.f.cmp(&self.f).then_with(|| other.seq.cmp(&self.seq))
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Grid A\* from `start` to `goal` over cells where `is_passable(x, y)` is true.
/// Returns the path **including** both endpoints, or `None` if unreachable.
pub fn astar(
    start: (i32, i32),
    goal: (i32, i32),
    is_passable: impl Fn(i32, i32) -> bool,
) -> Option<Vec<(i32, i32)>> {
    if start == goal {
        return Some(vec![start]);
    }
    if !is_passable(start.0, start.1) || !is_passable(goal.0, goal.1) {
        return None;
    }

    let mut open = BinaryHeap::new();
    let mut g_score: HashMap<(i32, i32), i32> = HashMap::new();
    let mut came_from: HashMap<(i32, i32), (i32, i32)> = HashMap::new();

    g_score.insert(start, 0);
    open.push(Node {
        f: heuristic(start, goal),
        x: start.0,
        y: start.1,
        seq: 0,
    });
    let mut seq = 1u64;

    while let Some(node) = open.pop() {
        let cur = (node.x, node.y);
        if cur == goal {
            return Some(reconstruct(came_from, cur));
        }

        let cur_g = g_score[&cur];
        for (dx, dy) in NEIGHBORS {
            let next = (cur.0 + dx, cur.1 + dy);
            if !is_passable(next.0, next.1) {
                continue;
            }
            let tentative = cur_g + 1;
            let better = match g_score.get(&next) {
                Some(&g) => tentative < g,
                None => true,
            };
            if better {
                g_score.insert(next, tentative);
                came_from.insert(next, cur);
                open.push(Node {
                    f: tentative + heuristic(next, goal),
                    x: next.0,
                    y: next.1,
                    seq,
                });
                seq += 1;
            }
        }
    }

    None
}

fn heuristic(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

fn reconstruct(came_from: HashMap<(i32, i32), (i32, i32)>, goal: (i32, i32)) -> Vec<(i32, i32)> {
    let mut path = vec![goal];
    let mut cur = goal;
    while let Some(&prev) = came_from.get(&cur) {
        path.push(prev);
        cur = prev;
    }
    path.reverse();
    path
}

/// Returns true if `goal` is reachable from `start` over passable cells.
pub fn has_path(
    start: (i32, i32),
    goal: (i32, i32),
    is_passable: impl Fn(i32, i32) -> bool,
) -> bool {
    astar(start, goal, is_passable).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 5x5 grid; only cells matching the mask are passable.
    fn grid_passable(mask: &[[bool; 5]; 5]) -> impl Fn(i32, i32) -> bool {
        let mask = *mask;
        move |x: i32, y: i32| -> bool {
            x >= 0 && y >= 0 && (x as usize) < 5 && (y as usize) < 5 && mask[y as usize][x as usize]
        }
    }

    #[test]
    fn finds_path_around_wall() {
        // Column x==2 is a wall everywhere except the bottom row, so a path
        // must detour around it (through row 4).
        let mut mask = [[true; 5]; 5];
        for row in &mut mask[..4] {
            row[2] = false;
        }
        let path = astar((0, 0), (4, 4), grid_passable(&mask)).expect("a path exists");
        // The path must be a valid, contiguous set of cells.
        assert_eq!(*path.first().unwrap(), (0, 0));
        assert_eq!(*path.last().unwrap(), (4, 4));
        for w in path.windows(2) {
            let (a, b) = (w[0], w[1]);
            let dist = (a.0 - b.0).abs() + (a.1 - b.1).abs();
            assert_eq!(
                dist, 1,
                "path must move cardinally one cell at a time: {a:?}->{b:?}"
            );
        }
        // It never steps onto the walled part of the column (x==2, y<4).
        assert!(path.iter().all(|&(x, y)| !(x == 2 && y < 4)));
    }

    #[test]
    fn returns_none_when_fully_blocked() {
        // Whole column is a wall, so left and right halves are disconnected.
        let mut mask = [[true; 5]; 5];
        for row in &mut mask {
            row[2] = false;
        }
        assert!(astar((0, 1), (4, 1), grid_passable(&mask)).is_none());
    }

    #[test]
    fn has_path_matches_astar() {
        let partial = {
            let mut m = [[true; 5]; 5];
            for row in &mut m[..4] {
                row[2] = false;
            }
            m
        };
        let full = {
            let mut m = [[true; 5]; 5];
            for row in &mut m {
                row[2] = false;
            }
            m
        };
        assert!(has_path((0, 0), (4, 4), grid_passable(&partial)));
        assert!(has_path((0, 0), (0, 0), grid_passable(&full)));
        assert!(!has_path((0, 1), (4, 1), grid_passable(&full)));
    }

    #[test]
    fn start_equals_goal_returns_single_cell_path() {
        assert_eq!(
            astar((2, 2), (2, 2), grid_passable(&[[true; 5]; 5])),
            Some(vec![(2, 2)])
        );
    }

    #[test]
    fn is_deterministic_for_same_input() {
        let mut mask = [[true; 5]; 5];
        for row in &mut mask {
            row[2] = false;
        }
        let a = astar((0, 0), (4, 4), grid_passable(&mask));
        let b = astar((0, 0), (4, 4), grid_passable(&mask));
        assert_eq!(a, b);
    }
}
