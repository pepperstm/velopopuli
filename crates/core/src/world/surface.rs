//! What the route's road is like: what it is made of (tarmac, concrete, cobbles, gravel or
//! dirt) and how wide it is.
//!
//! OpenStreetMap records these in each road's `highway`, `surface`, `width` and `lanes` tags.
//! The vector tiles carry only a coarse road class, so the tags are asked of the Overpass API
//! for the roads along the route; where no answer is to be had, both are guessed from the
//! class (a track is gravel and narrow, a main road tarmac and wide).

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

/// A mapped road near the route and what its tags say, as `[lat, lon]` points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MappedWay {
    /// The `highway` value: "primary", "residential", "track"…
    pub highway: String,
    #[serde(default)]
    pub surface: Option<Surface>,
    /// Carriageway width in metres, from `width` or else `lanes`.
    #[serde(default)]
    pub width: Option<f64>,
    pub line: Vec<[f64; 2]>,
}

/// Version of what `MappedWay` holds; caches from older versions are fetched again.
pub const WAY_TAGS: u32 = 2;

/// Usual carriageway width in metres for an OpenStreetMap `highway` class.
pub fn highway_width(highway: &str) -> Option<f64> {
    Some(match highway {
        "motorway" | "trunk" => 7.5,
        "primary" => 7.0,
        "secondary" => 6.5,
        "tertiary" => 6.0,
        "motorway_link" | "trunk_link" | "primary_link" | "secondary_link" | "tertiary_link" => 4.5,
        "unclassified" | "residential" | "busway" => 5.0,
        "living_street" | "pedestrian" => 4.5,
        "service" => 3.5,
        "track" => 3.0,
        "cycleway" => 2.5,
        "path" | "footway" | "bridleway" | "steps" => 2.0,
        _ => return None,
    })
}

/// Whether a road of this OpenStreetMap `highway` class has a line painted down the middle.
pub fn highway_marked(highway: &str) -> bool {
    matches!(highway.trim_end_matches("_link"), "motorway" | "trunk" | "primary" | "secondary" | "tertiary")
}

/// How the route's road is drawn at each sample: `Surface as u8`, or `UNMARKED` for
/// tarmac without a centre line (anything smaller than a main road).
pub const UNMARKED: u8 = 5;

/// Width in metres from a `width` tag ("5", "5.5 m", "6m") or, failing that, a `lanes` tag.
pub fn tagged_width(width: Option<&str>, lanes: Option<&str>) -> Option<f64> {
    let metres = width.and_then(|w| w.trim().trim_end_matches('m').trim().replace(',', ".").parse::<f64>().ok()).filter(|w| (1.0..40.0).contains(w));
    let from_lanes = lanes.and_then(|l| l.trim().parse::<f64>().ok()).filter(|l| (1.0..=8.0).contains(l)).map(|l| l * 3.25 + 0.5);
    metres.or(from_lanes)
}

/// The narrowest and widest the route's road is drawn, whatever the map says.
pub const MIN_WIDTH: f64 = 2.4;
pub const MAX_WIDTH: f64 = 11.0;

/// The Overpass QL query for every road within `radius_m` of a stretch of the route.
pub fn way_query(route: &Route, from: usize, to: usize, radius_m: f64, spacing_m: f64) -> String {
    let stride = ((spacing_m / route.step).round() as usize).max(1);
    let mut line: Vec<String> = (from..to).step_by(stride).map(|i| format!("{:.5},{:.5}", route.lat[i], route.lon[i])).collect();
    line.push(format!("{:.5},{:.5}", route.lat[to - 1], route.lon[to - 1]));
    format!("[out:json][timeout:25];way[\"highway\"](around:{radius_m:.0},{});out tags geom;", line.join(","))
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

/// Roads and their tags from an Overpass reply produced with `out tags geom`.
pub fn parse_ways(json: &str) -> Result<Vec<MappedWay>, String> {
    let reply: Reply = serde_json::from_str(json).map_err(|e| format!("Unexpected reply from the OpenStreetMap server: {e}"))?;
    Ok(reply
        .elements
        .into_iter()
        .filter(|e| e.geometry.len() >= 2)
        .filter_map(|e| {
            let tag = |k: &str| e.tags.get(k).map(String::as_str);
            Some(MappedWay {
                highway: tag("highway")?.to_string(),
                surface: tag("surface").and_then(Surface::from_osm),
                width: tagged_width(tag("width").or(tag("est_width")), tag("lanes")),
                line: e.geometry.iter().map(|p| [p.lat, p.lon]).collect(),
            })
        })
        .collect())
}

/// Evens out a width per sample: a median over `window` samples drops stray values, then a
/// running mean over the same span tapers the changes from one width to the next.
pub fn smooth_widths(widths: &mut [f64], window: usize) {
    let n = widths.len();
    let half = window / 2;
    if n == 0 || half == 0 {
        return;
    }
    let median: Vec<f64> = (0..n)
        .map(|i| {
            let mut w: Vec<f64> = widths[i.saturating_sub(half)..(i + half + 1).min(n)].to_vec();
            w.sort_by(f64::total_cmp);
            w[w.len() / 2]
        })
        .collect();
    let mut sum = vec![0.0; n + 1];
    for i in 0..n {
        sum[i + 1] = sum[i] + median[i];
    }
    for (i, w) in widths.iter_mut().enumerate() {
        let (a, b) = (i.saturating_sub(half), (i + half + 1).min(n));
        *w = (sum[b] - sum[a]) / (b - a) as f64;
    }
}

/// Stretches shorter than this are taken to be mapping noise (a patch at a junction,
/// a bridge deck tagged differently) and given the surface of the road either side.
pub const MIN_RUN_M: f64 = 30.0;

/// Merges runs of the same surface shorter than `min_samples` into what comes before them
/// (or, at the very start, after them).
pub fn tidy<T: Copy + PartialEq>(surfaces: &mut [T], min_samples: usize) {
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

/// `(first sample, one past the last, value)` for each stretch of one surface (or look).
pub fn runs<T: Copy + PartialEq>(surfaces: &[T]) -> Vec<(usize, usize, T)> {
    let mut out: Vec<(usize, usize, T)> = Vec::new();
    for (i, &s) in surfaces.iter().enumerate() {
        match out.last_mut() {
            Some(last) if last.2 == s => last.1 = i + 1,
            _ => out.push((i, i + 1, s)),
        }
    }
    out
}
