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

/// A power line's support in local metres: a lattice tower or a pole.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Support {
    pub at: (f64, f64),
    pub tower: bool,
}

/// The wires between two neighbouring supports of one line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub a: (f64, f64),
    pub b: (f64, f64),
    pub tower: bool,
}

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
            for w in held.windows(2) {
                scene.spans.push(Span { a: w[0], b: w[1], tower });
            }
            scene.supports.extend(held.into_iter().map(|at| Support { at, tower }));
        }
        for t in &data.trees {
            let (x, z) = route.to_local(t[0], t[1]);
            scene.trees.entry(((x / CHUNK).floor() as i32, (z / CHUNK).floor() as i32)).or_default().push((x, z, t[2]));
        }
        scene
    }
}
