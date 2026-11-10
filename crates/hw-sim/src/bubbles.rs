//! Bubbles: the parts of the world that are simulated right now.
//!
//! Players may be spread over a whole planet, so the server cannot run all of
//! it. Each player, or each group of players close together, gets a bubble:
//! a sphere around them where things move, fall and fight. Players who meet
//! share one bubble; when they drift apart again (further than they had to be
//! close to merge, so a pair on the edge does not flicker) it splits.
//! Outside every bubble the world holds still. Each bubble gets a share of
//! the entity budget by how many players are in it.

use glam::DVec3;
use hw_net::PlayerId;

/// Players closer than this share a bubble.
pub const MERGE_DISTANCE: f64 = 250.0;
/// Players who share a bubble keep it until they are this far apart.
pub const SPLIT_DISTANCE: f64 = 400.0;
/// How far around its players a bubble reaches.
pub const BUBBLE_MARGIN: f64 = 300.0;
/// Moving entities (props now; citizens, enemies and debris later) the
/// whole server simulates at once, shared among the bubbles.
pub const ACTIVE_BUDGET: usize = 2000;

#[derive(Clone, Debug, PartialEq)]
pub struct Bubble {
    /// The lowest player ID in it (stable while that player stays).
    pub id: PlayerId,
    /// Sorted.
    pub members: Vec<PlayerId>,
    pub center: DVec3,
    pub radius: f64,
    /// Moving entities this bubble may simulate.
    pub budget: usize,
}

impl Bubble {
    pub fn contains(&self, p: DVec3) -> bool {
        p.distance_squared(self.center) <= self.radius * self.radius
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bubbles {
    pub list: Vec<Bubble>,
}

impl Bubbles {
    /// Regroups the players (any order) into bubbles. Deterministic: the
    /// result depends only on the positions and the previous bubbles.
    pub fn update(&mut self, players: &[(PlayerId, DVec3)]) {
        let mut players = players.to_vec();
        players.sort_by_key(|(p, _)| *p);
        players.dedup_by_key(|(p, _)| *p);
        let n = players.len();
        let before = |p: PlayerId| self.of(p).map(|b| b.id);
        let mut parent: Vec<usize> = (0..n).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for i in 0..n {
            for j in i + 1..n {
                let d = players[i].1.distance(players[j].1);
                let together =
                    before(players[i].0).is_some() && before(players[i].0) == before(players[j].0);
                if d < MERGE_DISTANCE || (together && d < SPLIT_DISTANCE) {
                    let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                    parent[a.max(b)] = a.min(b);
                }
            }
        }
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut group_of = vec![usize::MAX; n];
        for i in 0..n {
            let r = root(&mut parent, i);
            if group_of[r] == usize::MAX {
                group_of[r] = groups.len();
                groups.push(Vec::new());
            }
            groups[group_of[r]].push(i);
        }
        self.list = groups
            .into_iter()
            .map(|g| {
                let center = g.iter().map(|&i| players[i].1).sum::<DVec3>() / g.len() as f64;
                let reach = g
                    .iter()
                    .map(|&i| players[i].1.distance(center))
                    .fold(0.0, f64::max);
                Bubble {
                    id: players[g[0]].0,
                    members: g.iter().map(|&i| players[i].0).collect(),
                    center,
                    radius: reach + BUBBLE_MARGIN,
                    budget: ACTIVE_BUDGET * g.len() / n.max(1),
                }
            })
            .collect();
    }

    /// The bubble a player is in.
    pub fn of(&self, p: PlayerId) -> Option<&Bubble> {
        self.list.iter().find(|b| b.members.contains(&p))
    }

    /// The bubble around a point, nearest centre first if several overlap.
    pub fn covering(&self, p: DVec3) -> Option<&Bubble> {
        self.list.iter().filter(|b| b.contains(p)).min_by(|a, b| {
            a.center
                .distance_squared(p)
                .total_cmp(&b.center.distance_squared(p))
        })
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64) -> DVec3 {
        DVec3::new(x, 0.0, 0.0)
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn players_far_apart_get_their_own_bubbles() {
        let mut b = Bubbles::default();
        b.update(&[(PlayerId(2), at(0.0)), (PlayerId(1), at(5000.0))]);
        assert_eq!(b.len(), 2);
        assert_eq!(b.of(PlayerId(1)).unwrap().members, vec![PlayerId(1)]);
        assert_eq!(
            b.list.iter().map(|x| x.budget).sum::<usize>(),
            ACTIVE_BUDGET
        );
        assert!(b.covering(at(5100.0)).is_some());
        assert!(b.covering(at(2500.0)).is_none());
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn bubbles_merge_when_players_meet_and_split_with_a_margin() {
        let mut b = Bubbles::default();
        let (p, q) = (PlayerId(1), PlayerId(2));
        b.update(&[(p, at(0.0)), (q, at(1000.0))]);
        assert_eq!(b.len(), 2);
        // They meet.
        b.update(&[(p, at(0.0)), (q, at(200.0))]);
        assert_eq!(b.len(), 1);
        let shared = b.of(q).unwrap();
        assert_eq!(shared.id, p);
        assert_eq!(shared.budget, ACTIVE_BUDGET);
        assert!(shared.contains(at(-250.0)) && shared.contains(at(450.0)));
        // Drifting a bit further than the merge distance keeps them together...
        b.update(&[(p, at(0.0)), (q, at(350.0))]);
        assert_eq!(b.len(), 1);
        // ...until they are clearly apart.
        b.update(&[(p, at(0.0)), (q, at(450.0))]);
        assert_eq!(b.len(), 2);
        // And apart, 350 m is not close enough to merge again.
        b.update(&[(p, at(0.0)), (q, at(350.0))]);
        assert_eq!(b.len(), 2);
    }

    #[test]
    #[ignore = "daily (gameplay): scripts/check.sh --daily, and CI's first run of the day"]
    fn a_chain_of_players_is_one_bubble() {
        let mut b = Bubbles::default();
        let players: Vec<_> = (1..=5)
            .map(|k| (PlayerId(k), at(k as f64 * 200.0)))
            .collect();
        b.update(&players);
        assert_eq!(b.len(), 1);
        assert_eq!(b.list[0].members.len(), 5);
    }
}
