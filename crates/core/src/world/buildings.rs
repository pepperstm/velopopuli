//! Buildings from OpenStreetMap: parsing footprints out of an Overpass API
//! reply and extruding them into simple blocks standing on the terrain.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::height::{hash2, HeightField};
pub use super::mesh::TriMesh as BuildingMesh;
use super::style::{look, parse_colour, region_at, BuildingTags, Look, Region, RoofShape};
use super::CHUNK;
use crate::gpx::Route;

/// A building outline as mapped: a closed ring of `[lat, lon]`, the height of its walls in
/// metres, and what else the map says of it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Footprint {
    pub id: u64,
    /// Height of the walls to the eaves (the roof comes on top).
    pub height: f32,
    pub ring: Vec<[f64; 2]>,
    #[serde(default)]
    pub tags: BuildingTags,
    /// The mapped height to the top of the roof, where there is one: the walls are then
    /// whatever the roof leaves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_height: Option<f32>,
}

#[derive(Deserialize)]
struct Reply {
    elements: Vec<Element>,
}

#[derive(Deserialize)]
struct Element {
    id: u64,
    #[serde(default)]
    tags: HashMap<String, String>,
    /// Present on ways. Entries can be null where a node fell outside the query.
    #[serde(default)]
    geometry: Vec<Option<LatLon>>,
    /// Present on multipolygon relations.
    #[serde(default)]
    members: Vec<Member>,
}

#[derive(Deserialize)]
struct Member {
    #[serde(default)]
    role: String,
    #[serde(default)]
    geometry: Vec<Option<LatLon>>,
}

#[derive(Deserialize, Clone, Copy)]
struct LatLon {
    lat: f64,
    lon: f64,
}

/// Reads the number at the start of a tag value such as `"12"`, `"7.5 m"` or `"3;4"`.
fn leading_number(s: &str) -> Option<f32> {
    let s = s.trim();
    let end = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    s[..end].parse().ok()
}

/// Height of the walls in metres from the mapped tags, falling back to a typical one for the building type.
fn height_from_tags(tags: &HashMap<String, String>) -> f32 {
    if let Some(levels) = tags.get("building:levels").and_then(|v| leading_number(v)) {
        return (levels * STOREY as f32 + 0.5).clamp(2.5, 150.0);
    }
    match tags.get("building").map(String::as_str).unwrap_or("yes") {
        "garage" | "garages" | "shed" | "hut" | "cabin" | "carport" | "kiosk" | "greenhouse" | "static_caravan" => 2.6,
        "house" | "detached" | "semidetached_house" | "bungalow" | "farm" | "terrace" | "chalet" => 5.8,
        "barn" | "farm_auxiliary" | "stable" | "cowshed" => 5.0,
        "residential" | "retail" | "supermarket" | "industrial" | "warehouse" | "school" => 7.5,
        "apartments" | "hotel" | "office" | "commercial" | "hospital" | "university" | "public" | "civic" => 12.5,
        "church" | "cathedral" | "chapel" | "mosque" | "temple" => 11.0,
        _ => 6.5,
    }
}

/// The tags that shape how a building looks.
fn building_tags(tags: &HashMap<String, String>) -> BuildingTags {
    let get = |k: &str| tags.get(k).map(|v| v.trim().to_ascii_lowercase()).unwrap_or_default();
    BuildingTags {
        kind: get("building"),
        roof_shape: get("roof:shape"),
        wall_colour: tags.get("building:colour").and_then(|v| parse_colour(v)),
        roof_colour: tags.get("roof:colour").and_then(|v| parse_colour(v)),
        wall_material: get("building:material"),
        roof_material: get("roof:material"),
    }
}

fn closed_ring(geometry: &[Option<LatLon>]) -> Option<Vec<[f64; 2]>> {
    let pts: Vec<LatLon> = geometry.iter().flatten().copied().collect();
    let (first, last) = (pts.first()?, pts.last()?);
    // An outline must come back to where it started; anything else is a fragment.
    (pts.len() >= 4 && pts.len() == geometry.len() && first.lat == last.lat && first.lon == last.lon).then(|| pts.iter().map(|p| [p.lat, p.lon]).collect())
}

/// Extracts building footprints from an Overpass API reply produced with `out geom`.
pub fn parse_overpass(json: &str) -> Result<Vec<Footprint>, String> {
    let reply: Reply = serde_json::from_str(json).map_err(|e| format!("Unexpected reply from the OpenStreetMap server: {e}"))?;
    let mut out = Vec::new();
    for e in reply.elements {
        let underground = e.tags.get("location").is_some_and(|v| v == "underground") || e.tags.get("layer").is_some_and(|v| v.trim_start().starts_with('-'));
        // "roof" is a canopy with no walls (petrol station forecourts, shelters).
        if underground || e.tags.get("building").is_some_and(|v| v == "roof" || v == "no") {
            continue;
        }
        let height = height_from_tags(&e.tags);
        let tags = building_tags(&e.tags);
        let total_height = e.tags.get("height").and_then(|v| leading_number(v)).map(|h| h.clamp(2.5, 150.0));
        if let Some(ring) = closed_ring(&e.geometry) {
            out.push(Footprint { id: e.id, height, ring, tags: tags.clone(), total_height });
        }
        // Multipolygons: each closed outer ring becomes a block; courtyards are ignored.
        for (k, m) in e.members.iter().enumerate().filter(|(_, m)| m.role == "outer") {
            if let Some(ring) = closed_ring(&m.geometry) {
                out.push(Footprint { id: e.id.wrapping_mul(31).wrapping_add(k as u64), height, ring, tags: tags.clone(), total_height });
            }
        }
    }
    Ok(out)
}

/// The Overpass QL query for every building within `radius_m` of a stretch of the route.
pub fn overpass_query(route: &Route, from: usize, to: usize, radius_m: f64, spacing_m: f64) -> String {
    let stride = ((spacing_m / route.step).round() as usize).max(1);
    let mut line: Vec<String> = (from..to).step_by(stride).map(|i| format!("{:.5},{:.5}", route.lat[i], route.lon[i])).collect();
    line.push(format!("{:.5},{:.5}", route.lat[to - 1], route.lon[to - 1]));
    let line = line.join(",");
    format!("[out:json][timeout:25];(way[\"building\"](around:{radius_m:.0},{line});relation[\"building\"](around:{radius_m:.0},{line}););out geom;")
}

struct Building {
    /// Outline in local metres (x, z), without a repeated closing point, wound so the area is positive.
    ring: Vec<(f64, f64)>,
    /// Terrain height under each outline vertex.
    ground: Vec<f64>,
    height: f64,
    /// The mapped height to the roof top, if any.
    total: Option<f64>,
    centre: (f64, f64),
    radius: f64,
    seed: i32,
    look: Look,
    /// The rectangle a pitched roof is built over; `None` for a flat roof.
    frame: Option<Frame>,
}

/// A rectangle fitted round an outline: centre, unit vector along its longer side, and half
/// its length and width.
#[derive(Debug, Clone, Copy)]
struct Frame {
    centre: (f64, f64),
    u: (f64, f64),
    a: f64,
    w: f64,
}

impl Frame {
    /// The smallest rectangle round the outline with a side along one of its walls.
    fn fit(ring: &[(f64, f64)]) -> Frame {
        let n = ring.len();
        let mut best: Option<(f64, Frame)> = None;
        for i in 0..n {
            let (p, q) = (ring[i], ring[(i + 1) % n]);
            let len = (q.0 - p.0).hypot(q.1 - p.1);
            if len < 0.5 {
                continue;
            }
            let u = ((q.0 - p.0) / len, (q.1 - p.1) / len);
            let v = (-u.1, u.0);
            let (mut s0, mut s1, mut r0, mut r1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
            for &(x, z) in ring {
                let (s, r) = (x * u.0 + z * u.1, x * v.0 + z * v.1);
                (s0, s1, r0, r1) = (s0.min(s), s1.max(s), r0.min(r), r1.max(r));
            }
            let area = (s1 - s0) * (r1 - r0);
            if best.as_ref().is_some_and(|b| b.0 <= area) {
                continue;
            }
            let (sm, rm) = ((s0 + s1) / 2.0, (r0 + r1) / 2.0);
            let centre = (sm * u.0 + rm * v.0, sm * u.1 + rm * v.1);
            let (a, w) = ((s1 - s0) / 2.0, (r1 - r0) / 2.0);
            // The long side along u.
            let frame = if a >= w { Frame { centre, u, a, w } } else { Frame { centre, u: v, a: w, w: a } };
            best = Some((area, frame));
        }
        best.map(|b| b.1).unwrap_or(Frame { centre: ring[0], u: (1.0, 0.0), a: 0.5, w: 0.5 })
    }

    /// A point `s` along and `r` across the rectangle.
    fn at(&self, s: f64, r: f64, y: f64) -> [f64; 3] {
        let v = (-self.u.1, self.u.0);
        [self.centre.0 + s * self.u.0 + r * v.0, y, self.centre.1 + s * self.u.1 + r * v.1]
    }
}

/// Pitched roofs only go on outlines this close to their rectangle (L-shapes and the like
/// get one over the whole), and no wider than this across.
const ROOF_FILL: f64 = 0.85;
const ROOF_SPAN: f64 = 26.0;
/// The tallest a roof gets, metres.
const ROOF_RISE: f64 = 9.0;

fn signed_area(ring: &[(f64, f64)]) -> f64 {
    let n = ring.len();
    (0..n).map(|i| ring[i].0 * ring[(i + 1) % n].1 - ring[(i + 1) % n].0 * ring[i].1).sum::<f64>() / 2.0
}

fn cross(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0)
}

fn in_triangle(p: (f64, f64), a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
    cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
}

/// Ear-clipping triangulation of a simple polygon with positive signed area.
pub(super) fn triangulate(ring: &[(f64, f64)]) -> Vec<[usize; 3]> {
    let mut idx: Vec<usize> = (0..ring.len()).collect();
    let mut out = Vec::with_capacity(ring.len().saturating_sub(2));
    while idx.len() > 3 {
        let m = idx.len();
        let ear = (0..m).find(|&k| {
            let (a, b, c) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            // A convex corner with no other vertex inside its triangle can be cut off.
            cross(ring[a], ring[b], ring[c]) > 1e-9 && !idx.iter().any(|&j| j != a && j != b && j != c && in_triangle(ring[j], ring[a], ring[b], ring[c]))
        });
        // Self-intersecting or degenerate outlines can run out of ears; close what is left as a fan.
        let Some(k) = ear else { break };
        out.push([idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]]);
        idx.remove(k);
    }
    for k in 1..idx.len().saturating_sub(1) {
        out.push([idx[0], idx[k], idx[k + 1]]);
    }
    out
}

fn inside(ring: &[(f64, f64)], x: f64, z: f64) -> bool {
    let n = ring.len();
    let mut hit = false;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a.1 > z) != (b.1 > z) && x < (b.0 - a.0) * (z - a.1) / (b.1 - a.1) + a.0 {
            hit = !hit;
        }
    }
    hit
}

/// Storey height, and the rough width of one window bay, in metres.
const STOREY: f64 = 3.0;
const BAY: f64 = 3.2;
/// A spot on the window tile that is plain wall, for roofs and gables.
const PLAIN: [f32; 2] = [0.04, 0.04];
/// Nothing is built this close to the road centreline: a mapped outline there means
/// the GPS track and the map disagree, and a wall across the road helps no one.
const ROAD_CLEARANCE: f64 = 5.5;

#[derive(Default)]
pub struct Buildings {
    by_chunk: HashMap<(i32, i32), Vec<Building>>,
    count: usize,
}

fn chunk_of(x: f64, z: f64) -> (i32, i32) {
    ((x / CHUNK).floor() as i32, (z / CHUNK).floor() as i32)
}

impl Buildings {
    /// Places footprints in the route's local frame, dropping any that are
    /// implausible or that would stand on the road.
    /// `widths` is the route's road width per sample (empty: the standard width throughout).
    pub fn new(route: &Route, field: &HeightField, widths: &[f64], footprints: &[Footprint]) -> Self {
        // Clear of the road and the barriers beside it, however wide the road is there.
        let clear = |x: f64, z: f64| match field.nearest_sample(x, z) {
            Some((d, i)) => d >= ROAD_CLEARANCE.max(widths.get(i).map_or(0.0, |w| w / 2.0 + 1.8)),
            None => true,
        };
        let mut out = Buildings::default();
        // The region is looked up once per few kilometres: borders only matter that finely.
        let mut regions: HashMap<(i32, i32), Region> = HashMap::new();
        for f in footprints {
            let mut ring: Vec<(f64, f64)> = f.ring.iter().map(|p| route.to_local(p[0], p[1])).collect();
            if ring.len() > 1 && ring.first() == ring.last() {
                ring.pop();
            }
            ring.dedup();
            // Mappers often add extra points along a straight wall; without them a plain
            // rectangle is recognisable as one (and gets a pitched roof).
            let mut k = 0;
            while ring.len() > 3 && k < ring.len() {
                let m = ring.len();
                let (a, p, c) = (ring[(k + m - 1) % m], ring[k], ring[(k + 1) % m]);
                let off_line = ((c.0 - a.0) * (p.1 - a.1) - (c.1 - a.1) * (p.0 - a.0)).abs() / (c.0 - a.0).hypot(c.1 - a.1).max(1e-9);
                if off_line < 0.25 {
                    ring.remove(k);
                } else {
                    k += 1;
                }
            }
            if ring.len() < 3 {
                continue;
            }
            let area = signed_area(&ring);
            if !(8.0..=60_000.0).contains(&area.abs()) {
                continue;
            }
            if area < 0.0 {
                ring.reverse();
            }
            let n = ring.len();
            let centre = (ring.iter().map(|p| p.0).sum::<f64>() / n as f64, ring.iter().map(|p| p.1).sum::<f64>() / n as f64);
            let ground: Vec<(f64, f64)> = ring.iter().map(|p| field.surface(p.0, p.1)).collect();
            // Corners, wall midpoints and the middle must all be clear of the road.
            let mid_clear = (0..n).all(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                clear((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
            });
            if ground.iter().any(|g| g.1 < ROAD_CLEARANCE) || !ring.iter().all(|p| clear(p.0, p.1)) || !mid_clear || !clear(centre.0, centre.1) {
                continue;
            }
            let radius = ring.iter().map(|p| (p.0 - centre.0).hypot(p.1 - centre.1)).fold(0.0, f64::max);
            let seed = (f.id ^ (f.id >> 32)) as i32;
            let [lat, lon] = f.ring[0];
            let region = *regions.entry(((lat * 20.0).floor() as i32, (lon * 20.0).floor() as i32)).or_insert_with(|| region_at(lat, lon));
            let mut look = look(region, &f.tags, area.abs(), seed);
            let frame = Frame::fit(&ring);
            let pitched = look.shape != RoofShape::Flat && area.abs() / (4.0 * frame.a * frame.w) >= ROOF_FILL && frame.w * 2.0 <= ROOF_SPAN;
            if !pitched {
                look.shape = RoofShape::Flat;
            }
            let frame = pitched.then_some(frame);
            let building = Building { ground: ground.iter().map(|g| g.0).collect(), ring, height: f.height as f64, total: f.total_height.map(f64::from), centre, radius, seed, look, frame };
            out.by_chunk.entry(chunk_of(centre.0, centre.1)).or_default().push(building);
            out.count += 1;
        }
        out
    }

    pub fn count(&self) -> usize {
        self.count
    }

    /// True if the point is inside a building's outline (used to keep trees out of houses).
    pub fn covers(&self, x: f64, z: f64) -> bool {
        let (cx, cz) = chunk_of(x, z);
        // A building belongs to the chunk holding its centre but may reach into a neighbour.
        (cx - 1..=cx + 1)
            .flat_map(|ix| (cz - 1..=cz + 1).map(move |iz| (ix, iz)))
            .filter_map(|key| self.by_chunk.get(&key))
            .flatten()
            .any(|b| (x - b.centre.0).hypot(z - b.centre.1) <= b.radius + 1.5 && inside(&b.ring, x, z))
    }

    /// Extrudes the buildings whose centre lies in chunk `(cx, cz)`.
    pub fn mesh(&self, cx: i32, cz: i32) -> BuildingMesh {
        let mut mesh = BuildingMesh::default();
        let Some(list) = self.by_chunk.get(&(cx, cz)) else { return mesh };
        for b in list {
            build(&mut mesh, b);
        }
        mesh
    }
}

/// One building: its walls, then its roof.
fn build(mesh: &mut BuildingMesh, b: &Building) {
    let n = b.ring.len();
    let l = &b.look;
    let lowest = b.ground.iter().cloned().fold(f64::INFINITY, f64::min);
    let highest = b.ground.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mean = b.ground.iter().sum::<f64>() / n as f64;
    let tan = l.pitch.to_radians().tan();
    // How high the roof rises above the eaves.
    let rise = match (l.shape, b.frame) {
        (RoofShape::Flat, _) | (_, None) => 0.0,
        (RoofShape::Skillion, Some(f)) => (2.0 * f.w * tan).min(ROOF_RISE),
        (_, Some(f)) => (f.w * tan).min(ROOF_RISE),
    };
    // A mapped overall height leaves the walls whatever the roof doesn't take.
    let walls = b.total.map_or(b.height, |t| (t - rise).max(2.5));
    // On a slope the walls reach down past the lowest corner. The eaves sit the walls' height
    // above the average ground, so the uphill side is dug in (as real hillside buildings are)
    // but always shows at least a low wall.
    let (base, top) = (lowest - 1.5, (mean + walls).max(highest + 2.5));
    let linear = |c: [f64; 3], k: f64| c.map(|v| ((v * k).clamp(0.0, 1.0)).powf(2.2) as f32);
    // A slight per-building brightness shift keeps a terrace from reading as one slab.
    let shade = 0.92 + hash2(b.seed, 7) * 0.16;
    // Darker at the foot, lighter at the eaves: cheap depth without textures.
    let (wall_top, wall_base) = (linear(l.wall, shade), linear(l.wall, shade * 0.8));
    // A chalet's timber starts above its ground floor.
    let split = l.upper.filter(|_| top - mean > STOREY + 1.5).map(|c| ((mean + STOREY + 0.3).max(base + 1.0), linear(c, shade)));
    let upper = split.map_or(wall_top, |s| s.1);
    let roof = linear(l.roof, shade);
    let under = linear(l.roof, shade * 0.45);
    // Window rows count down from the eaves; walls without windows use a plain spot on the tile.
    let uv = |bays: f32, y: f64| if l.windows { [bays, ((top - y) / STOREY) as f32] } else { PLAIN };

    let at = |p: (f64, f64), y: f64| [p.0, y, p.1];
    for i in 0..n {
        let (a, c) = (b.ring[i], b.ring[(i + 1) % n]);
        // The ring has positive area in (x, z), which puts "outside" on this side of each edge.
        let out = [c.1 - a.1, 0.0, a.0 - c.0];
        // A whole number of window bays per wall, so no window is cut by a corner.
        let bays = ((c.0 - a.0).hypot(c.1 - a.1) / BAY).round().max(1.0) as f32;
        let bands = match split {
            Some((y, timber)) => vec![(base, y, wall_base, wall_top), (y, top, timber, timber)],
            None => vec![(base, top, wall_base, wall_top)],
        };
        for (y0, y1, c0, c1) in bands {
            mesh.triangle([at(a, y0), at(c, y0), at(c, y1)], out, [c0, c0, c1], [uv(0.0, y0), uv(bays, y0), uv(bays, y1)]);
            mesh.triangle([at(a, y0), at(c, y1), at(a, y1)], out, [c0, c1, c1], [uv(0.0, y0), uv(bays, y1), uv(0.0, y1)]);
        }
    }

    let up = [0.0, 1.0, 0.0];
    let Some(f) = b.frame.filter(|_| rise > 0.0) else {
        for tri in triangulate(&b.ring) {
            mesh.triangle(tri.map(|k| at(b.ring[k], top)), up, [roof; 3], [PLAIN; 3]);
        }
        return;
    };
    // The roof reaches past the walls by the eaves' overhang, falling at its own pitch.
    let o = l.eave;
    let (e, w) = (f.a + o, f.w + o);
    let slope = if l.shape == RoofShape::Skillion { rise / (2.0 * f.w) } else { rise / f.w };
    let ye = top - o * slope;
    let ridge = top + rise;
    let mut faces: Vec<Vec<[f64; 3]>> = Vec::new();
    // Gable ends and the like, in the walls' (upper) colour: (points, outward).
    let mut ends: Vec<([[f64; 3]; 3], [f64; 3])> = Vec::new();
    let along = |sign: f64| {
        let p = f.at(sign, 0.0, 0.0);
        let c = f.at(0.0, 0.0, 0.0);
        [p[0] - c[0], 0.0, p[2] - c[2]]
    };
    match l.shape {
        RoofShape::Gabled => {
            for r in [1.0, -1.0] {
                faces.push(vec![f.at(-e, r * w, ye), f.at(e, r * w, ye), f.at(e, 0.0, ridge), f.at(-e, 0.0, ridge)]);
            }
            for s in [1.0, -1.0] {
                ends.push(([f.at(s * f.a, -f.w, top), f.at(s * f.a, f.w, top), f.at(s * f.a, 0.0, ridge)], along(s)));
            }
        }
        RoofShape::Hipped | RoofShape::Pyramidal => {
            let r = if l.shape == RoofShape::Hipped { (f.a - f.w).max(0.0) } else { 0.0 };
            for side in [1.0, -1.0] {
                faces.push(vec![f.at(-e, side * w, ye), f.at(e, side * w, ye), f.at(r, 0.0, ridge), f.at(-r, 0.0, ridge)]);
                faces.push(vec![f.at(side * e, -w, ye), f.at(side * e, w, ye), f.at(side * r, 0.0, ridge)]);
            }
        }
        RoofShape::Skillion => {
            // High along the −r side, falling to the eaves on the +r side.
            let y = |r: f64| top + (f.w - r) * slope;
            faces.push(vec![f.at(-e, w, y(w)), f.at(e, w, y(w)), f.at(e, -w, y(-w)), f.at(-e, -w, y(-w))]);
            for s in [1.0, -1.0] {
                ends.push(([f.at(s * f.a, -f.w, top), f.at(s * f.a, f.w, top), f.at(s * f.a, -f.w, top + rise)], along(s)));
            }
            let back = f.at(0.0, -1.0, 0.0);
            let c = f.at(0.0, 0.0, 0.0);
            let out = [back[0] - c[0], 0.0, back[2] - c[2]];
            for t in [[f.at(-f.a, -f.w, top), f.at(f.a, -f.w, top), f.at(f.a, -f.w, top + rise)], [f.at(-f.a, -f.w, top), f.at(f.a, -f.w, top + rise), f.at(-f.a, -f.w, top + rise)]] {
                ends.push((t, out));
            }
        }
        RoofShape::Flat => {}
    }
    for face in &faces {
        let fan = |mesh: &mut BuildingMesh, facing: [f64; 3], colour: [f32; 3]| {
            for k in 1..face.len() - 1 {
                mesh.triangle([face[0], face[k], face[k + 1]], facing, [colour; 3], [PLAIN; 3]);
            }
        };
        fan(mesh, up, roof);
        // The underside of the eaves, seen from below.
        if o > 0.05 {
            fan(mesh, [0.0, -1.0, 0.0], under);
        }
    }
    for (t, out) in ends {
        mesh.triangle(t, out, [upper; 3], [PLAIN; 3]);
    }
}
