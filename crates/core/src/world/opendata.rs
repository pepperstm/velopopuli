//! A route's open-data pack: the real things along it that the map tiles and the app's own
//! Overpass request don't bring, gathered once by the preparation script (`tools/opendata`)
//! from open sources — power lines, street lamps, cycle lanes, median barriers and water towers
//! from OpenStreetMap, the houses OpenStreetMap lacks (Google Open Buildings and Microsoft
//! through Overture Maps), the trees measured from space (Meta/WRI canopy height), lamps and
//! poles seen by Mapillary, and water and wetland (ESA WorldCover).
//!
//! The pack is stored as `opendata-<route id>` in the app's data folder. Buildings and water
//! join the map before the world is built (`footprints`, `areas`); the rest is kept here in the
//! route's local metres (`OpenScene`) for the roadside rules and the chunks.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::buildings::Footprint;
use super::scenery::{Area, AreaKind};
use super::CHUNK;
use crate::gpx::Route;

/// The pack format this app reads.
pub const OPEN_DATA_FORMAT: u32 = 1;
/// Building ids from the pack start here, clear of OpenStreetMap's.
const BUILDING_ID_BASE: u64 = 1 << 62;
/// How far either side of the route the pack's trees reach, unless the pack says.
const DEFAULT_TREES_CORRIDOR_M: f64 = 500.0;

/// A power line as mapped: its points, and what holds the wires up at each.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct PowerLine {
    /// `"line"` (high voltage, on towers) or `"minor_line"` (on poles).
    pub voltage_kind: String,
    /// `[lat, lon]` points.
    pub points: Vec<[f64; 2]>,
    /// Per point: `"tower"`, `"pole"` or `""` (a bend with nothing mapped).
    pub supports: Vec<String>,
}

/// A building outline from the pack, with its height in metres.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct OpenBuilding {
    pub ring: Vec<[f64; 2]>,
    pub height: f32,
}

/// A route's open-data pack, as the preparation script writes it. Every list is optional.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct OpenData {
    pub format: u32,
    /// The sources that answered, with their credits.
    pub sources: Vec<String>,
    pub power_lines: Vec<PowerLine>,
    /// Street lamps, `[lat, lon]`.
    pub lamps: Vec<[f64; 2]>,
    /// Utility poles not on a mapped power line, `[lat, lon]`.
    pub poles: Vec<[f64; 2]>,
    /// Cycle lanes and tracks along roads, as lines.
    pub cycleways: Vec<Vec<[f64; 2]>>,
    /// Concrete median barriers, as lines.
    pub jersey: Vec<Vec<[f64; 2]>>,
    pub water_towers: Vec<[f64; 2]>,
    pub buildings: Vec<OpenBuilding>,
    /// Trees as `[lat, lon, height in metres]`.
    pub trees: Vec<[f64; 3]>,
    /// How far either side of the route the trees were gathered.
    pub trees_corridor_m: f64,
    /// Water and wetland, each an area of `[lat, lon]` rings.
    pub water: Vec<Vec<Vec<[f64; 2]>>>,
}

impl OpenData {
    /// Whether this app can read the pack.
    pub fn usable(&self) -> bool {
        self.format == OPEN_DATA_FORMAT
    }

    /// The pack's buildings as footprints, numbered clear of OpenStreetMap's.
    pub fn footprints(&self) -> Vec<Footprint> {
        self.buildings.iter().enumerate().filter(|(_, b)| b.ring.len() >= 4).map(|(k, b)| Footprint { id: BUILDING_ID_BASE + k as u64, height: b.height, ring: b.ring.clone() }).collect()
    }

    /// The pack's water and wetland as map areas.
    pub fn areas(&self) -> Vec<Area> {
        self.water.iter().filter(|rings| !rings.is_empty()).map(|rings| Area { kind: AreaKind::Water, rings: rings.clone() }).collect()
    }
}

/// A power line's support in local metres: a lattice tower or a pole, turned to the line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Support {
    pub at: (f64, f64),
    pub tower: bool,
    /// The line's direction here (radians, as a model's yaw: its z axis along the line).
    pub heading: f64,
    /// Close enough to the route to be drawn.
    pub near: bool,
}

/// The wires between two neighbouring supports of one line, as indices into `supports`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub a: usize,
    pub b: usize,
    pub tower: bool,
}

/// Supports are drawn within this of the route.
pub const SUPPORT_REACH_M: f64 = 300.0;
/// A support mapped on the road is moved this far beyond its edge.
const OFF_ROAD_M: f64 = 2.0;
/// Where the wires hang on a tower and on a pole: `(across the line, height)`.
const TOWER_WIRES: [(f64, f64); 3] = [(-6.0, 26.0), (6.0, 26.0), (0.0, 29.5)];
const POLE_WIRES: [(f64, f64); 3] = [(-1.0, 10.4), (1.0, 10.4), (0.0, 10.9)];
/// How far the wires sag at mid-span, as a share of the span, and how many pieces draw one.
const SAG: f64 = 0.03;
const WIRE_PIECES: usize = 8;
const WIRE_THICKNESS: f64 = 0.06;
/// A cycle lane counts as running along the route within this of the road's edge, this nearly
/// parallel (as a cosine), for at least this far.
const CYCLE_REACH_M: f64 = 12.0;
const CYCLE_PARALLEL_COS: f64 = 0.94;
const CYCLE_MIN_M: f64 = 40.0;

/// The pack in the route's local metres.
#[derive(Debug, Clone, Default)]
pub struct OpenScene {
    pub lamps: Vec<(f64, f64)>,
    pub poles: Vec<(f64, f64)>,
    pub supports: Vec<Support>,
    pub spans: Vec<Span>,
    pub cycleways: Vec<Vec<(f64, f64)>>,
    pub jersey: Vec<Vec<(f64, f64)>>,
    pub water_towers: Vec<(f64, f64)>,
    /// Trees as `(x, z, height)`, by the chunk they stand in.
    pub trees: HashMap<(i32, i32), Vec<(f64, f64, f64)>>,
    pub trees_corridor_m: f64,
    /// Per route sample, whether a cycle lane runs along its right and its left: `[right, left]`.
    pub cycle_sides: Vec<[bool; 2]>,
}

impl OpenScene {
    pub fn new(route: &Route, data: &OpenData) -> Self {
        let local = |p: &[f64; 2]| route.to_local(p[0], p[1]);
        let line = |l: &Vec<[f64; 2]>| l.iter().map(local).collect::<Vec<_>>();
        let mut scene = OpenScene {
            lamps: data.lamps.iter().map(local).collect(),
            poles: data.poles.iter().map(local).collect(),
            cycleways: data.cycleways.iter().map(line).filter(|l| l.len() >= 2).collect(),
            jersey: data.jersey.iter().map(line).filter(|l| l.len() >= 2).collect(),
            water_towers: data.water_towers.iter().map(local).collect(),
            trees_corridor_m: if data.trees_corridor_m > 0.0 { data.trees_corridor_m } else { DEFAULT_TREES_CORRIDOR_M },
            ..Default::default()
        };
        for power in &data.power_lines {
            let tower = power.voltage_kind != "minor_line";
            let held: Vec<(f64, f64)> =
                power.points.iter().zip(power.supports.iter().chain(std::iter::repeat(&String::new()))).filter(|(_, s)| s.as_str() == "tower" || s.as_str() == "pole").map(|(p, _)| local(p)).collect();
            let first = scene.supports.len();
            for (k, &at) in held.iter().enumerate() {
                let (prev, next) = (held[k.saturating_sub(1)], held[(k + 1).min(held.len() - 1)]);
                let heading = (next.0 - prev.0).atan2(next.1 - prev.1);
                scene.supports.push(Support { at, tower, heading, near: true });
            }
            for k in 1..held.len() {
                scene.spans.push(Span { a: first + k - 1, b: first + k, tower });
            }
        }
        for t in &data.trees {
            let (x, z) = route.to_local(t[0], t[1]);
            scene.trees.entry(((x / CHUNK).floor() as i32, (z / CHUNK).floor() as i32)).or_default().push((x, z, t[2]));
        }
        scene
    }
}

impl OpenScene {
    /// Marks the supports near enough to the route to draw, and moves any mapped on the road
    /// itself (or within a metre of it) off to its side, `half_width(i)` being the road's half
    /// width at route sample `i`.
    pub(super) fn keep_off_road(&mut self, route: &Route, half_width: impl Fn(usize) -> f64) {
        for s in &mut self.supports {
            let Some((i, dist)) = (0..route.n).map(|i| (i, (route.x[i] - s.at.0).hypot(route.z[i] - s.at.1))).min_by(|a, b| a.1.total_cmp(&b.1)) else { continue };
            s.near = dist <= SUPPORT_REACH_M;
            let half = half_width(i);
            if dist < half + 1.0 {
                let (a, b) = (i.saturating_sub(1), (i + 1).min(route.n - 1));
                let (tx, tz) = (route.x[b] - route.x[a], route.z[b] - route.z[a]);
                let len = tx.hypot(tz).max(1e-9);
                let (rx, rz) = (-tz / len, tx / len);
                let right = (s.at.0 - route.x[i]) * rx + (s.at.1 - route.z[i]) * rz;
                let side = if right >= 0.0 { 1.0 } else { -1.0 };
                s.at = (route.x[i] + rx * side * (half + OFF_ROAD_M), route.z[i] + rz * side * (half + OFF_ROAD_M));
            }
        }
    }

    /// Works out which side of the route the pack's cycle lanes run along, sample by sample: a
    /// lane within `CYCLE_REACH_M` of the road's edge, nearly parallel to it, for at least
    /// `CYCLE_MIN_M`.
    pub(super) fn find_cycle_sides(&mut self, route: &Route, half_width: impl Fn(usize) -> f64) {
        let mut sides = vec![[false; 2]; route.n];
        let tangent = |i: usize| {
            let (a, b) = (i.saturating_sub(1), (i + 1).min(route.n - 1));
            let (tx, tz) = (route.x[b] - route.x[a], route.z[b] - route.z[a]);
            let len = tx.hypot(tz).max(1e-9);
            (tx / len, tz / len)
        };
        for line in &self.cycleways {
            for w in line.windows(2) {
                let (dx, dz) = (w[1].0 - w[0].0, w[1].1 - w[0].1);
                let len = dx.hypot(dz);
                if len < 1e-6 {
                    continue;
                }
                let pieces = (len / route.step).ceil().max(1.0) as usize;
                for k in 0..pieces {
                    let t = (k as f64 + 0.5) / pieces as f64;
                    let p = (w[0].0 + dx * t, w[0].1 + dz * t);
                    let Some((i, _)) = (0..route.n).map(|i| (i, (route.x[i] - p.0).hypot(route.z[i] - p.1))).min_by(|a, b| a.1.total_cmp(&b.1)) else { continue };
                    let (tx, tz) = tangent(i);
                    if ((dx * tx + dz * tz) / len).abs() < CYCLE_PARALLEL_COS {
                        continue;
                    }
                    let right = (p.0 - route.x[i]) * -tz + (p.1 - route.z[i]) * tx;
                    let gap = right.abs() - half_width(i);
                    if (-1.0..=CYCLE_REACH_M).contains(&gap) {
                        sides[i][usize::from(right < 0.0)] = true;
                    }
                }
            }
        }
        // Close small gaps, then keep only lanes alongside for a good stretch.
        let (fill, min_run) = (2, (CYCLE_MIN_M / route.step).ceil() as usize);
        for k in 0..2 {
            let mut side: Vec<bool> = sides.iter().map(|s| s[k]).collect();
            for (from, to, on) in super::surface::runs(&side.clone()) {
                if !on && to - from <= fill && from > 0 && to < route.n {
                    side[from..to].fill(true);
                }
            }
            for (from, to, on) in super::surface::runs(&side.clone()) {
                if on && to - from < min_run {
                    side[from..to].fill(false);
                }
            }
            for (s, on) in sides.iter_mut().zip(side) {
                s[k] = on;
            }
        }
        self.cycle_sides = sides;
    }

    /// The wires of the power lines whose pieces have their middle in chunk `(cx, cz)`, sagging
    /// between their supports, added to `mesh`. `ground(x, z)` is the height of the ground.
    pub(super) fn wires(&self, cx: i32, cz: i32, ground: impl Fn(f64, f64) -> f64, mesh: &mut super::mesh::TriMesh) {
        let colour = [0.05f32, 0.05, 0.05];
        for span in &self.spans {
            let (a, b) = (self.supports[span.a], self.supports[span.b]);
            if !(a.near || b.near) {
                continue;
            }
            let length = (b.at.0 - a.at.0).hypot(b.at.1 - a.at.1);
            let (ga, gb) = (ground(a.at.0, a.at.1), ground(b.at.0, b.at.1));
            let hung = |s: &Support, g: f64, (across, up): (f64, f64)| [s.at.0 + s.heading.cos() * across, g + up, s.at.1 - s.heading.sin() * across];
            for &wire in if span.tower { &TOWER_WIRES } else { &POLE_WIRES } {
                let (p, q) = (hung(&a, ga, wire), hung(&b, gb, wire));
                let at = |t: f64| [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t - 4.0 * SAG * length * t * (1.0 - t), p[2] + (q[2] - p[2]) * t];
                for k in 0..WIRE_PIECES {
                    let (u, v) = (at(k as f64 / WIRE_PIECES as f64), at((k + 1) as f64 / WIRE_PIECES as f64));
                    let mid = ((u[0] + v[0]) / 2.0, (u[2] + v[2]) / 2.0);
                    if (mid.0 / CHUNK).floor() as i32 == cx && (mid.1 / CHUNK).floor() as i32 == cz {
                        tube(mesh, u, v, colour);
                    }
                }
            }
        }
    }
}

/// A thin square tube from `a` to `b`, for a wire.
fn tube(mesh: &mut super::mesh::TriMesh, a: [f64; 3], b: [f64; 3], colour: [f32; 3]) {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-9);
    let flat = (-d[2]).hypot(d[0]).max(1e-9);
    let side = [-d[2] / flat, 0.0, d[0] / flat];
    let up = [(side[1] * d[2] - side[2] * d[1]) / len, (side[2] * d[0] - side[0] * d[2]) / len, (side[0] * d[1] - side[1] * d[0]) / len];
    let h = WIRE_THICKNESS / 2.0;
    let corner = |p: [f64; 3], s: f64, u: f64| [p[0] + (side[0] * s + up[0] * u) * h, p[1] + (side[1] * s + up[1] * u) * h, p[2] + (side[2] * s + up[2] * u) * h];
    let faces = [((1.0, 1.0), (1.0, -1.0), side), ((-1.0, -1.0), (-1.0, 1.0), side.map(|c| -c)), ((-1.0, 1.0), (1.0, 1.0), up), ((1.0, -1.0), (-1.0, -1.0), up.map(|c| -c))];
    for ((s0, u0), (s1, u1), facing) in faces {
        mesh.quad([corner(a, s0, u0), corner(a, s1, u1), corner(b, s1, u1), corner(b, s0, u0)], facing, [colour; 4]);
    }
}
