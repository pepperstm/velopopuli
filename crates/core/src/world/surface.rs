//! What the route's road is made of: tarmac, concrete, cobbles, gravel or dirt.
//!
//! OpenStreetMap records it in each road's `surface` tag. The vector tiles leave that
//! tag out, so it is asked of the Overpass API for the roads along the route; where no
//! answer is to be had, it is guessed from the kind of road (a track is gravel, a path dirt).

use serde::{Deserialize, Serialize};

use crate::gpx::Route;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum Surface {
    #[default]
    Asphalt = 0,
    Concrete = 1,
    /// Setts, cobblestones and paving stones.
    Cobbles = 2,
    /// Gravel, compacted or loose, and other unpaved but made-up surfaces.
    Gravel = 3,
    /// Earth, grass, mud and sand.
    Dirt = 4,
}

impl Surface {
    pub const ALL: [Surface; 5] = [Surface::Asphalt, Surface::Concrete, Surface::Cobbles, Surface::Gravel, Surface::Dirt];

    /// From an OpenStreetMap `surface` value; `None` for values that say nothing useful.
    pub fn from_osm(value: &str) -> Option<Surface> {
        Some(match value.split(';').next().unwrap_or("").trim() {
            "asphalt" | "paved" | "chipseal" | "metal" | "wood" | "rubber" | "tartan" => Surface::Asphalt,
            "concrete" | "concrete:plates" | "concrete:lanes" => Surface::Concrete,
            "sett" | "cobblestone" | "unhewn_cobblestone" | "cobblestone:flattened" | "paving_stones" | "paving_stones:lanes" | "bricks" | "stone" | "grass_paver" => Surface::Cobbles,
            "compacted" | "fine_gravel" | "gravel" | "unpaved" | "pebblestone" | "rock" | "shells" => Surface::Gravel,
            "dirt" | "ground" | "earth" | "grass" | "mud" | "sand" | "soil" | "woodchips" | "snow" | "ice" => Surface::Dirt,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Surface::Asphalt => "asphalt",
            Surface::Concrete => "concrete",
            Surface::Cobbles => "cobbles",
            Surface::Gravel => "gravel",
            Surface::Dirt => "dirt",
        }
    }
}

/// A mapped road with a known surface, as `[lat, lon]` points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceWay {
    pub surface: Surface,
    pub line: Vec<[f64; 2]>,
}

/// The Overpass QL query for every road with a `surface` tag within `radius_m` of a stretch of the route.
pub fn surface_query(route: &Route, from: usize, to: usize, radius_m: f64, spacing_m: f64) -> String {
    let stride = ((spacing_m / route.step).round() as usize).max(1);
    let mut line: Vec<String> = (from..to).step_by(stride).map(|i| format!("{:.5},{:.5}", route.lat[i], route.lon[i])).collect();
    line.push(format!("{:.5},{:.5}", route.lat[to - 1], route.lon[to - 1]));
    format!("[out:json][timeout:25];way[\"highway\"][\"surface\"](around:{radius_m:.0},{});out tags geom;", line.join(","))
}

#[derive(Deserialize)]
struct Reply {
    elements: Vec<Element>,
}

#[derive(Deserialize)]
struct Element {
    #[serde(default)]
    tags: std::collections::HashMap<String, String>,
    #[serde(default)]
    geometry: Vec<Point>,
}

#[derive(Deserialize)]
struct Point {
    lat: f64,
    lon: f64,
}

/// Roads with a known surface from an Overpass reply produced with `out tags geom`.
pub fn parse_surfaces(json: &str) -> Result<Vec<SurfaceWay>, String> {
    let reply: Reply = serde_json::from_str(json).map_err(|e| format!("Unexpected reply from the OpenStreetMap server: {e}"))?;
    Ok(reply
        .elements
        .into_iter()
        .filter(|e| e.geometry.len() >= 2)
        .filter_map(|e| Some(SurfaceWay { surface: Surface::from_osm(e.tags.get("surface")?)?, line: e.geometry.iter().map(|p| [p.lat, p.lon]).collect() }))
        .collect())
}

/// Stretches shorter than this are taken to be mapping noise (a patch at a junction,
/// a bridge deck tagged differently) and given the surface of the road either side.
pub const MIN_RUN_M: f64 = 30.0;

/// Merges runs of the same surface shorter than `min_samples` into what comes before them
/// (or, at the very start, after them).
pub fn tidy(surfaces: &mut [Surface], min_samples: usize) {
    let n = surfaces.len();
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j < n && surfaces[j] == surfaces[i] {
            j += 1;
        }
        if j - i < min_samples && n > j - i {
            let fill = if i > 0 { surfaces[i - 1] } else { surfaces[j.min(n - 1)] };
            surfaces[i..j].fill(fill);
        }
        i = j;
    }
}

/// `(first sample, one past the last, surface)` for each stretch of one surface.
pub fn runs(surfaces: &[Surface]) -> Vec<(usize, usize, Surface)> {
    let mut out: Vec<(usize, usize, Surface)> = Vec::new();
    for (i, &s) in surfaces.iter().enumerate() {
        match out.last_mut() {
            Some(last) if last.2 == s => last.1 = i + 1,
            _ => out.push((i, i + 1, s)),
        }
    }
    out
}
