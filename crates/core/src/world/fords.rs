//! Fords: where a trail crosses a mapped stream with no bridge, it goes through the water
//! rather than over a culvert as a road would.
//!
//! The trail dips at the crossing (deepest at the sample nearest the stream, easing back to
//! its own height a sample either side), its bed is stones, and the stream runs on across it
//! at a level a little below the trail's surface either side, so the water is shallow at the
//! edges of the dip and deepest in the middle.
//! Riding through is heavier and slower.

use std::collections::HashMap;

use super::surface::Trail;
use super::{World, OPEN};
use crate::gpx::Route;

/// How far the trail's surface dips at the crossing, in metres.
pub const FORD_DEPTH: f64 = 0.3;
/// How far below the trail's surface either side the water stands.
pub(super) const FORD_WATER: f64 = 0.1;
/// Rolling resistance through a ford, as a multiple of the base (wet stones and water).
pub const FORD_ROLLING: f64 = 6.0;
/// The fastest a rider goes through a ford, in m/s (12 km/h).
pub const FORD_MAX_SPEED: f64 = 12.0 / 3.6;
/// How far either side of the crossing the ford slows a rider, in metres.
pub const FORD_REACH: f64 = 6.0;
/// A stream this nearly parallel to the trail (cosine of the angle between them: within about
/// 25°) runs beside it rather than across it. Smoothing the route turns it a little where a
/// trail bends at a ford, so a real crossing can meet the stream at 40° or less.
const ALONG_COS: f64 = 0.9;
/// Two crossings closer than this many samples are one ford.
const SAME_FORD: usize = 3;
/// Grid cell for finding route segments near a point, in metres.
const CELL: f64 = 10.0;

/// The route samples where a trail fords a stream: for each crossing of a stream line over
/// the route, the nearest sample, if it is open trail and not by a bridge or tunnel.
pub(super) fn fords(route: &Route, streams: &[Vec<(f64, f64)>], trails: &[Option<Trail>], kinds: &[u8]) -> Vec<usize> {
    let n = route.n;
    if n < 2 {
        return Vec::new();
    }
    let cell = |x: f64, z: f64| ((x / CELL).floor() as i32, (z / CELL).floor() as i32);
    // Route segments (sample i to i + 1) by the grid cells their ends fall in.
    let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for i in 0..n - 1 {
        let (a, b) = (cell(route.x[i], route.z[i]), cell(route.x[i + 1], route.z[i + 1]));
        cells.entry(a).or_default().push(i);
        if b != a {
            cells.entry(b).or_default().push(i);
        }
    }
    let open_trail = |i: usize| {
        let near = i.saturating_sub(2)..(i + 3).min(n);
        trails.get(i).is_some_and(Option::is_some) && kinds.get(near).is_some_and(|k| k.iter().all(|&k| k == OPEN))
    };
    let mut found: Vec<usize> = Vec::new();
    for line in streams {
        for w in line.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = (b.0 - a.0).hypot(b.1 - a.1);
            if len < 1e-6 {
                continue;
            }
            // The cells this piece of stream passes through, and their neighbours.
            let mut near: Vec<usize> = Vec::new();
            let steps = (len / (CELL / 2.0)).ceil() as usize;
            for s in 0..=steps {
                let t = s as f64 / steps as f64;
                let (cx, cz) = cell(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        near.extend(cells.get(&(cx + dx, cz + dz)).map_or(&[][..], |v| v));
                    }
                }
            }
            near.sort_unstable();
            near.dedup();
            for i in near {
                let (p, q) = ((route.x[i], route.z[i]), (route.x[i + 1], route.z[i + 1]));
                let Some(t) = crossing(p, q, a, b) else { continue };
                let (rx, rz) = (q.0 - p.0, q.1 - p.1);
                let rlen = rx.hypot(rz).max(1e-9);
                let cos = ((rx * (b.0 - a.0) + rz * (b.1 - a.1)) / (rlen * len)).abs();
                let at = if t < 0.5 { i } else { i + 1 };
                if cos < ALONG_COS && open_trail(at) && !found.iter().any(|&f| f.abs_diff(at) < SAME_FORD) {
                    found.push(at);
                }
            }
        }
    }
    found.sort_unstable();
    found
}

/// Where segment `p→q` crosses segment `a→b`: how far along `p→q` (0 to 1), if they cross.
fn crossing(p: (f64, f64), q: (f64, f64), a: (f64, f64), b: (f64, f64)) -> Option<f64> {
    let (r, s) = ((q.0 - p.0, q.1 - p.1), (b.0 - a.0, b.1 - a.1));
    let cross = r.0 * s.1 - r.1 * s.0;
    if cross.abs() < 1e-12 {
        return None;
    }
    let (ax, az) = (a.0 - p.0, a.1 - p.1);
    let t = (ax * s.1 - az * s.0) / cross;
    let u = (ax * r.1 - az * r.0) / cross;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

impl World {
    /// The route samples where the trail fords a stream.
    pub fn fords(&self) -> &[usize] {
        &self.scene.fords
    }

    /// How far the riding surface dips at sample `i`: a ford's depth at its crossing, else 0.
    pub fn dip(&self, i: usize) -> f64 {
        if self.scene.fords.binary_search(&i).is_ok() {
            FORD_DEPTH
        } else {
            0.0
        }
    }

    /// Per route sample, whether a rider there is in a ford.
    pub fn in_ford(&self) -> Vec<bool> {
        let reach = (FORD_REACH / self.route.step).round() as usize;
        let mut out = vec![false; self.route.n];
        for &f in &self.scene.fords {
            out[f.saturating_sub(reach)..(f + reach + 1).min(self.route.n)].fill(true);
        }
        out
    }

    /// The height of a ford's water at sample `f`: the stream is held at this level near it.
    pub(super) fn ford_level(&self, f: usize) -> f64 {
        self.route.ele[f] as f64 + super::ROAD_LIFT - FORD_WATER
    }
}
