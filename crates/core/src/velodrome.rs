//! Velodromes: real cycling tracks built into the app as routes, ridden in laps.
//!
//! A track is an oval of two straights and two semicircles round its measuring line, placed
//! at its real location. Its route is the measuring line itself, so a lap is the track's
//! official length; the world round it (the banked track, its markings, the stands) is drawn
//! by `world::venue`. Herne Hill's oval is the one in OpenBikeNet's `herne-hill-450` route
//! file (fitted to the track's outline in OpenStreetMap), so riders from other apps line up.

use crate::gpx::{build_route_with_step, RawPoint, Route, M_PER_DEG};

/// A track's shape and place.
#[derive(Debug, Clone, Copy)]
pub struct Track {
    /// Route ID for riding it anticlockwise, as track races go; `<id>-clockwise` the other way.
    pub id: &'static str,
    pub name: &'static str,
    /// One lap on the measuring line, in metres.
    pub length: f64,
    /// Radius of the bends on the measuring line.
    pub radius: f64,
    /// The middle of the oval, as (lat, lon).
    pub centre: (f64, f64),
    /// Direction of the long axis, in degrees anticlockwise from east.
    pub axis: f64,
    /// The start/finish line is this far from the middle of the home straight, against the
    /// direction of riding anticlockwise.
    pub start_offset: f64,
    /// The riding surface from inside edge to outside edge, côte d'azur included.
    pub width: f64,
    /// Banking at the steepest part of the bends, and along the straights, in degrees.
    pub bend_banking: f64,
    pub straight_banking: f64,
}

/// Herne Hill Velodrome, London: opened in 1891, the track events of the 1948 Olympics, 450 m.
/// Sources give its banking as anything from 18° to 30°; 26° (the 1992 rebuild) has the most behind it.
pub const HERNE_HILL: Track = Track {
    id: "herne-hill",
    name: "Herne Hill Velodrome",
    length: 450.0,
    radius: 45.3,
    centre: (51.45105, -0.09142),
    axis: -76.8,
    start_offset: 13.0,
    width: 7.4,
    bend_banking: 26.0,
    straight_banking: 7.0,
};

pub const TRACKS: [Track; 1] = [HERNE_HILL];

/// Points along the bends, about this far apart.
const BEND_STEP: f64 = 2.5;
/// The route is sampled this often, in metres (roads are every 5 m).
pub const ROUTE_STEP: f64 = 1.0;

impl Track {
    /// Length of each straight on the measuring line.
    pub fn straight(&self) -> f64 {
        (self.length - 2.0 * std::f64::consts::PI * self.radius) / 2.0
    }

    /// The route ID for riding it this way round.
    pub fn route_id(&self, clockwise: bool) -> String {
        if clockwise {
            format!("{}-clockwise", self.id)
        } else {
            self.id.to_string()
        }
    }

    pub fn route_name(&self, clockwise: bool) -> String {
        if clockwise {
            format!("{} (clockwise)", self.name)
        } else {
            self.name.to_string()
        }
    }

    /// The measuring line as `(a, b)` metres from the middle of the oval, `a` along the long
    /// axis and `b` across it, the home straight at `b = +radius`: from the start/finish line
    /// round anticlockwise and back to it.
    pub fn outline(&self) -> Vec<(f64, f64)> {
        let (r, half) = (self.radius, self.straight() / 2.0);
        let steps = (std::f64::consts::PI * r / BEND_STEP).round() as usize;
        let mut points = vec![(-self.start_offset, r), (-half, r)];
        for k in 1..steps {
            let t = std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * k as f64 / steps as f64;
            points.push((-half + r * t.cos(), r * t.sin()));
        }
        points.extend([(-half, -r), (half, -r)]);
        for k in 1..steps {
            let t = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * k as f64 / steps as f64;
            points.push((half + r * t.cos(), r * t.sin()));
        }
        points.push((half, r));
        points.push(points[0]);
        points
    }

    /// `(a, b)` metres in the oval's frame as (lat, lon), stretched by `scale`.
    fn lat_lon(&self, (a, b): (f64, f64), scale: f64) -> (f64, f64) {
        let (c, s) = (self.axis.to_radians().cos(), self.axis.to_radians().sin());
        let (east, north) = ((a * c - b * s) * scale, (a * s + b * c) * scale);
        (self.centre.0 + north / M_PER_DEG, self.centre.1 + east / (M_PER_DEG * self.centre.0.to_radians().cos()))
    }

    /// The route: the measuring line at its real place, flat, exactly `length` round (as the
    /// app measures distance), from the start/finish line and back. Sampled every metre, so the
    /// bends are smooth and rounding off corners takes nothing measurable off a lap.
    pub fn route(&self, clockwise: bool) -> Route {
        let outline = self.outline();
        let build = |scale: f64| {
            let mut points: Vec<RawPoint> = outline
                .iter()
                .map(|&q| {
                    let (lat, lon) = self.lat_lon(q, scale);
                    RawPoint { lat, lon, ele: Some(0.0) }
                })
                .collect();
            if clockwise {
                points.reverse();
            }
            build_route_with_step(&self.route_name(clockwise), &points, ROUTE_STEP).expect("a track is a valid route")
        };
        // Scaled until the route as built, smoothing and all, is the track's length.
        let first: f64 = outline.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum();
        let mut scale = self.length / first;
        let mut route = build(scale);
        for _ in 0..3 {
            scale *= self.length / route.total;
            route = build(scale);
        }
        route
    }
}

/// The track and which way round, for a built-in track's route ID.
pub fn track_for(route_id: &str) -> Option<(Track, bool)> {
    TRACKS.iter().find_map(|t| {
        if route_id == t.id {
            Some((*t, false))
        } else if route_id == t.route_id(true) {
            Some((*t, true))
        } else {
            None
        }
    })
}
