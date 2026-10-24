//! Landmarks that stand out on the skyline: wind turbines, lighthouses, masts and TV
//! towers, chimneys, water towers, windmills, castles and monuments, from OpenStreetMap.
//! They are fetched with the route's road tags, reach a few kilometres out (far beyond
//! the buildings), and are modelled here as one mesh for the whole route, like the
//! bridges. Turbine rotors are left for the renderer to turn.
//!
//! Churches and castles that are mapped as buildings stay buildings: the map only tells
//! the buildings what they are (see `mark_footprints`), and they grow towers there.

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

use serde::{Deserialize, Serialize};

use super::buildings::Footprint;
use super::height::hash2;
use super::mesh::TriMesh;
use super::mvt::Label;
use super::style::{region_at, Region};
use super::surface::query_line;
use super::World;
use crate::gpx::Route;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SightKind {
    WindTurbine,
    Lighthouse,
    WaterTower,
    Chimney,
    /// A guyed or lattice mast.
    Mast,
    /// A concrete telecoms tower with a pod near the top.
    TvTower,
    /// Lookout, bell and other towers.
    Tower,
    Windmill,
    Castle,
    Monument,
    Obelisk,
    /// A place of worship: it marks the building it stands in, which then gets a tower.
    Church,
}

/// A landmark as mapped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sight {
    pub kind: SightKind,
    /// `[lat, lon]`: the node, or the middle of the outline.
    pub at: [f64; 2],
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The mapped `height`, metres (to the hub, for a wind turbine).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// The outline, for a landmark mapped as an area (a castle's walls).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ring: Vec<[f64; 2]>,
}

/// How far from the route each kind of landmark is fetched: tall things from a long way
/// off, places of worship only as far as the buildings go.
pub const SIGHT_RADIUS_M: f64 = 3000.0;
const CASTLE_RADIUS_M: f64 = 2000.0;
const CHURCH_RADIUS_M: f64 = 400.0;
/// Spacing of the route points the landmark search goes round; coarse, as the radius is wide.
const SIGHT_SPACING_M: f64 = 250.0;
/// More than this many is a wind farm seen from everywhere; the rest add little but cost.
const MAX_SIGHTS: usize = 1500;

/// The Overpass QL query for the roads along a stretch of the route (as `way_query`) and the
/// landmarks around it, in one request: Overpass servers ration requests, not results.
pub fn map_query(route: &Route, from: usize, to: usize, radius_m: f64, spacing_m: f64) -> String {
    let (near, far) = (query_line(route, from, to, spacing_m), query_line(route, from, to, SIGHT_SPACING_M));
    format!(
        "[out:json][timeout:25];(way[\"highway\"](around:{radius_m:.0},{near});\
         node[\"generator:source\"=\"wind\"](around:{SIGHT_RADIUS_M:.0},{far});\
         nw[\"man_made\"~\"^(lighthouse|water_tower|chimney|mast|communications_tower|tower|windmill|obelisk)$\"](around:{SIGHT_RADIUS_M:.0},{far});\
         nw[\"historic\"~\"^(castle|monument)$\"](around:{CASTLE_RADIUS_M:.0},{far});\
         nw[\"amenity\"=\"place_of_worship\"](around:{CHURCH_RADIUS_M:.0},{far}););out tags geom;"
    )
}

#[derive(Deserialize)]
struct Reply {
    elements: Vec<Element>,
}

#[derive(Deserialize)]
struct Element {
    #[serde(default)]
    lat: Option<f64>,
    #[serde(default)]
    lon: Option<f64>,
    #[serde(default)]
    tags: HashMap<String, String>,
    #[serde(default)]
    geometry: Vec<Option<Point>>,
}

#[derive(Deserialize)]
struct Point {
    lat: f64,
    lon: f64,
}

fn sight_kind(tags: &HashMap<String, String>) -> Option<SightKind> {
    use SightKind::*;
    let tag = |k: &str| tags.get(k).map(String::as_str);
    if tag("generator:source") == Some("wind") {
        return Some(WindTurbine);
    }
    let kind = match tag("man_made") {
        Some("lighthouse") => Lighthouse,
        Some("water_tower") => WaterTower,
        Some("chimney") => Chimney,
        Some("mast") => Mast,
        Some("communications_tower") => TvTower,
        Some("windmill") => Windmill,
        Some("obelisk") => Obelisk,
        Some("tower") => match tag("tower:type") {
            Some("communication") => Mast,
            // Cooling towers are a shape of their own; lighting and the like are small.
            Some("cooling" | "lighting" | "minaret" | "siren" | "climbing" | "watchtower") => return None,
            _ => Tower,
        },
        _ => match (tag("historic"), tag("amenity")) {
            (Some("castle"), _) => Castle,
            (Some("monument"), _) => Monument,
            (_, Some("place_of_worship")) if matches!(tag("religion"), None | Some("christian")) => Church,
            _ => return None,
        },
    };
    Some(kind)
}

fn leading_number(s: &str) -> Option<f32> {
    let end = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    s[..end].parse().ok()
}

/// The landmarks in an Overpass reply (which may hold roads too: those are left out).
pub fn parse_sights(json: &str) -> Result<Vec<Sight>, String> {
    let reply: Reply = serde_json::from_str(json).map_err(|e| format!("Unexpected reply from the OpenStreetMap server: {e}"))?;
    let mut out: Vec<Sight> = Vec::new();
    for e in reply.elements {
        let tags = &e.tags;
        let tag = |k: &str| tags.get(k).map(String::as_str);
        if tag("highway").is_some() {
            continue;
        }
        let Some(kind) = sight_kind(&e.tags) else { continue };
        let line: Vec<[f64; 2]> = e.geometry.iter().flatten().map(|p| [p.lat, p.lon]).collect();
        let (at, ring) = match (e.lat, e.lon) {
            (Some(lat), Some(lon)) => ([lat, lon], Vec::new()),
            _ if line.len() >= 2 => {
                let closed = line.len() >= 4 && line.first() == line.last();
                let pts = if closed { &line[..line.len() - 1] } else { &line[..] };
                let n = pts.len() as f64;
                ([pts.iter().map(|p| p[0]).sum::<f64>() / n, pts.iter().map(|p| p[1]).sum::<f64>() / n], if closed { pts.to_vec() } else { Vec::new() })
            }
            _ => continue,
        };
        let height = tag("height").and_then(leading_number).filter(|h| (2.0..=650.0).contains(h));
        // The same thing is sometimes mapped twice, as a node and as an outline.
        if out.iter().any(|s| s.kind == kind && (s.at[0] - at[0]).abs() < 5e-5 && (s.at[1] - at[1]).abs() < 7e-5) {
            continue;
        }
        out.push(Sight { kind, at, name: tag("name").unwrap_or_default().chars().take(48).collect(), height, ring });
    }
    Ok(out)
}

/// Castles and churches from the map tiles' points of interest, for when Overpass has not
/// answered: no outlines or heights, but enough to mark their buildings.
pub fn sights_from_labels(labels: &[Label]) -> Vec<Sight> {
    labels
        .iter()
        .filter_map(|l| {
            let kind = match l.kind.as_str() {
                "castle" => SightKind::Castle,
                "church" => SightKind::Church,
                _ => return None,
            };
            Some(Sight { kind, at: l.at, name: l.name.clone(), height: None, ring: Vec::new() })
        })
        .collect()
}

fn inside(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
    let n = ring.len();
    let mut hit = false;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a[0] > p[0]) != (b[0] > p[0]) && p[1] < (b[1] - a[1]) * (p[0] - a[0]) / (b[0] - a[0]) + a[1] {
            hit = !hit;
        }
    }
    hit
}

/// What the landmarks say about the buildings they stand in. A church's building is marked
/// as a church (and gets a tower); a castle's as a castle (stone, battlements) — either way
/// the landmark itself is then left to the building. A lighthouse, water tower, windmill,
/// tower, chimney or monument replaces the plain block its outline would otherwise be.
/// Returns the footprints to build and the landmarks still to model.
pub fn mark_footprints(footprints: &[Footprint], sights: &[Sight]) -> (Vec<Footprint>, Vec<Sight>) {
    let mut footprints = footprints.to_vec();
    let bounds: Vec<[f64; 4]> = footprints
        .iter()
        .map(|f| f.ring.iter().fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]))
        .collect();
    let mut gone = vec![false; footprints.len()];
    let mut left = Vec::new();
    for s in sights {
        let home =
            (0..footprints.len()).find(|&i| !gone[i] && s.at[0] >= bounds[i][0] && s.at[0] <= bounds[i][2] && s.at[1] >= bounds[i][1] && s.at[1] <= bounds[i][3] && inside(&footprints[i].ring, s.at));
        match (s.kind, home) {
            (SightKind::Church, Some(i)) => {
                let f = &mut footprints[i];
                if !matches!(f.tags.kind.as_str(), "church" | "cathedral" | "chapel" | "mosque" | "synagogue" | "temple") {
                    f.tags.kind = "church".into();
                }
            }
            (SightKind::Church, None) => {}
            (SightKind::Castle, Some(i)) => {
                let f = &mut footprints[i];
                f.tags.kind = "castle".into();
                f.height = f.height.max(10.0);
            }
            (SightKind::WindTurbine | SightKind::Mast | SightKind::TvTower | SightKind::Castle, _) => left.push(s.clone()),
            (_, Some(i)) => {
                gone[i] = true;
                left.push(s.clone());
            }
            (_, None) => left.push(s.clone()),
        }
    }
    let footprints = footprints.into_iter().zip(gone).filter(|(_, g)| !g).map(|(f, _)| f).collect();
    (footprints, left)
}

/// A wind turbine's rotor, for the renderer to turn.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rotor {
    /// The hub, in the route's local frame.
    pub hub: [f64; 3],
    /// Unit `(x, z)` direction the rotor faces: into the wind.
    pub facing: [f64; 2],
    pub radius: f64,
    /// Starting angle of the first blade, radians, so a wind farm doesn't turn in step.
    pub phase: f64,
}

/// The way turbines face until there is weather to say otherwise: into a west-south-westerly.
const WIND_FROM_DEG: f64 = 250.0;
/// How far the foot of a landmark reaches below the ground under it: the distant landscape is
/// coarser than the near, and a tower must not stand on air where it dips below the truth.
const SINK: f64 = 25.0;

pub(super) fn linear(c: [f64; 3]) -> [f32; 3] {
    c.map(|v| v.clamp(0.0, 1.0).powf(2.2) as f32)
}

/// Draws `count` blades round `hub` in the plane facing `facing`: each a long taper from
/// `root` to `tip` half-width, out to `radius`, the first at `angle`. Seen from both sides.
pub fn blades(mesh: &mut TriMesh, rotor: &Rotor, count: usize, angle: f64, (root, tip): (f64, f64), colour: [f32; 3]) {
    let Rotor { hub, facing, radius, .. } = *rotor;
    let side = [-facing[1], 0.0, facing[0]];
    let front = [facing[0], 0.0, facing[1]];
    for k in 0..count {
        let t = angle + k as f64 * TAU / count as f64;
        let dir = [side[0] * t.cos(), t.sin(), side[2] * t.cos()];
        let across = [-side[0] * t.sin(), t.cos(), -side[2] * t.sin()];
        let p = |d: f64, w: f64| [0, 1, 2].map(|i| hub[i] + dir[i] * d + across[i] * w);
        let q = [p(radius * 0.04, -root), p(radius, -tip), p(radius, tip), p(radius * 0.04, root)];
        mesh.quad(q, front, [colour; 4]);
        mesh.quad(q, front.map(|v| -v), [colour; 4]);
    }
}

/// The walls of an upright prism with `sides` sides round `c`, tapering from corner radius
/// `r0` at `y0` to `r1` at `y1` (0 makes a cone), turned by `turn` radians.
pub(super) fn prism(m: &mut TriMesh, c: (f64, f64), (y0, y1): (f64, f64), (r0, r1): (f64, f64), sides: usize, turn: f64, colour: [f64; 3]) {
    let (lo, hi) = (linear(colour.map(|v| v * 0.85)), linear(colour));
    let p = |a: f64, r: f64, y: f64| [c.0 + a.cos() * r, y, c.1 + a.sin() * r];
    for k in 0..sides {
        let a0 = turn + k as f64 * TAU / sides as f64;
        let a1 = a0 + TAU / sides as f64;
        let mid = (a0 + a1) / 2.0;
        m.quad([p(a0, r0, y0), p(a1, r0, y0), p(a1, r1, y1), p(a0, r1, y1)], [mid.cos(), 0.0, mid.sin()], [lo, lo, hi, hi]);
    }
}

/// A flat lid (facing up) or floor (facing down) on such a prism.
pub(super) fn cap(m: &mut TriMesh, c: (f64, f64), (y, r): (f64, f64), sides: usize, turn: f64, up: bool, colour: [f64; 3]) {
    let col = linear(colour);
    let p = |a: f64| [c.0 + a.cos() * r, y, c.1 + a.sin() * r];
    for k in 0..sides {
        let a0 = turn + k as f64 * TAU / sides as f64;
        m.triangle([[c.0, y, c.1], p(a0), p(a0 + TAU / sides as f64)], [0.0, if up { 1.0 } else { -1.0 }, 0.0], [col; 3], [[0.04; 2]; 3]);
    }
}

/// A square-based prism of half-side `h` facing along `turn`, closed on top.
pub(super) fn square(m: &mut TriMesh, c: (f64, f64), (y0, y1): (f64, f64), (h0, h1): (f64, f64), turn: f64, colour: [f64; 3]) {
    prism(m, c, (y0, y1), (h0 * 2f64.sqrt(), h1 * 2f64.sqrt()), 4, turn + PI / 4.0, colour);
    if h1 > 0.0 {
        cap(m, c, (y1, h1 * 2f64.sqrt()), 4, turn + PI / 4.0, true, colour);
    }
}

/// Merlons along the top of a wall from `a` to `b` (each `(x, z, top)`), set in from the
/// outer face along `inward`.
pub(super) fn merlons(m: &mut TriMesh, a: (f64, f64, f64), b: (f64, f64, f64), inward: (f64, f64), colour: [f64; 3]) {
    let len = (b.0 - a.0).hypot(b.1 - a.1);
    let n = (len / 2.6).floor() as usize;
    if n == 0 {
        return;
    }
    let u = ((b.0 - a.0) / len, (b.1 - a.1) / len);
    for k in 0..n {
        let t = (k as f64 + 0.5) / n as f64;
        let c = (a.0 + (b.0 - a.0) * t + inward.0 * 0.4, a.1 + (b.1 - a.1) * t + inward.1 * 0.4);
        let top = a.2 + (b.2 - a.2) * t;
        // A merlon 1.3 m along the wall, 0.8 m thick, 1.2 m high.
        let (s, w) = (0.65, 0.4);
        let corner = |ds: f64, dw: f64| (c.0 + u.0 * ds - u.1 * dw, c.1 + u.1 * ds + u.0 * dw);
        let pts = [corner(-s, -w), corner(s, -w), corner(s, w), corner(-s, w)];
        let col = linear(colour);
        for i in 0..4 {
            let (p, q) = (pts[i], pts[(i + 1) % 4]);
            let mid = ((p.0 + q.0) / 2.0 - c.0, (p.1 + q.1) / 2.0 - c.1);
            m.quad([[p.0, top, p.1], [q.0, top, q.1], [q.0, top + 1.2, q.1], [p.0, top + 1.2, p.1]], [mid.0, 0.0, mid.1], [col; 4]);
        }
        m.quad(pts.map(|p| [p.0, top + 1.2, p.1]), [0.0, 1.0, 0.0], [col; 4]);
    }
}

const WHITE: [f64; 3] = [0.93, 0.94, 0.94];
const RED: [f64; 3] = [0.78, 0.16, 0.13];
const CONCRETE: [f64; 3] = [0.76, 0.75, 0.71];
const STONE: [f64; 3] = [0.66, 0.62, 0.55];
const BRICK: [f64; 3] = [0.56, 0.31, 0.23];
const SLATE: [f64; 3] = [0.30, 0.31, 0.35];
const TERRACOTTA: [f64; 3] = [0.66, 0.33, 0.22];
const GLASS: [f64; 3] = [0.62, 0.74, 0.72];

impl World {
    /// The modelled landmarks and the rotors to turn on them.
    pub(super) fn sight_models(&self, mesh: &mut TriMesh, rotors: &mut Vec<Rotor>) {
        let wind = WIND_FROM_DEG.to_radians();
        // Facing into the wind: towards where it comes from, with z pointing south.
        let facing = [wind.sin(), -wind.cos()];
        for (k, s) in self.sights.iter().take(MAX_SIGHTS).enumerate() {
            let (x, z) = self.route.to_local(s.at[0], s.at[1]);
            // A landmark mapped on the road itself is a mapping slip.
            if self.field.nearest_sample(x, z).is_some_and(|(d, _)| d < 8.0) {
                continue;
            }
            let seed = (x.round() as i32).wrapping_mul(31) ^ z.round() as i32 ^ k as i32;
            let g = self.field.height(x, z).0;
            let foot = g - SINK;
            let h = s.height.map(f64::from);
            let c = (x, z);
            match s.kind {
                SightKind::WindTurbine => {
                    let hub = h.unwrap_or(95.0).clamp(12.0, 170.0);
                    prism(mesh, c, (foot, g + hub), (hub * 0.026, hub * 0.015), 12, 0.0, WHITE);
                    // The nacelle, behind the hub.
                    let len = hub * 0.11;
                    let back = (x - facing[0] * len * 0.35, z - facing[1] * len * 0.35);
                    let half = hub * 0.022;
                    let corner = |a: f64, b: f64| (back.0 + facing[0] * a - facing[1] * b, back.1 + facing[1] * a + facing[0] * b);
                    let pts = [corner(-len / 2.0, -half), corner(len / 2.0, -half), corner(len / 2.0, half), corner(-len / 2.0, half)];
                    let (y0, y1) = (g + hub - half, g + hub + half * 1.2);
                    let col = linear(WHITE);
                    for i in 0..4 {
                        let (p, q) = (pts[i], pts[(i + 1) % 4]);
                        let out = ((p.0 + q.0) / 2.0 - back.0, (p.1 + q.1) / 2.0 - back.1);
                        mesh.quad([[p.0, y0, p.1], [q.0, y0, q.1], [q.0, y1, q.1], [p.0, y1, p.1]], [out.0, 0.0, out.1], [col; 4]);
                    }
                    mesh.quad(pts.map(|p| [p.0, y1, p.1]), [0.0, 1.0, 0.0], [col; 4]);
                    let hub_at = [x + facing[0] * len * 0.25, g + hub, z + facing[1] * len * 0.25];
                    rotors.push(Rotor { hub: hub_at, facing, radius: (hub * 0.47).min(hub - 6.0), phase: hash2(seed, 1) * TAU });
                }
                SightKind::Lighthouse => {
                    let height = h.unwrap_or(20.0).clamp(6.0, 70.0);
                    let r0 = (height * 0.13).max(2.0);
                    let r1 = r0 * 0.72;
                    let r = |y: f64| r0 + (r1 - r0) * (y / height);
                    let banded = hash2(seed, 2) < 0.6;
                    let bands = 4;
                    for b in 0..bands {
                        let (y0, y1) = (height * b as f64 / bands as f64, height * (b + 1) as f64 / bands as f64);
                        let colour = if banded && b % 2 == 1 { RED } else { WHITE };
                        prism(mesh, c, (if b == 0 { foot } else { g + y0 }, g + y1), (if b == 0 { r0 } else { r(y0) }, r(y1)), 12, 0.0, colour);
                    }
                    let top = g + height;
                    // Gallery, lantern and dome.
                    prism(mesh, c, (top - 0.3, top), (r1 + 0.7, r1 + 0.7), 12, 0.0, SLATE);
                    cap(mesh, c, (top, r1 + 0.7), 12, 0.0, true, SLATE);
                    cap(mesh, c, (top - 0.3, r1 + 0.7), 12, 0.0, false, SLATE);
                    prism(mesh, c, (top, top + 2.6), (r1 * 0.62, r1 * 0.62), 10, 0.0, GLASS);
                    prism(mesh, c, (top + 2.6, top + 4.0), (r1 * 0.75, 0.0), 10, 0.0, if banded { RED } else { SLATE });
                }
                SightKind::WaterTower => {
                    let height = h.unwrap_or(30.0).clamp(10.0, 80.0);
                    let tank = (height * 0.22).clamp(4.0, 10.0);
                    let stem = (tank * 0.33).max(1.5);
                    let concrete = CONCRETE.map(|v| v * (0.92 + hash2(seed, 3) * 0.12));
                    prism(mesh, c, (foot, g + height * 0.66), (stem, stem), 12, 0.0, concrete);
                    prism(mesh, c, (g + height * 0.62, g + height * 0.74), (stem, tank), 14, 0.0, concrete);
                    prism(mesh, c, (g + height * 0.74, g + height * 0.93), (tank, tank), 14, 0.0, concrete);
                    prism(mesh, c, (g + height * 0.93, g + height), (tank, tank * 0.25), 14, 0.0, SLATE);
                    cap(mesh, c, (g + height, tank * 0.25), 14, 0.0, true, SLATE);
                }
                SightKind::Chimney => {
                    let height = h.unwrap_or(45.0).clamp(10.0, 300.0);
                    let r0 = (height * 0.05).max(1.2);
                    let colour = if hash2(seed, 4) < 0.5 { BRICK } else { CONCRETE };
                    prism(mesh, c, (foot, g + height - 1.5), (r0, r0 * 0.62), 12, 0.0, colour);
                    prism(mesh, c, (g + height - 1.5, g + height), (r0 * 0.64, r0 * 0.62), 12, 0.0, SLATE);
                    cap(mesh, c, (g + height, r0 * 0.62), 12, 0.0, true, [0.12, 0.12, 0.12]);
                }
                SightKind::Mast => {
                    let height = h.unwrap_or(60.0).clamp(10.0, 400.0);
                    // A slim lattice reads as a tapering three-sided shaft from any distance.
                    let r0 = height * 0.012 + 0.5;
                    let bands = 7;
                    for b in 0..bands {
                        let t0 = b as f64 / bands as f64;
                        let t1 = (b + 1) as f64 / bands as f64;
                        let rr = |t: f64| r0 + (0.25 - r0) * t;
                        prism(mesh, c, (if b == 0 { foot } else { g + height * t0 }, g + height * t1), (if b == 0 { r0 } else { rr(t0) }, rr(t1)), 3, 0.0, if b % 2 == 0 { RED } else { WHITE });
                    }
                }
                SightKind::TvTower => {
                    let height = h.unwrap_or(150.0).clamp(40.0, 400.0);
                    let (r0, r1) = (height * 0.045, height * 0.026);
                    let shaft = height * 0.78;
                    prism(mesh, c, (foot, g + shaft), (r0, r1), 16, 0.0, CONCRETE);
                    let pod = (height * 0.075).max(r1 + 2.0);
                    let (p0, p1) = (g + height * 0.68, g + height * 0.75);
                    prism(mesh, c, (p0 - 2.0, p0), (r1, pod), 16, 0.0, CONCRETE);
                    prism(mesh, c, (p0, p1), (pod, pod), 16, 0.0, GLASS.map(|v| v * 0.8));
                    prism(mesh, c, (p1, p1 + 1.5), (pod, r1), 16, 0.0, CONCRETE);
                    cap(mesh, c, (g + shaft, r1), 16, 0.0, true, CONCRETE);
                    let mast = height - shaft;
                    for b in 0..4 {
                        let (t0, t1) = (b as f64 / 4.0, (b + 1) as f64 / 4.0);
                        prism(mesh, c, (g + shaft + mast * t0, g + shaft + mast * t1), (0.9 - 0.6 * t0, 0.9 - 0.6 * t1), 6, 0.0, if b % 2 == 0 { RED } else { WHITE });
                    }
                }
                SightKind::Tower => {
                    let height = h.unwrap_or(18.0).clamp(5.0, 120.0);
                    let half = (height * 0.14).clamp(1.8, 4.0);
                    let turn = hash2(seed, 5) * PI;
                    let region = region_at(s.at[0], s.at[1]);
                    let (wall, roof) = match region {
                        Region::Mediterranean | Region::Arid => ([0.80, 0.70, 0.55], TERRACOTTA),
                        _ => (STONE, SLATE),
                    };
                    square(mesh, c, (foot, g + height), (half, half), turn, wall);
                    // A low pyramid roof with a little overhang.
                    square(mesh, c, (g + height, g + height + half * 1.4), (half + 0.4, 0.0), turn, roof);
                }
                SightKind::Windmill => {
                    let height = h.unwrap_or(13.0).clamp(6.0, 35.0);
                    let r0 = (height * 0.32).clamp(2.5, 6.0);
                    let r1 = r0 * 0.7;
                    let wall = if hash2(seed, 6) < 0.5 { [0.90, 0.88, 0.82] } else { BRICK };
                    prism(mesh, c, (foot, g + height), (r0, r1), 8, 0.0, wall);
                    prism(mesh, c, (g + height, g + height + r1 * 1.1), (r1 + 0.3, 0.0), 8, 0.0, SLATE);
                    // Sails, still: most windmills are kept, not worked.
                    let hub = [x + facing[0] * (r1 + 0.8), g + height - 0.2, z + facing[1] * (r1 + 0.8)];
                    let sails = Rotor { hub, facing, radius: (height * 0.8).min(12.0), phase: 0.0 };
                    blades(mesh, &sails, 4, PI / 4.0 + hash2(seed, 7), (0.9, 0.9), linear([0.45, 0.33, 0.22]));
                }
                SightKind::Castle => self.castle(mesh, s, seed),
                SightKind::Monument => {
                    let height = h.unwrap_or(9.0).clamp(3.0, 60.0);
                    let turn = hash2(seed, 8) * PI;
                    let base = (height * 0.25).clamp(1.5, 5.0);
                    square(mesh, c, (g - 2.0, g + base * 0.35), (base, base), turn, STONE.map(|v| v * 0.9));
                    square(mesh, c, (g + base * 0.35, g + height * 0.92), (base * 0.38, base * 0.34), turn, STONE);
                    square(mesh, c, (g + height * 0.92, g + height), (base * 0.45, base * 0.45), turn, STONE.map(|v| v * 0.9));
                }
                SightKind::Obelisk => {
                    let height = h.unwrap_or(15.0).clamp(3.0, 170.0);
                    let turn = hash2(seed, 9) * PI;
                    let half = height * 0.05;
                    square(mesh, c, (g - 2.0, g + height * 0.9), (half, half * 0.65), turn, STONE);
                    square(mesh, c, (g + height * 0.9, g + height), (half * 0.65, 0.0), turn, STONE);
                }
                SightKind::Church => {}
            }
        }
    }

    /// A castle standing on its own: a curtain wall with round towers along its outline if it
    /// has one, and a keep in the middle unless its buildings already fill it.
    fn castle(&self, mesh: &mut TriMesh, s: &Sight, seed: i32) {
        let region = region_at(s.at[0], s.at[1]);
        let cones = !matches!(region, Region::Mediterranean | Region::Arid);
        let mut ring: Vec<(f64, f64)> = s.ring.iter().map(|p| self.route.to_local(p[0], p[1])).collect();
        let n = ring.len();
        let area = (0..n).map(|i| ring[i].0 * ring[(i + 1) % n].1 - ring[(i + 1) % n].0 * ring[i].1).sum::<f64>() / 2.0;
        if area < 0.0 {
            ring.reverse();
        }
        let perimeter: f64 = (0..n).map(|i| (ring[(i + 1) % n].0 - ring[i].0).hypot(ring[(i + 1) % n].1 - ring[i].1)).sum();
        let walled = n >= 3 && (300.0..=80_000.0).contains(&area.abs()) && perimeter < 1500.0;
        let (cx, cz) = self.route.to_local(s.at[0], s.at[1]);
        let tower = |mesh: &mut TriMesh, c: (f64, f64), r: f64, height: f64| {
            let g = self.field.height(c.0, c.1).0;
            prism(mesh, c, (g - SINK, g + height), (r, r), 10, 0.0, STONE);
            if cones {
                prism(mesh, c, (g + height, g + height + r * 1.6), (r + 0.4, 0.0), 10, 0.0, SLATE);
            } else {
                cap(mesh, c, (g + height, r), 10, 0.0, true, STONE.map(|v| v * 0.9));
            }
        };
        if walled {
            const WALL: f64 = 8.0;
            const THICK: f64 = 1.6;
            let ground: Vec<f64> = ring.iter().map(|p| self.field.height(p.0, p.1).0).collect();
            let (light, dark) = (linear(STONE), linear(STONE.map(|v| v * 0.8)));
            for i in 0..n {
                let j = (i + 1) % n;
                let (a, b) = (ring[i], ring[j]);
                let len = (b.0 - a.0).hypot(b.1 - a.1).max(1e-6);
                let out = ((b.1 - a.1) / len, (a.0 - b.0) / len);
                let inn = |p: (f64, f64)| (p.0 - out.0 * THICK, p.1 - out.1 * THICK);
                let (ta, tb) = (ground[i] + WALL, ground[j] + WALL);
                let (ia, ib) = (inn(a), inn(b));
                mesh.quad([[a.0, ground[i] - 3.0, a.1], [b.0, ground[j] - 3.0, b.1], [b.0, tb, b.1], [a.0, ta, a.1]], [out.0, 0.0, out.1], [dark, dark, light, light]);
                mesh.quad([[ia.0, ground[i] - 3.0, ia.1], [ib.0, ground[j] - 3.0, ib.1], [ib.0, tb, ib.1], [ia.0, ta, ia.1]], [-out.0, 0.0, -out.1], [dark, dark, light, light]);
                mesh.quad([[a.0, ta, a.1], [b.0, tb, b.1], [ib.0, tb, ib.1], [ia.0, ta, ia.1]], [0.0, 1.0, 0.0], [light; 4]);
                merlons(mesh, (a.0, a.1, ta), (b.0, b.1, tb), (-out.0, -out.1), STONE);
            }
            // Towers where the wall turns a corner, at most a dozen.
            let corners: Vec<usize> = (0..n)
                .filter(|&i| {
                    let (p, q, r) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
                    let (a1, a2) = ((q.1 - p.1).atan2(q.0 - p.0), (r.1 - q.1).atan2(r.0 - q.0));
                    let turn = (a2 - a1 + PI).rem_euclid(TAU) - PI;
                    turn.abs() > 0.5
                })
                .collect();
            let every = corners.len().div_ceil(12).max(1);
            for &i in corners.iter().step_by(every) {
                tower(mesh, ring[i], 3.4, WALL + 4.0);
            }
        }
        if !self.buildings.covers(cx, cz) && (!walled || area.abs() > 2500.0) {
            // The keep, with turrets at its corners when it stands alone.
            let g = self.field.height(cx, cz).0;
            let height = s.height.map_or(20.0, f64::from).clamp(10.0, 45.0);
            let turn = hash2(seed, 10) * PI;
            let half = 6.5;
            square(mesh, (cx, cz), (g - SINK, g + height), (half, half), turn, STONE);
            let corner = |k: usize| {
                let a = turn + PI / 4.0 + k as f64 * PI / 2.0;
                ((cx + a.cos() * half * 2f64.sqrt()), (cz + a.sin() * half * 2f64.sqrt()))
            };
            for k in 0..4 {
                let (p, q) = (corner(k), corner(k + 1));
                let mid = ((p.0 + q.0) / 2.0 - cx, (p.1 + q.1) / 2.0 - cz);
                merlons(mesh, (p.0, p.1, g + height), (q.0, q.1, g + height), (-mid.0 / half, -mid.1 / half), STONE);
                if !walled {
                    tower(mesh, p, 2.4, height + 4.0);
                }
            }
        }
    }
}
