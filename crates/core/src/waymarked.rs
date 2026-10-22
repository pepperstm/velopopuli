//! Waymarked mountain-bike routes from OpenStreetMap: `route=mtb` relations, listed for an
//! area and, once one is picked, its ways joined into one line to ride.
//!
//! A relation's ways are often in order, but not always, and each may point either way. They
//! are joined greedily: from a loose end, always on to the nearest remaining way, turned round
//! if need be. A jump between ways that don't meet is counted as a gap, and ridden straight.

use serde::{Deserialize, Serialize};

use crate::gpx::M_PER_DEG;

/// One route in an area, for a list to pick from.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaymarkedSummary {
    pub id: u64,
    pub name: String,
    /// The route's number or code on the signs (`ref`), if any.
    pub reference: String,
    /// "lcn", "rcn", "ncn" (local, regional, national) or "", from `network`.
    pub network: String,
    /// Length in metres as mapped (`distance` in km), if mapped.
    pub distance: Option<f64>,
    /// Bounds as `[south, west, north, east]`, to show where it is.
    pub bounds: Option<[f64; 4]>,
}

/// A picked route as one line of `[lat, lon]` points.
#[derive(Debug, Clone, PartialEq)]
pub struct WaymarkedLine {
    pub name: String,
    pub line: Vec<[f64; 2]>,
    /// Places where its ways don't meet and the line jumps straight across.
    pub gaps: usize,
}

/// Ways closer than this at their ends are taken to meet.
const MEET_M: f64 = 20.0;
/// The most routes listed for one area.
pub const MAX_LISTED: usize = 60;

/// Overpass query for the mountain-bike routes within `[south, west, north, east]`.
pub fn area_query(bounds: [f64; 4]) -> String {
    let [s, w, n, e] = bounds;
    format!("[out:json][timeout:25];relation[\"route\"=\"mtb\"]({s:.5},{w:.5},{n:.5},{e:.5});out tags bb;")
}

/// Overpass query for one route relation with the geometry of its ways.
pub fn route_query(id: u64) -> String {
    format!("[out:json][timeout:25];relation({id});out geom;")
}

#[derive(Deserialize)]
struct Reply {
    elements: Vec<Element>,
}

#[derive(Deserialize)]
struct Element {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    tags: std::collections::HashMap<String, String>,
    #[serde(default)]
    bounds: Option<Bounds>,
    #[serde(default)]
    members: Vec<Member>,
}

#[derive(Deserialize)]
struct Bounds {
    minlat: f64,
    minlon: f64,
    maxlat: f64,
    maxlon: f64,
}

#[derive(Deserialize)]
struct Member {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    geometry: Vec<Option<Point>>,
}

#[derive(Deserialize)]
struct Point {
    lat: f64,
    lon: f64,
}

fn reply(json: &str) -> Result<Reply, String> {
    serde_json::from_str(json).map_err(|e| format!("Unexpected reply from the OpenStreetMap server: {e}"))
}

fn name_of(tags: &std::collections::HashMap<String, String>) -> String {
    let get = |k: &str| tags.get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    get("name").or_else(|| get("ref").map(|r| format!("MTB route {r}"))).unwrap_or_else(|| "Mountain-bike route".into())
}

/// The routes in an Overpass reply to `area_query`, longest first: the mapped distance, or else
/// the size of the area it covers (an area can hold hundreds of short named trail sections).
pub fn parse_area(json: &str) -> Result<Vec<WaymarkedSummary>, String> {
    let mut out: Vec<WaymarkedSummary> = reply(json)?
        .elements
        .into_iter()
        .map(|e| WaymarkedSummary {
            id: e.id,
            name: name_of(&e.tags),
            reference: e.tags.get("ref").cloned().unwrap_or_default(),
            network: e.tags.get("network").cloned().unwrap_or_default(),
            distance: e.tags.get("distance").and_then(|d| d.trim().trim_end_matches("km").trim().replace(',', ".").parse::<f64>().ok()).map(|km| km * 1000.0),
            bounds: e.bounds.map(|b| [b.minlat, b.minlon, b.maxlat, b.maxlon]),
        })
        .collect();
    let size = |r: &WaymarkedSummary| r.distance.unwrap_or_else(|| r.bounds.map_or(0.0, |[s, w, n, e]| metres([s, w], [n, e])));
    out.sort_by(|a, b| size(b).total_cmp(&size(a)).then_with(|| a.name.cmp(&b.name)));
    out.truncate(MAX_LISTED);
    Ok(out)
}

/// The route in an Overpass reply to `route_query`, its ways joined into one line.
pub fn parse_route(json: &str) -> Result<WaymarkedLine, String> {
    let relation = reply(json)?.elements.into_iter().next().ok_or("That route is no longer on the map.")?;
    let ways: Vec<Vec<[f64; 2]>> = relation
        .members
        .iter()
        // The main line only: not the return half of a one-way pair, side trips or link roads.
        .filter(|m| m.kind == "way" && matches!(m.role.as_str(), "" | "forward" | "main" | "route"))
        .map(|m| m.geometry.iter().flatten().map(|p| [p.lat, p.lon]).collect::<Vec<_>>())
        .filter(|g| g.len() >= 2)
        .collect();
    if ways.is_empty() {
        return Err("This route has no ways mapped to ride.".into());
    }
    let (line, gaps) = join(ways);
    Ok(WaymarkedLine { name: name_of(&relation.tags), line, gaps })
}

/// Distance in metres between two `[lat, lon]` points (flat-earth, fine at these scales).
fn metres(a: [f64; 2], b: [f64; 2]) -> f64 {
    let cos = ((a[0] + b[0]) / 2.0).to_radians().cos();
    ((b[0] - a[0]) * M_PER_DEG).hypot((b[1] - a[1]) * M_PER_DEG * cos)
}

/// Joins ways into one line, and counts the places where they don't meet.
pub fn join(mut ways: Vec<Vec<[f64; 2]>>) -> (Vec<[f64; 2]>, usize) {
    if ways.is_empty() {
        return (Vec::new(), 0);
    }
    // Start from a loose end — one that meets no other way — so a linear route is ridden end to end.
    let meets_another = |k: usize, p: [f64; 2], ways: &[Vec<[f64; 2]>]| ways.iter().enumerate().any(|(j, w)| j != k && (metres(p, w[0]) < MEET_M || metres(p, w[w.len() - 1]) < MEET_M));
    let start = (0..ways.len()).find_map(|k| {
        let (first, last) = (ways[k][0], ways[k][ways[k].len() - 1]);
        if !meets_another(k, first, &ways) {
            Some((k, false))
        } else if !meets_another(k, last, &ways) {
            Some((k, true))
        } else {
            None
        }
    });
    let (k, reverse) = start.unwrap_or((0, false));
    let mut line = ways.swap_remove(k);
    if reverse {
        line.reverse();
    }
    let mut gaps = 0;
    while !ways.is_empty() {
        let end = line[line.len() - 1];
        // The nearest end of any remaining way.
        let (k, flip, d) =
            ways.iter().enumerate().flat_map(|(k, w)| [(k, false, metres(end, w[0])), (k, true, metres(end, w[w.len() - 1]))]).min_by(|a, b| a.2.total_cmp(&b.2)).expect("ways is not empty");
        let mut next = ways.swap_remove(k);
        if flip {
            next.reverse();
        }
        if d < MEET_M {
            next.remove(0);
        } else {
            gaps += 1;
        }
        line.extend(next);
    }
    (line, gaps)
}

/// A GPX file for a line, with elevations where known, to save to the library.
pub fn to_gpx(name: &str, points: &[([f64; 2], Option<f64>)]) -> String {
    let escape = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let mut gpx =
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\" creator=\"VeloPopuli\" xmlns=\"http://www.topografix.com/GPX/1/1\"><trk><name>{}</name><trkseg>\n", escape(name));
    for ([lat, lon], ele) in points {
        match ele {
            Some(ele) => gpx.push_str(&format!("<trkpt lat=\"{lat:.6}\" lon=\"{lon:.6}\"><ele>{ele:.1}</ele></trkpt>\n")),
            None => gpx.push_str(&format!("<trkpt lat=\"{lat:.6}\" lon=\"{lon:.6}\"></trkpt>\n")),
        }
    }
    gpx.push_str("</trkseg></trk></gpx>\n");
    gpx
}
