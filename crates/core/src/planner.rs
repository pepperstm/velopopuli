//! Planning a route on the map: the sections between waypoints as BRouter returns them,
//! joined into one line, with what each stretch is (its surface and the kind of way), a
//! breakdown of the whole route, an estimate of how long it takes to ride, and loops.
//!
//! Fetching is the app's job (`src-tauri/src/geo.rs`); everything here works on the replies.

use serde::Serialize;

use crate::gpx::{haversine, RawPoint, Route};
use crate::physics::{step_speed, PhysicsParams};
use crate::world::Surface;

/// The routing styles offered, as BRouter profile names. The first is the default.
pub const STYLES: [&str; 6] = ["fastbike", "fastbike-verylowtraffic", "gravel", "trekking", "safety", "mtb"];

/// The style to route with: `style` when it's one of [`STYLES`], otherwise the default.
pub fn style(style: &str) -> &'static str {
    STYLES.iter().find(|s| **s == style).copied().unwrap_or(STYLES[0])
}

/// One routed section, or several joined.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Leg {
    /// `[lat, lon]` and the height when the router knew it.
    pub points: Vec<([f64; 2], Option<f64>)>,
    /// Stretches of the same way, in order: each runs from the previous one's `to` (or the
    /// first point) to the point at index `to`, with that way's OpenStreetMap tags.
    pub stretches: Vec<Stretch>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stretch {
    pub to: usize,
    pub tags: String,
}

impl Leg {
    /// Length in metres.
    pub fn length(&self) -> f64 {
        self.points.windows(2).map(|w| haversine(w[0].0[0], w[0].0[1], w[1].0[0], w[1].0[1])).sum()
    }

    /// The part from point `from` to point `to`, both included.
    pub fn slice(&self, from: usize, to: usize) -> Leg {
        let to = to.min(self.points.len().saturating_sub(1));
        let mut stretches = Vec::new();
        for s in &self.stretches {
            if s.to <= from {
                continue;
            }
            stretches.push(Stretch { to: s.to.min(to) - from, tags: s.tags.clone() });
            if s.to >= to {
                break;
            }
        }
        Leg { points: self.points[from..=to].to_vec(), stretches }
    }

    pub fn raw_points(&self) -> Vec<RawPoint> {
        self.points.iter().map(|&([lat, lon], ele)| RawPoint { lat, lon, ele }).collect()
    }
}

/// Reads one BRouter reply in `format=geojson`: the line, and the tags of each way along it
/// (the `messages` table, one row where each way ends).
pub fn parse_brouter(json: &str) -> Result<Leg, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|_| "The route planner sent something unexpected.".to_string())?;
    let feature = &v["features"][0];
    let points: Vec<([f64; 2], Option<f64>)> = feature["geometry"]["coordinates"]
        .as_array()
        .ok_or("The route planner found no route.")?
        .iter()
        .filter_map(|c| Some(([c[1].as_f64()?, c[0].as_f64()?], c.get(2).and_then(serde_json::Value::as_f64))))
        .collect();
    if points.len() < 2 {
        return Err("The route planner found no route.".into());
    }
    let mut stretches = Vec::new();
    if let Some(rows) = feature["properties"]["messages"].as_array() {
        let header: Vec<&str> = rows.first().and_then(|h| h.as_array()).map(|h| h.iter().filter_map(|c| c.as_str()).collect()).unwrap_or_default();
        let col = |name: &str| header.iter().position(|h| *h == name);
        if let (Some(lon), Some(lat), Some(tags)) = (col("Longitude"), col("Latitude"), col("WayTags")) {
            // Rows give their end point in millionths of a degree; find it along the line.
            let key = |p: [f64; 2]| ((p[0] * 1e6).round() as i64, (p[1] * 1e6).round() as i64);
            let mut at = 0;
            for row in rows.iter().skip(1).filter_map(|r| r.as_array()) {
                let cell = |i: usize| row.get(i).and_then(|c| c.as_str()).unwrap_or("");
                let (Ok(x), Ok(y)) = (cell(lon).parse::<i64>(), cell(lat).parse::<i64>()) else { continue };
                if let Some(k) = (at..points.len()).find(|&k| key(points[k].0) == (y, x)) {
                    if k > at {
                        stretches.push(Stretch { to: k, tags: cell(tags).to_string() });
                        at = k;
                    }
                }
            }
        }
    }
    // Whatever the table didn't reach (or a reply without one) is a stretch of unknown way.
    let last = points.len() - 1;
    if stretches.last().is_none_or(|s| s.to < last) {
        stretches.push(Stretch { to: last, tags: String::new() });
    }
    Ok(Leg { points, stretches })
}

/// The sections end to end, the shared point where they meet kept once.
pub fn join(legs: &[Leg]) -> Leg {
    let mut out = Leg::default();
    for leg in legs {
        let offset = out.points.len().saturating_sub(1);
        let skip = usize::from(!out.points.is_empty());
        out.points.extend(leg.points.iter().skip(skip).copied());
        for (k, s) in leg.stretches.iter().filter(|s| s.to > 0).enumerate() {
            match out.stretches.last_mut() {
                // The same way carrying on across the join is one stretch, as it was before cutting.
                Some(last) if k == 0 && last.tags == s.tags => last.to = s.to + offset,
                _ => out.stretches.push(Stretch { to: s.to + offset, tags: s.tags.clone() }),
            }
        }
    }
    out
}

/// What a stretch's surface is, as far as the map says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceClass {
    Paved,
    Cobbles,
    Gravel,
    Dirt,
    Unknown,
}

/// What kind of way a stretch is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum WayClass {
    MainRoad,
    MinorRoad,
    CyclePath,
    Track,
    Path,
}

fn tag<'a>(tags: &'a str, key: &str) -> Option<&'a str> {
    tags.split(' ').find_map(|kv| kv.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v))
}

/// The surface and the kind of way, from a stretch's tags. A road with no surface mapped is
/// taken to be paved, as the 3D world does; a track or path with none is unknown. A track's
/// grade stands in for its surface when that's missing.
pub fn classify(tags: &str) -> (SurfaceClass, WayClass) {
    let highway = tag(tags, "highway").unwrap_or("");
    let road = highway.trim_end_matches("_link");
    let way = match road {
        "motorway" | "trunk" | "primary" | "secondary" => WayClass::MainRoad,
        "tertiary" | "unclassified" | "residential" | "living_street" | "service" | "road" => WayClass::MinorRoad,
        "cycleway" => WayClass::CyclePath,
        "path" | "footway" | "bridleway" if tag(tags, "bicycle") == Some("designated") => WayClass::CyclePath,
        "track" => WayClass::Track,
        _ => WayClass::Path,
    };
    let tracktype = || match tag(tags, "tracktype") {
        Some("grade1") => SurfaceClass::Paved,
        Some("grade2" | "grade3") => SurfaceClass::Gravel,
        Some("grade4" | "grade5") => SurfaceClass::Dirt,
        _ => SurfaceClass::Unknown,
    };
    let surface = match tag(tags, "surface").and_then(Surface::from_osm) {
        Some(Surface::Asphalt | Surface::Concrete) => SurfaceClass::Paved,
        Some(Surface::Cobbles) => SurfaceClass::Cobbles,
        Some(Surface::Gravel) => SurfaceClass::Gravel,
        Some(Surface::Dirt) => SurfaceClass::Dirt,
        None if matches!(way, WayClass::MainRoad | WayClass::MinorRoad) => SurfaceClass::Paved,
        None if way == WayClass::Track => tracktype(),
        None => SurfaceClass::Unknown,
    };
    (surface, way)
}

impl SurfaceClass {
    /// How much harder it rolls than tarmac, as the ride will feel it (unknown counts as tarmac).
    pub fn rolling_factor(self) -> f64 {
        match self {
            SurfaceClass::Paved | SurfaceClass::Unknown => Surface::Asphalt.rolling_factor(),
            SurfaceClass::Cobbles => Surface::Cobbles.rolling_factor(),
            SurfaceClass::Gravel => Surface::Gravel.rolling_factor(),
            SurfaceClass::Dirt => Surface::Dirt.rolling_factor(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Share<T> {
    pub class: T,
    pub metres: f64,
}

/// A run of the same surface and kind of way, for drawing on the map.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Run {
    pub surface: SurfaceClass,
    pub way: WayClass,
    /// Where it starts and ends along the route, in metres.
    pub from: f64,
    pub to: f64,
    /// `[lat, lon]`, thinned.
    pub line: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Breakdown {
    /// Each surface and each kind of way with how much of the route it is, largest first.
    pub surfaces: Vec<Share<SurfaceClass>>,
    pub ways: Vec<Share<WayClass>>,
    pub runs: Vec<Run>,
}

/// How much of the route is each surface and each kind of way, and where. Lines are thinned
/// to about `max_points` points in all.
pub fn breakdown(leg: &Leg, max_points: usize) -> Breakdown {
    let n = leg.points.len();
    if n < 2 {
        return Breakdown::default();
    }
    let mut along = vec![0.0; n];
    for i in 1..n {
        along[i] = along[i - 1] + haversine(leg.points[i - 1].0[0], leg.points[i - 1].0[1], leg.points[i].0[0], leg.points[i].0[1]);
    }
    let stride = (n / max_points.max(2)).max(1);
    let mut runs: Vec<Run> = Vec::new();
    let mut start = 0;
    for s in &leg.stretches {
        let end = s.to.min(n - 1);
        if end <= start {
            continue;
        }
        let (surface, way) = classify(&s.tags);
        let points = |from: usize| (from..=end).filter(move |&i| i == from || i == end || (i - from).is_multiple_of(stride));
        match runs.last_mut() {
            Some(r) if r.surface == surface && r.way == way => {
                r.to = along[end];
                r.line.extend(points(start).skip(1).map(|i| leg.points[i].0));
            }
            _ => runs.push(Run { surface, way, from: along[start], to: along[end], line: points(start).map(|i| leg.points[i].0).collect() }),
        }
        start = end;
    }
    let mut surfaces: Vec<Share<SurfaceClass>> = Vec::new();
    let mut ways: Vec<Share<WayClass>> = Vec::new();
    for r in &runs {
        let metres = r.to - r.from;
        match surfaces.iter_mut().find(|s| s.class == r.surface) {
            Some(s) => s.metres += metres,
            None => surfaces.push(Share { class: r.surface, metres }),
        }
        match ways.iter_mut().find(|s| s.class == r.way) {
            Some(s) => s.metres += metres,
            None => ways.push(Share { class: r.way, metres }),
        }
    }
    surfaces.sort_by(|a, b| b.metres.total_cmp(&a.metres));
    ways.sort_by(|a, b| b.metres.total_cmp(&a.metres));
    Breakdown { surfaces, ways, runs }
}

/// The joined line thinned to about `max_points`, always keeping the points where sections
/// meet; and, for each of those, its index in the thinned line (so a click on the line can be
/// put in the right section).
pub fn outline(leg: &Leg, section_ends: &[usize], max_points: usize) -> (Vec<[f64; 2]>, Vec<usize>) {
    let n = leg.points.len();
    let stride = (n / max_points.max(2)).max(1);
    let mut line = Vec::new();
    let mut ends = Vec::new();
    for i in 0..n {
        let end = section_ends.contains(&i);
        if end || i % stride == 0 || i == n - 1 {
            if end {
                ends.push(line.len());
            }
            line.push(leg.points[i].0);
        }
    }
    (line, ends)
}

/// How long the route takes at a steady `power`, with the ride's own physics: the route's
/// gradients and heights, and the rolling resistance of each stretch's surface (`rolling`
/// gives the factor at a distance along the route). Seconds.
pub fn ride_time(route: &Route, power: f64, params: &PhysicsParams, rolling: impl Fn(f64) -> f64) -> f64 {
    const DT: f64 = 0.5;
    // Up a wall at low power the model would crawl for ever; nobody rides slower than this.
    const SLOWEST: f64 = 1.2;
    let (mut v, mut d, mut t) = (0.0f64, 0.0f64, 0.0f64);
    while d < route.total && t < 7.0 * 86_400.0 {
        let pos = route.sample_at(d);
        let here = PhysicsParams { crr: params.crr * rolling(d), ..*params };
        v = step_speed(v, power, pos.grade, pos.ele, &here, DT).max(SLOWEST);
        d += v * DT;
        t += DT;
    }
    t
}

/// The rolling factor along a joined line's stretches, as a lookup by distance along the
/// route (scaled to the route's own length, which differs a little after resampling).
pub fn rolling_lookup(breakdown: &Breakdown, route_total: f64) -> impl Fn(f64) -> f64 + '_ {
    let line_total = breakdown.runs.last().map_or(1.0, |r| r.to).max(1.0);
    let scale = line_total / route_total.max(1.0);
    move |d: f64| {
        let at = d * scale;
        breakdown.runs.iter().find(|r| at <= r.to).map_or(1.0, |r| r.surface.rolling_factor())
    }
}

/// BRouter makes a loop through points about this far from the start; the loop it rides is
/// roughly this many times as long.
const LOOP_PER_RADIUS: f64 = 6.4;

/// The radius to ask BRouter for, for a loop of about `distance` metres.
pub fn round_trip_radius(distance: f64) -> f64 {
    (distance / LOOP_PER_RADIUS).clamp(500.0, 60_000.0)
}

/// Waypoints along a loop so it can be edited like any planned route: about one every
/// `spacing` metres (between 4 and 12 of them), the start again at the end. Each is given
/// with its index in the loop's points, so the sections between them can be reused as they are.
pub fn loop_waypoints(leg: &Leg, spacing: f64) -> Vec<(usize, [f64; 2])> {
    let n = leg.points.len();
    if n < 2 {
        return Vec::new();
    }
    let total = leg.length();
    let count = ((total / spacing.max(1.0)).round() as usize).clamp(4, 12);
    let mut out = vec![(0, leg.points[0].0)];
    let mut along = 0.0;
    let mut next = 1;
    for i in 1..n {
        along += haversine(leg.points[i - 1].0[0], leg.points[i - 1].0[1], leg.points[i].0[0], leg.points[i].0[1]);
        if next < count && along >= total * next as f64 / count as f64 && i < n - 1 {
            out.push((i, leg.points[i].0));
            next += 1;
        }
    }
    out.push((n - 1, leg.points[n - 1].0));
    out
}
