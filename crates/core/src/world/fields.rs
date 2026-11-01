//! Field boundaries: hedgerows, dry-stone walls and fences. The ones mapped near the route
//! (`barrier=hedge|wall|fence`, from the Overpass API) stand where the map has them; elsewhere
//! a pattern of fields is made up in the style of the region: hedgerows in Britain, Ireland
//! and the bocage of north-west France, dry-stone walls in their uplands and round the
//! Mediterranean, timber fences in the mountains and the north.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use super::height::{fbm, hash2, smoothstep};
use super::mesh::TriMesh;
use super::{linear, CHUNK};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Boundary {
    Hedge,
    Wall,
    Fence,
}

impl Boundary {
    /// From a way's `barrier` tag, and its `wall` tag for walls. Retaining and noise walls
    /// aren't field boundaries; nor are kerbs, gates and the rest.
    pub fn from_osm(barrier: &str, wall: Option<&str>) -> Option<Self> {
        match barrier {
            "hedge" | "hedge_bank" => Some(Boundary::Hedge),
            "wall" if !matches!(wall, Some("retaining" | "noise_barrier" | "flood_wall" | "castle_wall")) => Some(Boundary::Wall),
            "fence" => Some(Boundary::Fence),
            _ => None,
        }
    }
}

/// A field boundary mapped near the route, as `[lat, lon]` points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MappedBoundary {
    pub kind: Boundary,
    pub line: Vec<[f64; 2]>,
}

/// The hedges, walls and fences in an Overpass reply to [`super::way_query`].
pub fn parse_boundaries(json: &str) -> Result<Vec<MappedBoundary>, String> {
    let reply: super::surface::Reply = serde_json::from_str(json).map_err(|e| format!("Unexpected reply from the OpenStreetMap server: {e}"))?;
    Ok(reply
        .elements
        .into_iter()
        .filter(|e| e.geometry.len() >= 2)
        .filter_map(|e| {
            let kind = Boundary::from_osm(e.tags.get("barrier")?, e.tags.get("wall").map(String::as_str))?;
            Some(MappedBoundary { kind, line: e.geometry.iter().map(|p| [p.lat, p.lon]).collect() })
        })
        .collect())
}

/// Boundaries are drawn in pieces this long at most, each standing on the ground at its ends.
pub(super) const PIECE: f64 = 6.0;

/// One unbroken stretch of boundary, as points in the local frame.
pub(super) struct Run {
    pub kind: Boundary,
    pub points: Vec<(f64, f64)>,
}

/// A stretch of a made-up field's edge, before it is known what stands there.
pub(super) struct Edge {
    pub points: Vec<(f64, f64)>,
    /// The middle of the whole edge (which may reach into other chunks), to decide its kind by.
    pub middle: (f64, f64),
    /// The edge's own random number, in [0, 1).
    pub roll: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Region {
    /// Britain and Ireland.
    Hedgerows,
    /// The bocage of Normandy, Brittany and the Loire: small fields, all hedged.
    Bocage,
    /// Terraces and stony fields.
    Mediterranean,
    /// Scandinavia.
    North,
    /// The rest of Europe: big open fields, the odd hedge or fence.
    Continental,
    Elsewhere,
}

impl Region {
    pub fn at(lat: f64, lon: f64) -> Region {
        let (lat_in, lon_in) = (|lo: f64, hi: f64| lat >= lo && lat < hi, |lo: f64, hi: f64| lon >= lo && lon < hi);
        if lat_in(49.8, 61.0) && lon_in(-11.0, 2.0) {
            Region::Hedgerows
        } else if lat_in(46.5, 49.8) && lon_in(-5.5, 0.5) {
            Region::Bocage
        } else if lat_in(34.0, 44.5) && lon_in(-10.0, 36.0) {
            Region::Mediterranean
        } else if lat_in(58.0, 72.0) && lon_in(4.0, 32.0) {
            Region::North
        } else if lat_in(34.0, 72.0) && lon_in(-11.0, 45.0) {
            Region::Continental
        } else {
            Region::Elsewhere
        }
    }
}

/// How fields are laid out and bounded where a ride goes.
#[derive(Debug, Clone, Copy)]
pub(super) struct Countryside {
    pub region: Region,
    tree_line: f64,
    /// Which way the field pattern runs, the same for a whole ride.
    angle: f64,
    /// A field's rough width, metres.
    size: f64,
    /// The share of field edges that have a boundary at all.
    density: f64,
}

impl Countryside {
    pub fn new(lat: f64, lon: f64, tree_line: f64) -> Self {
        let region = Region::at(lat, lon);
        let (size, density) = match region {
            Region::Hedgerows => (110.0, 0.85),
            Region::Bocage => (85.0, 0.9),
            Region::Mediterranean => (70.0, 0.5),
            Region::North => (120.0, 0.45),
            Region::Continental => (150.0, 0.3),
            Region::Elsewhere => (180.0, 0.45),
        };
        let angle = hash2((lat * 100.0) as i32, (lon * 100.0) as i32) * PI;
        Countryside { region, tree_line, angle, size, density }
    }

    /// What bounds a field edge whose middle is `height` metres up; `roll` is the edge's own
    /// random number. `None` on bare mountainside, above the trees.
    pub fn kind(&self, height: f64, roll: f64) -> Option<Boundary> {
        if height > self.tree_line + 50.0 {
            return None;
        }
        // Mountain pastures are fenced, outside Britain and Ireland, whose hills are walled.
        let mountain = smoothstep(800.0, 1400.0, height);
        Some(match self.region {
            Region::Hedgerows if roll < smoothstep(200.0, 330.0, height) => Boundary::Wall,
            Region::Hedgerows | Region::Bocage if roll > 0.93 => Boundary::Fence,
            Region::Hedgerows | Region::Bocage => Boundary::Hedge,
            _ if roll < mountain => Boundary::Fence,
            Region::Mediterranean if roll < 0.75 => Boundary::Wall,
            Region::Continental if roll < 0.55 => Boundary::Hedge,
            Region::Mediterranean => Boundary::Hedge,
            Region::North | Region::Continental | Region::Elsewhere => Boundary::Fence,
        })
    }

    /// Whether walls are of warmer stone: round the Mediterranean.
    pub fn warm(&self) -> bool {
        self.region == Region::Mediterranean
    }

    /// The made-up field edges in chunk `(cx, cz)`: a jittered grid turned to the ride's own
    /// angle, its edges wandering a little, cut into pieces. A piece belongs to the chunk holding
    /// its middle, so neighbouring chunks share nothing and miss nothing.
    pub fn edges(&self, cx: i32, cz: i32) -> Vec<Edge> {
        let (c, s) = (self.angle.cos(), self.angle.sin());
        let to_grid = |x: f64, z: f64| (x * c + z * s, -x * s + z * c);
        let from_grid = |u: f64, v: f64| (u * c - v * s, u * s + v * c);
        let (x0, z0) = (cx as f64 * CHUNK, cz as f64 * CHUNK);
        let corners = [(x0, z0), (x0 + CHUNK, z0), (x0, z0 + CHUNK), (x0 + CHUNK, z0 + CHUNK)].map(|(x, z)| to_grid(x, z));
        let range = |f: fn(&(f64, f64)) -> f64| {
            let (lo, hi) = corners.iter().map(f).fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
            ((lo / self.size).floor() as i32 - 1, (hi / self.size).ceil() as i32 + 1)
        };
        let ((i0, i1), (j0, j1)) = (range(|p| p.0), range(|p| p.1));
        let size = self.size;
        let corner = |i: i32, j: i32| {
            let jitter = |a: i32, b: i32| (hash2(a, b) - 0.5) * 0.5 * size;
            from_grid(i as f64 * size + jitter(i.wrapping_mul(7) + 1, j.wrapping_mul(13) - 3), j as f64 * size + jitter(i.wrapping_mul(11) - 5, j.wrapping_mul(3) + 2))
        };
        let inside = |p: (f64, f64)| p.0 >= x0 && p.0 < x0 + CHUNK && p.1 >= z0 && p.1 < z0 + CHUNK;
        let mut edges = Vec::new();
        for j in j0..j1 {
            for i in i0..i1 {
                for (di, dj) in [(1, 0), (0, 1)] {
                    let (si, sj) = (i.wrapping_mul(2) + di, j.wrapping_mul(2) + dj);
                    if hash2(si.wrapping_mul(17) + 3, sj.wrapping_mul(29) - 1) > self.density {
                        continue;
                    }
                    let (a, b) = (corner(i, j), corner(i + di, j + dj));
                    let len = (b.0 - a.0).hypot(b.1 - a.1);
                    let n = (len / PIECE).ceil().max(1.0) as usize;
                    let across = ((a.1 - b.1) / len, (b.0 - a.0) / len);
                    let points: Vec<(f64, f64)> = (0..=n)
                        .map(|k| {
                            let t = k as f64 / n as f64;
                            let p = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                            let wander = (PI * t).sin() * 4.0 * fbm(p.0 / 45.0 + 1.7, p.1 / 45.0 - 2.3);
                            (p.0 + across.0 * wander, p.1 + across.1 * wander)
                        })
                        .collect();
                    let (middle, roll) = (((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), hash2(si.wrapping_mul(5) - 7, sj.wrapping_mul(23) + 11));
                    for run in runs(&points, |p, q| inside(((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0))) {
                        edges.push(Edge { points: run, middle, roll });
                    }
                }
            }
        }
        edges
    }
}

/// Splits a line into the unbroken runs of pieces (from one point to the next) that `keep` keeps.
pub(super) fn runs(points: &[(f64, f64)], keep: impl Fn((f64, f64), (f64, f64)) -> bool) -> Vec<Vec<(f64, f64)>> {
    let mut out: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut open = false;
    for w in points.windows(2) {
        if !keep(w[0], w[1]) {
            open = false;
            continue;
        }
        match out.last_mut() {
            Some(run) if open => run.push(w[1]),
            _ => out.push(vec![w[0], w[1]]),
        }
        open = true;
    }
    out
}

/// A hedge's or wall's cross-section, from one foot over the top to the other (across, up);
/// the feet are this far below the ground, and the rest is scaled by its height, `tall.0` plus
/// up to `tall.1`. Colours, in sRGB: at the foot, on the sides, on top.
struct Section {
    profile: &'static [(f64, f64)],
    tall: (f64, f64),
    colours: [[f64; 3]; 3],
}

const HEDGE: Section =
    Section { profile: &[(-0.85, -0.5), (-0.8, 0.65), (0.0, 1.0), (0.8, 0.65), (0.85, -0.5)], tall: (1.5, 0.7), colours: [[0.16, 0.25, 0.11], [0.24, 0.37, 0.16], [0.30, 0.44, 0.19]] };
const WALL: Section = Section { profile: &[(-0.34, -0.4), (-0.26, 1.0), (0.26, 1.0), (0.34, -0.4)], tall: (1.0, 0.3), colours: [[0.38, 0.37, 0.34], [0.52, 0.51, 0.47], [0.44, 0.43, 0.40]] };
const WARM_WALL: Section = Section { colours: [[0.52, 0.46, 0.37], [0.66, 0.59, 0.47], [0.58, 0.53, 0.44]], ..WALL };

/// Adds a run of boundary to `mesh`, standing on the ground at `ys` (one height per point).
/// `seed` varies its height and colour; `warm` makes a wall of warmer stone.
pub(super) fn draw(mesh: &mut TriMesh, kind: Boundary, points: &[(f64, f64)], ys: &[f64], seed: i32, warm: bool) {
    let n = points.len();
    if n < 2 {
        return;
    }
    let r = |k: usize, salt: i32| hash2(seed.wrapping_mul(31).wrapping_add(k as i32), salt);
    // Across each point: the average of the pieces either side, so bends don't open gaps.
    let across: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            let (a, b) = (points[k.saturating_sub(1)], points[(k + 1).min(n - 1)]);
            let len = (b.0 - a.0).hypot(b.1 - a.1).max(1e-9);
            ((a.1 - b.1) / len, (b.0 - a.0) / len)
        })
        .collect();
    let at = |k: usize, (side, up): (f64, f64)| [points[k].0 + across[k].0 * side, ys[k] + up, points[k].1 + across[k].1 * side];
    let along = |k: usize| {
        let (a, b) = (points[k], points[k + 1]);
        let len = (b.0 - a.0).hypot(b.1 - a.1).max(1e-9);
        ((b.0 - a.0) / len, (b.1 - a.1) / len)
    };
    match kind {
        Boundary::Hedge | Boundary::Wall => {
            // Sunk into the ground a little, so a slope across it leaves no gap.
            let Section { profile, tall, colours } = match kind {
                Boundary::Hedge => HEDGE,
                _ if warm => WARM_WALL,
                _ => WALL,
            };
            let height: Vec<f64> = (0..n).map(|k| tall.0 + tall.1 * r(k, 3)).collect();
            let shade: Vec<f64> = (0..n).map(|k| 0.85 + 0.3 * r(k, 5)).collect();
            let place = |k: usize, q: (f64, f64)| at(k, (q.0, if q.1 < 0.0 { q.1 } else { q.1 * height[k] }));
            let colour = |k: usize, q: usize| {
                let c = colours[if profile[q].1 < 0.0 {
                    0
                } else if q == 0 || q + 1 == profile.len() || profile[q].1 < 0.66 {
                    1
                } else {
                    2
                }];
                linear(c.map(|v| (v * shade[k]).min(1.0)))
            };
            for (k, &(ax, az)) in across.iter().enumerate().take(n - 1) {
                for q in 0..profile.len() - 1 {
                    let (dl, du) = (profile[q + 1].0 - profile[q].0, profile[q + 1].1 - profile[q].1);
                    let facing = [-du * ax, dl, -du * az];
                    mesh.quad(
                        [place(k, profile[q]), place(k + 1, profile[q]), place(k + 1, profile[q + 1]), place(k, profile[q + 1])],
                        facing,
                        [colour(k, q), colour(k + 1, q), colour(k + 1, q + 1), colour(k, q + 1)],
                    );
                }
            }
            // The ends.
            for (k, d) in [(0, along(0)), (n - 1, along(n - 2))] {
                let out = if k == 0 { [-d.0, 0.0, -d.1] } else { [d.0, 0.0, d.1] };
                for q in 1..profile.len() - 1 {
                    let c = colour(k, 1);
                    mesh.triangle([place(k, profile[0]), place(k, profile[q]), place(k, profile[q + 1])], out, [c, c, c], [[0.0; 2]; 3]);
                }
            }
        }
        Boundary::Fence => {
            // Posts at every point and half way to the next, two rails between.
            let wood = if r(0, 9) < 0.5 { [0.45, 0.36, 0.27] } else { [0.50, 0.48, 0.44] };
            for half in 0..2 * n - 1 {
                let k = half / 2;
                let (p, y) = if half % 2 == 0 { (points[k], ys[k]) } else { (((points[k].0 + points[k + 1].0) / 2.0, (points[k].1 + points[k + 1].1) / 2.0), (ys[k] + ys[k + 1]) / 2.0) };
                let c = linear(wood.map(|v| v * (0.85 + 0.3 * r(half, 5))));
                let (d, x) = (if k + 1 < n { along(k) } else { along(k - 1) }, across[k]);
                let corner = |s: f64, t: f64, up: f64| [p.0 + d.0 * s + x.0 * t, y + up, p.1 + d.1 * s + x.1 * t];
                const W: f64 = 0.07;
                for (s0, t0, s1, t1, facing) in [(-W, -W, W, -W, [-x.0, 0.0, -x.1]), (W, W, -W, W, [x.0, 0.0, x.1]), (-W, W, -W, -W, [-d.0, 0.0, -d.1]), (W, -W, W, W, [d.0, 0.0, d.1])] {
                    mesh.quad([corner(s0, t0, -0.3), corner(s1, t1, -0.3), corner(s1, t1, 1.25), corner(s0, t0, 1.25)], facing, [c; 4]);
                }
            }
            for (k, &x) in across.iter().enumerate().take(n - 1) {
                let c = linear(wood.map(|v| v * (0.85 + 0.3 * r(k, 7))));
                for up in [0.5, 1.0] {
                    let rail = |j: usize, dy: f64| [points[j].0, ys[j] + up + dy, points[j].1];
                    let (a0, a1, b0, b1) = (rail(k, 0.0), rail(k, 0.1), rail(k + 1, 0.0), rail(k + 1, 0.1));
                    mesh.quad([a0, b0, b1, a1], [x.0, 0.0, x.1], [c; 4]);
                    mesh.quad([a0, a1, b1, b0], [-x.0, 0.0, -x.1], [c; 4]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries_follow_the_region() {
        let devon = Countryside::new(50.7, -3.5, 1700.0);
        assert_eq!(devon.region, Region::Hedgerows);
        // Hedges in the valleys, mostly walls on the moors.
        assert_eq!(devon.kind(100.0, 0.5), Some(Boundary::Hedge));
        assert_eq!(devon.kind(450.0, 0.5), Some(Boundary::Wall));
        assert_eq!(Region::at(48.4, -2.0), Region::Bocage);
        // Fences on alpine pastures, nothing above the trees; walls in Provence.
        let alps = Countryside::new(45.1, 6.1, 1900.0);
        assert_eq!(alps.kind(1500.0, 0.5), Some(Boundary::Fence));
        assert_eq!(alps.kind(2100.0, 0.5), None);
        assert_eq!(Countryside::new(43.7, 5.5, 2000.0).kind(300.0, 0.3), Some(Boundary::Wall));
        assert_eq!(Region::at(61.0, 10.0), Region::North);
    }

    #[test]
    fn neighbouring_chunks_share_no_piece_of_a_field_edge() {
        let c = Countryside::new(50.7, -3.5, 1700.0);
        let pieces = |cx: i32, cz: i32| -> Vec<((i64, i64), (i64, i64))> {
            let key = |p: (f64, f64)| ((p.0 * 100.0).round() as i64, (p.1 * 100.0).round() as i64);
            c.edges(cx, cz).iter().flat_map(|e| e.points.windows(2).map(|w| (key(w[0]), key(w[1]))).collect::<Vec<_>>()).collect()
        };
        let (a, b) = (pieces(0, 0), pieces(1, 0));
        assert!(!a.is_empty() && !b.is_empty());
        assert!(a.iter().all(|p| !b.contains(p)));
        // And each piece's middle is in its own chunk.
        for e in c.edges(1, 0) {
            for w in e.points.windows(2) {
                let mid = ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0);
                assert!(mid.0 >= CHUNK && mid.0 < 2.0 * CHUNK && mid.1 >= 0.0 && mid.1 < CHUNK);
            }
        }
    }
}
