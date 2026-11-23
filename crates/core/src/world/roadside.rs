//! The roadside of a tropical route: what kind of road each stretch is (a built-up avenue,
//! an open divided road, a rural road, a town street), worked out from what the world already
//! knows, so that palms, lamps and barriers can be placed the way they are there.
//!
//! The kinds come only from the map: whether the route runs on one carriageway of a divided
//! road (another one-way carriageway close beside it, nearly parallel) and whether buildings
//! stand nearby. Nothing here changes a route outside the tropics: the rules that use the
//! plan look at `tropical` and `americas` first.

use std::collections::HashMap;

use super::buildings::Buildings;
use super::scenery::Scene;
use super::surface::{runs, tidy};
use super::tropics::{in_americas, tropical_weight};
use super::ROAD_WIDTH;
use crate::gpx::Route;

/// What kind of road a stretch of the route is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stretch {
    /// A divided road through a built-up area: a planted median, lamps, sidewalks.
    Avenue,
    /// A divided road through open country: a concrete barrier down the middle.
    Periurban,
    /// A single carriageway through open country.
    Rural,
    /// A single carriageway through a built-up area.
    Town,
}

/// What the rider has chosen for the roadside.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoadsideStyle {
    /// The road as it is on an ordinary day: no race barriers, kerbs and sidewalks instead.
    pub open_road: bool,
}

/// The roadside along the route, one entry per route sample.
#[derive(Clone, Debug)]
pub struct RoadsidePlan {
    pub stretch: Vec<Stretch>,
    /// Where the route is one carriageway of a divided road: metres between the two
    /// carriageways' inner edges.
    pub median: Vec<Option<f64>>,
    /// Which side the other carriageway is on: `1.0` the rider's right, `-1.0` the left, `0.0` none.
    pub median_side: Vec<f64>,
    /// In the lowland tropics.
    pub tropical: bool,
    /// In the Americas.
    pub americas: bool,
    pub style: RoadsideStyle,
}

/// Another carriageway counts if it is this nearly parallel to the route (as a cosine)…
const PARALLEL_COS: f64 = 0.94;
/// …its centre line this far beyond the route's edge…
const MEDIAN_REACH: (f64, f64) = (1.0, 40.0);
/// …for at least this far along the route.
const DIVIDED_MIN_M: f64 = 60.0;
/// A sample is built up when this many buildings stand within `URBAN_REACH_M` of it.
const URBAN_BUILDINGS: usize = 6;
const URBAN_REACH_M: f64 = 150.0;
/// Where to look for housing on the map round a sample: `(distance, angle)`, out to 120 m.
const HOUSING_PROBES: [(f64, f64); 16] = {
    const Q: f64 = std::f64::consts::FRAC_PI_4;
    [
        (40.0, 0.0),
        (40.0, 2.0 * Q),
        (40.0, 4.0 * Q),
        (40.0, 6.0 * Q),
        (80.0, 0.0),
        (80.0, Q),
        (80.0, 2.0 * Q),
        (80.0, 3.0 * Q),
        (80.0, 4.0 * Q),
        (80.0, 5.0 * Q),
        (80.0, 6.0 * Q),
        (80.0, 7.0 * Q),
        (120.0, 0.0),
        (120.0, 2.0 * Q),
        (120.0, 4.0 * Q),
        (120.0, 6.0 * Q),
    ]
};
/// Changes of stretch shorter than this are smoothed away.
const STRETCH_MIN_M: f64 = 200.0;
/// Route samples are binned in squares this wide to find the ones near a carriageway.
const BIN_M: f64 = 50.0;

/// The route's direction at sample `i`, as a unit vector in `(x, z)`.
fn tangent(route: &Route, i: usize) -> (f64, f64) {
    let (a, b) = (i.saturating_sub(1), (i + 1).min(route.n - 1));
    let (tx, tz) = (route.x[b] - route.x[a], route.z[b] - route.z[a]);
    let len = tx.hypot(tz).max(1e-9);
    (tx / len, tz / len)
}

/// Works out the roadside along the route from its scenery and buildings.
pub fn plan(route: &Route, scene: &Scene, buildings: &Buildings, style: RoadsideStyle) -> RoadsidePlan {
    let n = route.n;
    let half = |i: usize| scene.widths.get(i).copied().unwrap_or(ROAD_WIDTH) / 2.0;

    // The nearest other carriageway beside each sample: (signed offset to the right, its half width).
    let mut bins: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for i in 0..n {
        bins.entry(((route.x[i] / BIN_M).floor() as i32, (route.z[i] / BIN_M).floor() as i32)).or_default().push(i);
    }
    let mut beside: Vec<Option<(f64, f64)>> = vec![None; n];
    for (a, b, strip_half) in scene.oneway_strips() {
        let (dx, dz) = (b.0 - a.0, b.1 - a.1);
        let len = dx.hypot(dz);
        if len < 1e-6 {
            continue;
        }
        let (ux, uz) = (dx / len, dz / len);
        let reach = len / 2.0 + MEDIAN_REACH.1 + BIN_M;
        let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        let (bx0, bx1) = (((mid.0 - reach) / BIN_M).floor() as i32, ((mid.0 + reach) / BIN_M).floor() as i32);
        let (bz0, bz1) = (((mid.1 - reach) / BIN_M).floor() as i32, ((mid.1 + reach) / BIN_M).floor() as i32);
        for bx in bx0..=bx1 {
            for bz in bz0..=bz1 {
                for &i in bins.get(&(bx, bz)).into_iter().flatten() {
                    let (tx, tz) = tangent(route, i);
                    if (tx * ux + tz * uz).abs() < PARALLEL_COS {
                        continue;
                    }
                    // The foot of the sample on the strip must fall within it.
                    let (px, pz) = (route.x[i] - a.0, route.z[i] - a.1);
                    let along = px * ux + pz * uz;
                    if !(-2.0..=len + 2.0).contains(&along) {
                        continue;
                    }
                    let (fx, fz) = (a.0 + ux * along, a.1 + uz * along);
                    // To the rider's right is (−tz, tx).
                    let right = (fx - route.x[i]) * -tz + (fz - route.z[i]) * tx;
                    let gap = right.abs() - half(i);
                    if !(MEDIAN_REACH.0..=MEDIAN_REACH.1).contains(&gap) {
                        continue;
                    }
                    if beside[i].is_none_or(|(o, _)| right.abs() < o.abs()) {
                        beside[i] = Some((right, strip_half));
                    }
                }
            }
        }
    }
    // Only carriageways alongside for a good stretch: a slip road or junction is not a median.
    let min_divided = (DIVIDED_MIN_M / route.step).ceil() as usize;
    let alongside: Vec<bool> = beside.iter().map(Option::is_some).collect();
    for (from, to, on) in runs(&alongside) {
        if on && to - from < min_divided {
            beside[from..to].fill(None);
        }
    }
    let median: Vec<Option<f64>> = (0..n).map(|i| beside[i].map(|(o, h)| (o.abs() - half(i) - h).max(0.0))).collect();
    let median_side: Vec<f64> = beside.iter().map(|b| b.map_or(0.0, |(o, _)| o.signum())).collect();

    // Built up: enough buildings nearby, or housing or shops on the map within reach (the map
    // often has an estate's outline long before anyone has drawn its houses).
    let housing_near = |i: usize| {
        let (x, z) = (route.x[i], route.z[i]);
        HOUSING_PROBES.iter().any(|&(r, a): &(f64, f64)| {
            let (px, pz) = (x + r * a.cos(), z + r * a.sin());
            scene.residential.at(px, pz) || scene.commercial.at(px, pz)
        })
    };
    let urban: Vec<bool> = (0..n).map(|i| buildings.count_near(route.x[i], route.z[i], URBAN_REACH_M) >= URBAN_BUILDINGS || housing_near(i)).collect();
    let mut stretch: Vec<Stretch> = (0..n)
        .map(|i| match (median[i].is_some(), urban[i]) {
            (true, true) => Stretch::Avenue,
            (true, false) => Stretch::Periurban,
            (false, true) => Stretch::Town,
            (false, false) => Stretch::Rural,
        })
        .collect();
    tidy(&mut stretch, (STRETCH_MIN_M / route.step).ceil() as usize);

    let mut heights: Vec<f64> = route.ele.iter().map(|&e| e as f64).collect();
    heights.sort_by(f64::total_cmp);
    let typical = heights.get(heights.len() / 2).copied().unwrap_or(0.0);
    RoadsidePlan { stretch, median, median_side, tropical: tropical_weight(route.lat0.abs(), typical) > 0.5, americas: in_americas(route.lon0), style }
}

/// Lamps stand this far apart along an avenue or a town street, the first this far in.
const LAMP_SPACING_M: f64 = 38.0;
const LAMP_START_M: f64 = 19.0;
/// An avenue's median needs to be this wide for lamps, and for palms.
const LAMP_MEDIAN_M: f64 = 1.5;
const PALM_MEDIAN_M: f64 = 2.0;
/// Palms down the median, the first this far in; a bush halfway between where it is wide enough.
const PALM_SPACING_M: f64 = 12.0;
const PALM_START_M: f64 = 6.0;
const BUSH_MEDIAN_M: f64 = 3.0;
/// No median palm this close (along the road) to a lamp.
const LAMP_CLEAR_M: f64 = 3.0;
/// Palms along an avenue's outer side: this far apart, this far beyond the road's edge.
const OUTER_SPACING_M: f64 = 24.0;
const OUTER_SET_BACK_M: f64 = 5.0;
/// A town street's lamps stand this far beyond the road's edge (beyond the sidewalk on an open road).
const TOWN_LAMP_SET_BACK: (f64, f64) = (0.8, 2.8);
/// A median barrier stands at this offset where the median is too narrow to be centred in.
const JERSEY_NARROW_M: f64 = 1.0;
const JERSEY_SET_BACK_M: f64 = 0.5;

/// Where a prop stands `offset` metres to one side (`side`: +1 the rider's right) of the route,
/// `d` metres along it: its position and the yaw that turns a model's −x towards the road.
fn beside_route(route: &Route, d: f64, side: f64, offset: f64) -> ((f64, f64), f64) {
    let i = ((d / route.step).round() as usize).min(route.n - 1);
    let (tx, tz) = tangent(route, i);
    let p = route.sample_at(d);
    let heading = tx.atan2(tz);
    // Models have the road on their −x side: turned by `heading` that faces the rider's right,
    // so a prop on the right is turned round.
    let yaw = if side > 0.0 { heading + std::f64::consts::PI } else { heading };
    ((p.x - tz * side * offset, p.z + tx * side * offset), yaw)
}

/// The distances `start, start + spacing, …` along the route, short of its end (a whole
/// spacing short on a loop, so nothing doubles up where it closes).
fn every(route: &Route, start: f64, spacing: f64) -> impl Iterator<Item = f64> {
    let end = if route.closed { route.total - spacing } else { route.total };
    (0..).map(move |k| start + k as f64 * spacing).take_while(move |&d| d < end)
}

/// Whether a prop at `p` stands clear of the roads that cross or leave the route there: not
/// within a metre of any road strip that is not running alongside the route (the carriageways
/// alongside — the other side of a divided road — are what lamps and palms stand between).
fn clear_of_side_roads(world: &super::World, i: usize, p: (f64, f64)) -> bool {
    use super::scenery::to_segment;
    use super::CHUNK;
    let (tx, tz) = tangent(&world.route, i);
    let (cx, cz) = ((p.0 / CHUNK).floor() as i32, (p.1 / CHUNK).floor() as i32);
    world.scene.roads_in(cx, cz).all(|s| {
        let (dx, dz) = (s.b.0 - s.a.0, s.b.1 - s.a.1);
        let len = dx.hypot(dz);
        let alongside = len > 1e-6 && ((dx * tx + dz * tz) / len).abs() >= PARALLEL_COS;
        alongside || to_segment(p, s.a, s.b) > s.half + 1.0
    })
}

/// The sample index at `d` metres along.
fn sample(route: &Route, d: f64) -> usize {
    ((d / route.step).round() as usize).min(route.n - 1)
}

/// The tropical roadside props along the route, laid out like `World::barriers()`
/// (`[x, y, z, yaw, distance along, look, pitch]`): double lamps down an avenue's median,
/// single lamps along a town street, concrete barriers down an open divided road.
pub(super) fn props(world: &super::World, plan: &RoadsidePlan) -> Vec<[f32; 7]> {
    use super::barriers::{JERSEY, LAMP_DOUBLE, LAMP_SINGLE, SECTION_M};
    use super::OPEN;
    let route = &*world.route;
    let kinds = world.field.kinds();
    // Open ground, and a road rather than a trail.
    let open = |i: usize| kinds.get(i).is_none_or(|&k| k == OPEN) && world.scene.trails.get(i).is_none_or(Option::is_none);
    let mut out = Vec::new();
    let mut put = |d: f64, side: f64, offset: f64, look: usize, pitch: f64| {
        let ((x, z), yaw) = beside_route(route, d, side, offset);
        if !clear_of_side_roads(world, sample(route, d), (x, z)) {
            return;
        }
        let y = world.field.surface(x, z).0;
        out.push([x as f32, y as f32, z as f32, yaw as f32, d as f32, look as f32, pitch as f32]);
    };
    for d in every(route, LAMP_START_M, LAMP_SPACING_M) {
        let i = sample(route, d);
        if !open(i) {
            continue;
        }
        match plan.stretch[i] {
            Stretch::Avenue => {
                if let Some(m) = plan.median[i].filter(|&m| m >= LAMP_MEDIAN_M) {
                    put(d, plan.median_side[i], world.half_width(i) + m / 2.0, LAMP_DOUBLE, 0.0);
                }
            }
            Stretch::Town => {
                let set_back = if plan.style.open_road { TOWN_LAMP_SET_BACK.1 } else { TOWN_LAMP_SET_BACK.0 };
                put(d, 1.0, world.half_width(i) + set_back, LAMP_SINGLE, 0.0);
            }
            _ => {}
        }
    }
    for d in every(route, SECTION_M / 2.0, SECTION_M) {
        let i = sample(route, d);
        if !open(i) || plan.stretch[i] != Stretch::Periurban || plan.median_side[i] == 0.0 {
            continue;
        }
        let offset = match plan.median[i] {
            Some(m) if m >= JERSEY_NARROW_M => world.half_width(i) + m / 2.0,
            _ => world.half_width(i) + JERSEY_SET_BACK_M,
        };
        let climb = (route.sample_at(d + SECTION_M / 2.0).ele - route.sample_at(d - SECTION_M / 2.0).ele).atan2(SECTION_M);
        let side = plan.median_side[i];
        put(d, side, offset, JERSEY, if side > 0.0 { climb } else { -climb });
    }
    out
}

/// The palms and bushes of the tropical roadside standing in chunk `(cx, cz)`, as tree
/// entries (`[x, y, z, yaw, width scale, height scale, kind, shade]`): down an avenue's median
/// (bushes instead where it is too high for palms), and along its outer side.
pub(super) fn median_plants(world: &super::World, plan: &RoadsidePlan, cx: i32, cz: i32) -> Vec<(u8, [f32; super::TREE_FLOATS])> {
    use super::height::hash2;
    use super::trees::{BUSH, PALM};
    use super::{CHUNK, OPEN};
    let route = &*world.route;
    let kinds = world.field.kinds();
    let in_chunk = |(x, z): (f64, f64)| (x / CHUNK).floor() as i32 == cx && (z / CHUNK).floor() as i32 == cz;
    let lamp_near = |d: f64| {
        let k = ((d - LAMP_START_M) / LAMP_SPACING_M).round();
        (d - (LAMP_START_M + k * LAMP_SPACING_M)).abs() < LAMP_CLEAR_M
    };
    let mut out = Vec::new();
    let mut grow = |(x, z): (f64, f64), kind: u8, d: f64| {
        let seed = (d * 10.0) as i32;
        let r = |salt: i32| hash2(seed.wrapping_mul(31).wrapping_add(salt), cx.wrapping_mul(17) ^ cz.wrapping_sub(salt));
        let y = world.field.surface(x, z).0 - 0.2;
        let width = 0.85 + r(3) * 0.3;
        let tall = width * (0.92 + r(4) * 0.2);
        out.push((kind, [x as f32, y as f32, z as f32, (r(7) * std::f64::consts::TAU) as f32, width as f32, tall as f32, kind as f32, (0.85 + r(5) * 0.3) as f32]));
    };
    let palm_or_bush = |i: usize| if tropical_weight(route.lat0.abs(), route.ele[i] as f64) > 0.5 { PALM } else { BUSH };
    for d in every(route, PALM_START_M, PALM_SPACING_M / 2.0) {
        let i = sample(route, d);
        if plan.stretch[i] != Stretch::Avenue || kinds.get(i).is_some_and(|&k| k != OPEN) {
            continue;
        }
        let Some(m) = plan.median[i].filter(|&m| m >= PALM_MEDIAN_M) else { continue };
        // Every other spot is a palm; the ones between are bushes where the median is wide.
        let palm_spot = ((d - PALM_START_M) / (PALM_SPACING_M / 2.0)).round() as i64 % 2 == 0;
        let kind = if palm_spot { palm_or_bush(i) } else { BUSH };
        if (!palm_spot && m < BUSH_MEDIAN_M) || (palm_spot && lamp_near(d)) {
            continue;
        }
        let (p, _) = beside_route(route, d, plan.median_side[i], world.half_width(i) + m / 2.0);
        if in_chunk(p) && clear_of_side_roads(world, i, p) {
            grow(p, kind, d);
        }
    }
    for d in every(route, PALM_START_M, OUTER_SPACING_M) {
        let i = sample(route, d);
        if plan.stretch[i] != Stretch::Avenue || plan.median_side[i] == 0.0 || kinds.get(i).is_some_and(|&k| k != OPEN) {
            continue;
        }
        let (p, _) = beside_route(route, d, -plan.median_side[i], world.half_width(i) + OUTER_SET_BACK_M);
        let clear = !world.buildings.near(p.0, p.1, 2.0) && world.scene.roads_in(cx, cz).all(|s| super::scenery::to_segment(p, s.a, s.b) > s.half + 1.6);
        if in_chunk(p) && clear {
            grow(p, palm_or_bush(i), d);
        }
    }
    out
}

/// Walls along housing estates: looked for every `WALL_STEP_M` along the route, from
/// `WALL_FROM_M` to `WALL_TO_M` beyond the road's edge in 1 m steps.
const WALL_STEP_M: f64 = 2.5;
const WALL_FROM_M: f64 = 3.0;
const WALL_TO_M: f64 = 60.0;
/// Wall points further apart than this start a new run; runs shorter than this are dropped.
const WALL_JOIN_M: f64 = 4.0;
const WALL_MIN_M: f64 = 20.0;

/// The pieces of `run` whose segments have their middle in chunk `(cx, cz)`.
fn pieces_in(run: &[(f64, f64)], cx: i32, cz: i32) -> Vec<Vec<(f64, f64)>> {
    use super::CHUNK;
    let mut out: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut open = false;
    for w in run.windows(2) {
        let mid = ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0);
        let inside = (mid.0 / CHUNK).floor() as i32 == cx && (mid.1 / CHUNK).floor() as i32 == cz;
        match (inside, open) {
            (true, true) => out.last_mut().expect("an open piece").push(w[1]),
            (true, false) => out.push(vec![w[0], w[1]]),
            _ => {}
        }
        open = inside;
    }
    out
}

/// Walls along the edge of housing estates that face the road, standing in chunk `(cx, cz)`,
/// as runs of points: on an avenue, an open divided road or a town street, never on the median
/// side, at the first spot out from the road that is in a residential area and clear of roads,
/// buildings and mapped walls and hedges.
pub(super) fn walls(world: &super::World, plan: &RoadsidePlan, cx: i32, cz: i32) -> Vec<Vec<(f64, f64)>> {
    use super::scenery::to_segment;
    use super::{CHUNK, OPEN};
    let route = &*world.route;
    let (kinds, scene) = (world.field.kinds(), &world.scene);
    // Only route samples near the chunk can put a wall in it.
    let margin = WALL_TO_M + 15.0;
    let (x0, z0) = (cx as f64 * CHUNK - margin, cz as f64 * CHUNK - margin);
    let (x1, z1) = ((cx + 1) as f64 * CHUNK + margin, (cz + 1) as f64 * CHUNK + margin);
    let near = |i: usize| (x0..=x1).contains(&route.x[i]) && (z0..=z1).contains(&route.z[i]);
    let clear = |p: (f64, f64)| {
        let (qx, qz) = ((p.0 / CHUNK).floor() as i32, (p.1 / CHUNK).floor() as i32);
        !world.buildings.covers(p.0, p.1) && !scene.near_mapped_boundary(p.0, p.1) && scene.roads_in(qx, qz).all(|s| to_segment(p, s.a, s.b) > s.half + 1.0)
    };
    let length = |run: &[(f64, f64)]| run.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum::<f64>();
    let mut out = Vec::new();
    for side in [1.0f64, -1.0] {
        let mut run: Vec<(f64, f64)> = Vec::new();
        let mut finish = |run: &mut Vec<(f64, f64)>| {
            if length(run) >= WALL_MIN_M {
                out.extend(pieces_in(run, cx, cz));
            }
            run.clear();
        };
        for d in every(route, 0.0, WALL_STEP_M) {
            let i = sample(route, d);
            let here = near(i) && matches!(plan.stretch[i], Stretch::Avenue | Stretch::Periurban | Stretch::Town) && kinds.get(i).is_none_or(|&k| k == OPEN) && plan.median_side[i] != side;
            // The estate's edge: the first step out from the road that goes from outside the
            // housing to inside it. Where the road itself is in the housing (a village mapped
            // whole), there is no edge facing it.
            let spot = if here {
                let probes: Vec<(f64, f64)> = (0..).map(|k| WALL_FROM_M + k as f64).take_while(|&o| o <= WALL_TO_M).map(|o| beside_route(route, d, side, world.half_width(i) + o).0).collect();
                let inside: Vec<bool> = probes.iter().map(|p| scene.residential.at(p.0, p.1)).collect();
                if inside.first().copied().unwrap_or(true) {
                    None
                } else {
                    (1..probes.len()).find(|&k| inside[k] && !inside[k - 1]).map(|k| probes[k]).filter(|&p| clear(p))
                }
            } else {
                None
            };
            match spot {
                Some(p) if run.last().is_none_or(|q| (q.0 - p.0).hypot(q.1 - p.1) <= WALL_JOIN_M) => run.push(p),
                Some(p) => {
                    finish(&mut run);
                    run.push(p);
                }
                None => finish(&mut run),
            }
        }
        finish(&mut run);
    }
    out
}
