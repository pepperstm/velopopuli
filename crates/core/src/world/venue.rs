//! A velodrome round a built-in track's route: the banked track itself (widths, banking and
//! the look of the road ribbon, set on the world), its markings, the railing and floodlights
//! round it, and the grounds laid level with the stands and buildings on them.
//!
//! Positions are worked out in the track's own frame: `a` metres along its long axis from the
//! middle of the oval and `b` across it, the home straight at `b = +radius`. The riding surface
//! is centred on the measuring line (the route), so it lies a few metres inside the real
//! track's footprint; the buildings are placed by their distance from the drawn outside edge,
//! so what stands beside the track stands as close to it as it really does.

use super::height::smoothstep;
use super::mesh::TriMesh;
use super::scenery::to_segment;
use super::{World, ROAD_LIFT};
use crate::gpx::Route;
use crate::velodrome::Track;

type P3 = [f64; 3];

fn rgb(c: [f64; 3]) -> [f32; 3] {
    c.map(|v| v.powf(2.2) as f32)
}

// Colours, authored in sRGB.
const GALVANISED: [f64; 3] = [0.72, 0.74, 0.75];
const KERB: [f64; 3] = [0.66, 0.65, 0.62];
const LINE_WHITE: [f64; 3] = [0.93, 0.93, 0.91];
const LINE_BLACK: [f64; 3] = [0.06, 0.06, 0.07];
const LINE_RED: [f64; 3] = [0.80, 0.16, 0.16];
const LAMP_HEAD: [f64; 3] = [0.24, 0.25, 0.27];
const PAVING: [f64; 3] = [0.60, 0.59, 0.56];
const ASPHALT: [f64; 3] = [0.34, 0.34, 0.36];
const JUNIOR: [f64; 3] = [0.40, 0.41, 0.44];
const BRICK: [f64; 3] = [0.60, 0.28, 0.21];
const CONCRETE: [f64; 3] = [0.72, 0.71, 0.68];
const COPING: [f64; 3] = [0.50, 0.52, 0.55];
const TIMBER: [f64; 3] = [0.76, 0.55, 0.33];
const TIMBER_DARK: [f64; 3] = [0.70, 0.50, 0.30];
const SOFFIT: [f64; 3] = [0.86, 0.70, 0.48];
const ROOF: [f64; 3] = [0.56, 0.58, 0.61];
const FASCIA: [f64; 3] = [0.62, 0.64, 0.67];
const GLASS: [f64; 3] = [0.17, 0.22, 0.27];
const MULLION: [f64; 3] = [0.13, 0.13, 0.14];
const COLUMN: [f64; 3] = [0.16, 0.18, 0.20];
const DOOR: [f64; 3] = [0.42, 0.47, 0.50];
const STAND: [f64; 3] = [0.45, 0.47, 0.50];
const SEAT: [f64; 3] = [0.07, 0.07, 0.08];
const MEMBRANE: [f64; 3] = [0.95, 0.95, 0.93];
const SHELTER_ROOF: [f64; 3] = [0.92, 0.92, 0.90];
/// The pavilion's track emblem, outside stripe first: purple, light blue, white and red.
const EMBLEM: [[f64; 3]; 4] = [[0.33, 0.27, 0.58], [0.40, 0.70, 0.93], [0.96, 0.96, 0.96], [0.85, 0.25, 0.42]];
/// Plain advertising boards on the railing, as runs of colour.
const BOARDS: [[f64; 3]; 8] = [[0.12, 0.66, 0.63], [0.95, 0.95, 0.93], [0.16, 0.56, 0.35], [0.08, 0.08, 0.09], [0.91, 0.78, 0.29], [0.78, 0.20, 0.23], [0.17, 0.35, 0.63], [0.90, 0.90, 0.90]];
const CONTAINERS: [[f64; 3]; 8] = [[0.55, 0.22, 0.16], [0.18, 0.33, 0.55], [0.22, 0.42, 0.28], [0.80, 0.45, 0.15], [0.85, 0.82, 0.72], [0.45, 0.47, 0.49], [0.15, 0.45, 0.47], [0.45, 0.16, 0.20]];

/// The railing stands this far outside the riding surface; its posts this far apart.
const RAIL_OUT: f64 = 0.15;
const RAIL_EVERY: f64 = 2.5;
/// Floodlights: how far apart round the track, how far outside it, how tall above its edge.
const LIGHT_EVERY: f64 = 15.0;
const LIGHT_OUT: f64 = 1.1;
const LIGHT_HEIGHT: f64 = 8.5;
/// Herne Hill's outside edge on the ground, metres from the middle of the oval across the home
/// straight (from OpenStreetMap). The buildings are placed by their distance from it.
const REAL_OUTSIDE: f64 = 53.0;

/// A capsule: a segment's ends and the distance round it, in local metres.
type Zone = ((f64, f64), (f64, f64), f64);

/// Ground laid level: capsules (a segment and the distance round it, in local metres).
#[derive(Clone)]
pub struct Grounds {
    zones: Vec<Zone>,
    pub height: f64,
    blend: f64,
}

impl Grounds {
    /// How far `(x, z)` is outside the grounds, in metres (negative inside).
    pub fn outside(&self, x: f64, z: f64) -> f64 {
        self.zones.iter().map(|&(a, b, r)| to_segment((x, z), a, b) - r).fold(f64::INFINITY, f64::min)
    }

    /// 1 on the grounds, falling to 0 over the blend distance beyond them.
    pub fn levelled(&self, x: f64, z: f64) -> f64 {
        1.0 - smoothstep(0.0, self.blend, self.outside(x, z))
    }
}

pub struct Venue {
    pub track: Track,
    pub clockwise: bool,
    /// The middle of the oval in local metres, and unit vectors along and across it.
    centre: (f64, f64),
    along: (f64, f64),
    across: (f64, f64),
    /// The height of the grounds (and the track: it is flat).
    pub height: f64,
    pub grounds: Grounds,
}

impl Venue {
    pub fn new(track: Track, clockwise: bool, route: &Route) -> Self {
        let centre = route.to_local(track.centre.0, track.centre.1);
        let (c, s) = (track.axis.to_radians().cos(), track.axis.to_radians().sin());
        // East is +x and north is -z locally.
        let (along, across) = ((c, -s), (-s, -c));
        let height = route.ele.first().copied().unwrap_or(0.0) as f64;
        let mut venue = Venue { track, clockwise, centre, along, across, height, grounds: Grounds { zones: Vec::new(), height, blend: 30.0 } };
        let (half, edge) = (track.straight() / 2.0, venue.outside_edge());
        venue.grounds.zones = vec![
            // The track and its infield, with a margin round the outside.
            (venue.at(-half, 0.0), venue.at(half, 0.0), edge + 16.0),
            // The pavilion, stands and stores beside the home straight.
            (venue.at(-40.0, edge + 11.0), venue.at(24.0, edge + 11.0), 18.0),
        ];
        venue
    }

    /// Local `(x, z)` of a point in the track's frame.
    pub fn at(&self, a: f64, b: f64) -> (f64, f64) {
        (self.centre.0 + a * self.along.0 + b * self.across.0, self.centre.1 + a * self.along.1 + b * self.across.1)
    }

    /// A point in the track's frame, `y` metres above the grounds.
    fn p(&self, a: f64, b: f64, y: f64) -> P3 {
        let (x, z) = self.at(a, b);
        [x, self.height + y, z]
    }

    /// `(a, b)` of a local point.
    pub fn frame(&self, x: f64, z: f64) -> (f64, f64) {
        let (dx, dz) = (x - self.centre.0, z - self.centre.1);
        (dx * self.along.0 + dz * self.along.1, dx * self.across.0 + dz * self.across.1)
    }

    /// The side the inside of the track is on, as the sign of a sideways offset from the route.
    pub fn inside(&self) -> f64 {
        if self.clockwise {
            -1.0
        } else {
            1.0
        }
    }

    fn half_width(&self) -> f64 {
        self.track.width / 2.0
    }

    /// `b` of the drawn track's outside edge along the straights.
    fn outside_edge(&self) -> f64 {
        self.track.radius + self.half_width()
    }

    /// `b` in the drawn frame of something `real` metres across the home straight on the ground.
    fn beside(&self, real: f64) -> f64 {
        real - REAL_OUTSIDE + self.outside_edge()
    }

    /// The cross-slope at every route sample: the straights' banking along them, rising
    /// through the start of each bend to the bends' and easing off again, the outside high.
    pub fn banks(&self, route: &Route) -> Vec<f64> {
        let half = self.track.straight() / 2.0;
        (0..route.n)
            .map(|i| {
                let (a, _) = self.frame(route.x[i], route.z[i]);
                let share = smoothstep(-8.0, 14.0, a.abs() - half);
                let degrees = self.track.straight_banking + (self.track.bend_banking - self.track.straight_banking) * share;
                -self.inside() * degrees.to_radians().tan()
            })
            .collect()
    }

    /// Route distance of the spot `physical` metres round from the line the way races go.
    fn along_route(&self, total: f64, physical: f64) -> f64 {
        if self.clockwise {
            (total - physical).rem_euclid(total)
        } else {
            physical.rem_euclid(total)
        }
    }
}

// ── Mesh helpers ───────────────────────────────────────────────────────────

fn quad(m: &mut TriMesh, p: [P3; 4], facing: P3, c: [f64; 3]) {
    let k = rgb(c);
    m.quad(p, facing, [k; 4]);
}

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: P3, k: f64) -> P3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: P3) -> P3 {
    let len = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-9);
    scale(a, 1.0 / len)
}

/// A bar from `a` to `b`, `w` wide and `h` deep, with its ends closed.
fn beam(m: &mut TriMesh, a: P3, b: P3, w: f64, h: f64, c: [f64; 3]) {
    let d = unit(sub(b, a));
    let side = cross(d, [0.0, 1.0, 0.0]);
    let side = if side.iter().map(|v| v * v).sum::<f64>() < 1e-6 { [1.0, 0.0, 0.0] } else { unit(side) };
    let up = unit(cross(side, d));
    let (s, u) = (scale(side, w / 2.0), scale(up, h / 2.0));
    let corner = |p: P3, i: f64, j: f64| add(add(p, scale(s, i)), scale(u, j));
    let ring = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
    for k in 0..4 {
        let (p, q) = (ring[k], ring[(k + 1) % 4]);
        let facing = add(scale(s, p.0 + q.0), scale(u, p.1 + q.1));
        quad(m, [corner(a, p.0, p.1), corner(b, p.0, p.1), corner(b, q.0, q.1), corner(a, q.0, q.1)], facing, c);
    }
    quad(m, ring.map(|(i, j)| corner(a, i, j)), scale(d, -1.0), c);
    quad(m, ring.map(|(i, j)| corner(b, i, j)), d, c);
}

/// An upright post `w` square from `base` up by `height`.
fn post(m: &mut TriMesh, base: P3, height: f64, w: f64, c: [f64; 3]) {
    beam(m, base, add(base, [0.0, height, 0.0]), w, w, c);
}

impl Venue {
    /// A box in the track's frame, from `a0` to `a1`, `b0` to `b1` and `y0` to `y1`: its sides
    /// in one colour and its top in another.
    #[allow(clippy::too_many_arguments)]
    fn block(&self, m: &mut TriMesh, (a0, a1): (f64, f64), (b0, b1): (f64, f64), (y0, y1): (f64, f64), sides: [f64; 3], top: [f64; 3]) {
        let ring = [(a0, b0), (a1, b0), (a1, b1), (a0, b1)];
        let (ca, cb) = ((a0 + a1) / 2.0, (b0 + b1) / 2.0);
        for k in 0..4 {
            let (p, q) = (ring[k], ring[(k + 1) % 4]);
            let out = self.dir((p.0 + q.0) / 2.0 - ca, (p.1 + q.1) / 2.0 - cb);
            quad(m, [self.p(p.0, p.1, y0), self.p(q.0, q.1, y0), self.p(q.0, q.1, y1), self.p(p.0, p.1, y1)], out, sides);
        }
        quad(m, ring.map(|(a, b)| self.p(a, b, y1)), [0.0, 1.0, 0.0], top);
    }

    /// A horizontal direction in the track's frame as a local vector.
    fn dir(&self, da: f64, db: f64) -> P3 {
        [da * self.along.0 + db * self.across.0, 0.0, da * self.along.1 + db * self.across.1]
    }

    /// A flat patch of ground cover in the track's frame, `y` above the grounds.
    fn patch(&self, m: &mut TriMesh, (a0, a1): (f64, f64), (b0, b1): (f64, f64), y: f64, c: [f64; 3]) {
        quad(m, [self.p(a0, b0, y), self.p(a1, b0, y), self.p(a1, b1, y), self.p(a0, b1, y)], [0.0, 1.0, 0.0], c);
    }
}

// ── The track ──────────────────────────────────────────────────────────────

impl World {
    /// A point on the track's surface (or its extension sideways) `d` metres along the route and
    /// `offset` across, `up` above it.
    fn on_track(&self, d: f64, offset: f64, up: f64) -> P3 {
        let r = &self.route;
        let d = d.rem_euclid(r.total);
        let p = r.sample_at(d);
        let (q0, q1) = (r.sample_at((d - 0.5).max(0.0)), r.sample_at((d + 0.5).min(r.total)));
        let (tx, tz) = (q1.x - q0.x, q1.z - q0.z);
        let len = tx.hypot(tz).max(1e-9);
        let (tx, tz) = (tx / len, tz / len);
        [p.x + tz * offset, p.ele + ROAD_LIFT + self.bank_at(d, offset).1 + up, p.z - tx * offset]
    }

    /// The direction across the track at `d`, toward +offset, as a local vector.
    fn across_track(&self, d: f64) -> P3 {
        let (a, b) = (self.on_track(d, 0.0, 0.0), self.on_track(d, 1.0, 0.0));
        unit([b[0] - a[0], 0.0, b[2] - a[2]])
    }
}

impl Venue {
    /// Everything round the track, for `World::structures`.
    pub fn dress(&self, world: &World, m: &mut TriMesh) {
        self.markings(world, m);
        self.railing(world, m);
        self.floodlights(world, m);
        if self.track.id == crate::velodrome::HERNE_HILL.id {
            self.herne_hill(m);
        }
    }

    /// A band painted across the track at `physical` metres round from the line, `long` along
    /// it, from `from` to `to` metres across measured from the inside edge.
    #[allow(clippy::too_many_arguments)]
    fn band(&self, world: &World, m: &mut TriMesh, physical: f64, long: f64, (from, to): (f64, f64), lift: f64, c: [f64; 3]) {
        let d = self.along_route(world.route.total, physical);
        let (inside, half) = (self.inside(), self.half_width());
        let (o0, o1) = (inside * (half - from), inside * (half - to));
        let p = [world.on_track(d - long / 2.0, o0, lift), world.on_track(d + long / 2.0, o0, lift), world.on_track(d + long / 2.0, o1, lift), world.on_track(d - long / 2.0, o1, lift)];
        quad(m, p, [0.0, 1.0, 0.0], c);
    }

    /// The lines painted across the track: the finish line (white with a black line down the
    /// middle), the 200 m line, and the red pursuit lines half-way along each straight.
    fn markings(&self, world: &World, m: &mut TriMesh) {
        let (total, width) = (world.route.total, self.track.width);
        self.band(world, m, 0.0, 0.72, (0.0, width), 0.012, LINE_WHITE);
        self.band(world, m, 0.0, 0.05, (0.0, width), 0.018, LINE_BLACK);
        self.band(world, m, total - 200.0, 0.05, (0.0, width), 0.012, LINE_WHITE);
        let home = total - self.track.start_offset;
        for at in [home, home - total / 2.0] {
            self.band(world, m, at, 0.05, (0.0, width / 2.0), 0.012, LINE_RED);
        }
    }

    /// A low kerb along the outside edge with a galvanised railing on it, and advertising
    /// boards hung along the straights.
    fn railing(&self, world: &World, m: &mut TriMesh) {
        let total = world.route.total;
        let out = -self.inside() * (self.half_width() + RAIL_OUT);
        let n = (total / RAIL_EVERY).round() as usize;
        let every = total / n as f64;
        let half = self.track.straight() / 2.0;
        for k in 0..n {
            let (d0, d1) = (k as f64 * every, (k + 1) as f64 * every);
            let (b0, b1) = (world.on_track(d0, out, 0.0), world.on_track(d1, out, 0.0));
            beam(m, add(b0, [0.0, 0.08, 0.0]), add(b1, [0.0, 0.08, 0.0]), 0.25, 0.3, KERB);
            post(m, add(b0, [0.0, 0.2, 0.0]), 1.0, 0.05, GALVANISED);
            for y in [0.62, 1.15] {
                beam(m, add(b0, [0.0, y, 0.0]), add(b1, [0.0, y, 0.0]), 0.05, 0.05, GALVANISED);
            }
            // Boards along the straights only, facing the track.
            let (a, _) = self.frame(b0[0], b0[2]);
            if a.abs() < half - 6.0 {
                let board = BOARDS[(k / 2) % BOARDS.len()];
                let toward = scale(world.across_track(d0), self.inside());
                let face = |d: f64, y: f64| add(world.on_track(d, out, y), scale(toward, 0.04));
                let (e0, e1) = (d0 + 0.1, d1 - 0.1);
                quad(m, [face(e0, 0.22), face(e1, 0.22), face(e1, 1.05), face(e0, 1.05)], toward, board);
                // A paler block in the middle, where the lettering would be.
                let lettering = if board.iter().sum::<f64>() > 2.0 { [0.25, 0.27, 0.30] } else { [0.93, 0.93, 0.90] };
                let (l0, l1) = (e0 + (e1 - e0) * 0.2, e1 - (e1 - e0) * 0.2);
                let lift = scale(toward, 0.05);
                quad(m, [add(face(l0, 0.5), lift), add(face(l1, 0.5), lift), add(face(l1, 0.78), lift), add(face(l0, 0.78), lift)], toward, lettering);
            }
        }
    }

    /// Floodlights round the outside: tall galvanised columns with an arm reaching out over the
    /// track and a lamp at its end.
    fn floodlights(&self, world: &World, m: &mut TriMesh) {
        let total = world.route.total;
        let n = (total / LIGHT_EVERY).round() as usize;
        let out = -self.inside() * (self.half_width() + LIGHT_OUT);
        let edge = -self.inside() * self.half_width();
        for k in 0..n {
            let d = (k as f64 + 0.5) * total / n as f64;
            let base = world.on_track(d, out, 0.0);
            // None in front of Herne Hill's pavilion and stands, where the seats come up to the rail.
            let (a, b) = self.frame(base[0], base[2]);
            if self.track.id == crate::velodrome::HERNE_HILL.id && b > 0.0 && (-36.0..7.0).contains(&a) {
                continue;
            }
            let top = world.on_track(d, edge, LIGHT_HEIGHT)[1];
            let foot = [base[0], self.height - 0.3, base[2]];
            post(m, foot, top - foot[1], 0.18, GALVANISED);
            let toward = scale(world.across_track(d), self.inside());
            let head = [foot[0], top, foot[2]];
            let reach = add(add(head, scale(toward, 2.4)), [0.0, 0.35, 0.0]);
            beam(m, head, reach, 0.1, 0.1, GALVANISED);
            beam(m, reach, add(add(reach, scale(toward, 0.7)), [0.0, 0.05, 0.0]), 0.36, 0.16, LAMP_HEAD);
        }
    }
}

// ── Herne Hill's grounds ───────────────────────────────────────────────────

/// The pavilion's roof in section, front to back: (metres from the front of the seating, height
/// of the top). It sweeps down over the viewing terrace, rises to the back and curls over.
const ROOF_PROFILE: [(f64, f64); 13] =
    [(0.9, 7.25), (2.5, 6.95), (4.5, 6.8), (6.5, 6.95), (8.5, 7.35), (10.5, 7.85), (12.5, 8.3), (14.0, 8.5), (15.0, 8.45), (15.6, 8.15), (16.0, 7.6), (16.15, 6.9), (16.15, 6.6)];
const ROOF_THICK: f64 = 0.3;

impl Venue {
    fn herne_hill(&self, m: &mut TriMesh) {
        let edge = self.outside_edge();
        // Paving beside the home straight, and the car park behind the pavilion.
        self.patch(m, (-45.0, 26.0), (edge + 1.6, edge + 27.0), 0.03, PAVING);
        self.patch(m, (6.0, 26.0), (self.beside(72.5), edge + 27.0), 0.06, ASPHALT);
        for k in 0..8 {
            let a = 7.5 + k as f64 * 2.5;
            self.patch(m, (a, a + 0.1), (self.beside(73.5), self.beside(78.0)), 0.08, LINE_WHITE);
        }
        self.pavilion(m);
        self.grandstand(m);
        self.stores(m);
        self.infield(m);
    }

    /// The pavilion (2018): brick below, cedar above with a glazed club room facing the track,
    /// seating steps up to a viewing terrace, and a wave of a roof curling over at the back.
    fn pavilion(&self, m: &mut TriMesh) {
        let (a0, a1) = (-12.5, 4.6);
        let front = self.beside(55.8);
        let b = |x: f64| front + x;
        // Seating steps up to the terrace, with a narrower stair at each end.
        let (s0, s1) = (a0 + 1.3, a1 - 1.3);
        for k in 0..6 {
            let x = k as f64 * 0.75;
            self.block(m, (s0, s1), (b(x), b(x + 0.75)), (0.0, 0.5 * (k + 1) as f64), BRICK, CONCRETE);
        }
        for (e0, e1) in [(a0, s0), (s1, a1)] {
            for k in 0..12 {
                let x = k as f64 * 0.375;
                self.block(m, (e0, e1), (b(x), b(x + 0.375)), (0.0, 0.25 * (k + 1) as f64), CONCRETE, CONCRETE);
            }
            let outer = if e0 == a0 { a0 + 0.05 } else { a1 - 0.05 };
            beam(m, self.p(outer, b(0.0), 1.0), self.p(outer, b(4.5), 4.0), 0.05, 0.05, GALVANISED);
            for x in [0.2, 1.6, 3.0, 4.4] {
                post(m, self.p(outer, b(x), 0.25 * ((x / 0.375).floor() + 1.0)), 0.9, 0.05, GALVANISED);
            }
        }
        // Brick below the terrace and the club room, with a grey band at first-floor level.
        self.block(m, (a0, a1), (b(4.5), b(16.0)), (0.0, 3.0), BRICK, CONCRETE);
        self.block(m, (a0 - 0.05, a1 + 0.05), (b(4.45), b(16.05)), (2.95, 3.1), COPING, CONCRETE);
        // A door and a window in the brick at the south end.
        let mut south = |x0: f64, x1: f64, y0: f64, y1: f64, c: [f64; 3]| {
            quad(m, [self.p(a1 + 0.02, b(x0), y0), self.p(a1 + 0.02, b(x1), y0), self.p(a1 + 0.02, b(x1), y1), self.p(a1 + 0.02, b(x0), y1)], self.dir(1.0, 0.0), c);
        };
        south(8.6, 9.8, 0.0, 2.2, DOOR);
        south(11.8, 14.2, 0.9, 2.2, GLASS);
        // Terrace railing, open where the steps come up.
        for k in 0..=10 {
            let a = a0 + 0.1 + k as f64 * (a1 - a0 - 0.2) / 10.0;
            post(m, self.p(a, b(4.6), 3.1), 1.0, 0.05, GALVANISED);
        }
        for (r0, r1) in [(a0 + 0.1, s0 + 1.5), (s1 - 1.5, a1 - 0.1)] {
            beam(m, self.p(r0, b(4.6), 4.1), self.p(r1, b(4.6), 4.1), 0.05, 0.05, GALVANISED);
        }

        // The roof: its section swept along the building, a little past each end.
        let underside = |x: f64| -> f64 {
            let i = ROOF_PROFILE.windows(2).position(|w| x <= w[1].0).unwrap_or(ROOF_PROFILE.len() - 2);
            let (p, q) = (ROOF_PROFILE[i], ROOF_PROFILE[i + 1]);
            p.1 + (q.1 - p.1) * ((x - p.0) / (q.0 - p.0)).clamp(0.0, 1.0) - ROOF_THICK
        };
        let normals: Vec<(f64, f64)> = (0..ROOF_PROFILE.len())
            .map(|i| {
                let (p, q) = (ROOF_PROFILE[i.saturating_sub(1)], ROOF_PROFILE[(i + 1).min(ROOF_PROFILE.len() - 1)]);
                let (dx, dy) = (q.0 - p.0, q.1 - p.1);
                let len = dx.hypot(dy).max(1e-9);
                (-dy / len, dx / len)
            })
            .collect();
        let top: Vec<(f64, f64)> = ROOF_PROFILE.to_vec();
        let inner: Vec<(f64, f64)> = ROOF_PROFILE.iter().zip(&normals).map(|(p, n)| (p.0 - n.0 * ROOF_THICK, p.1 - n.1 * ROOF_THICK)).collect();
        let (r0, r1) = (a0 - 0.5, a1 + 0.5);
        for i in 0..top.len() - 1 {
            let up = |n: (f64, f64)| [self.dir(0.0, n.0)[0], n.1, self.dir(0.0, n.0)[2]];
            let (p, q) = (top[i], top[i + 1]);
            quad(m, [self.p(r0, b(p.0), p.1), self.p(r1, b(p.0), p.1), self.p(r1, b(q.0), q.1), self.p(r0, b(q.0), q.1)], up(normals[i]), ROOF);
            let (p, q) = (inner[i], inner[i + 1]);
            quad(m, [self.p(r0, b(p.0), p.1), self.p(r1, b(p.0), p.1), self.p(r1, b(q.0), q.1), self.p(r0, b(q.0), q.1)], scale(up(normals[i]), -1.0), SOFFIT);
            for (a, side) in [(r0, -1.0), (r1, 1.0)] {
                let (t0, t1, n0, n1) = (top[i], top[i + 1], inner[i], inner[i + 1]);
                quad(m, [self.p(a, b(t0.0), t0.1), self.p(a, b(t1.0), t1.1), self.p(a, b(n1.0), n1.1), self.p(a, b(n0.0), n0.1)], self.dir(side, 0.0), FASCIA);
            }
        }
        let (t, n) = (top[0], inner[0]);
        quad(m, [self.p(r0, b(t.0), t.1), self.p(r1, b(t.0), t.1), self.p(r1, b(n.0), n.1), self.p(r0, b(n.0), n.1)], self.dir(0.0, -1.0), FASCIA);
        // Slender dark columns under the front of the canopy, with timber beams on them.
        for k in 0..6 {
            let a = a0 + 0.6 + k as f64 * (a1 - a0 - 1.2) / 5.0;
            post(m, self.p(a, b(4.9), 3.1), underside(4.9) - 3.1, 0.14, COLUMN);
            let mut x: f64 = 1.0;
            while x < 6.4 {
                let next = (x + 1.0).min(6.4);
                beam(m, self.p(a, b(x), underside(x) - 0.14), self.p(a, b(next), underside(next) - 0.14), 0.16, 0.26, SOFFIT);
                x = next;
            }
        }

        // The glazed club room facing the track, cedar boards round the rest.
        let (g0, g1) = (6.4, 15.95);
        quad(m, [self.p(a0, b(g0), 3.1), self.p(a1, b(g0), 3.1), self.p(a1, b(g0), underside(g0)), self.p(a0, b(g0), underside(g0))], self.dir(0.0, -1.0), GLASS);
        let mut a = a0;
        while a < a1 - 0.01 {
            post(m, self.p(a + 0.04, b(g0 - 0.03), 3.1), underside(g0) - 3.1, 0.08, MULLION);
            a += 1.7;
        }
        beam(m, self.p(a0, b(g0 - 0.03), 5.7), self.p(a1, b(g0 - 0.03), 5.7), 0.06, 0.08, MULLION);
        // Boards on the back wall and on each end, up to the underside of the roof.
        let boards = 0.5;
        let mut a = a0;
        let mut k = 0;
        while a < a1 - 1e-6 {
            let next = (a + boards).min(a1);
            let c = if k % 2 == 0 { TIMBER } else { TIMBER_DARK };
            quad(m, [self.p(a, b(g1), 3.1), self.p(next, b(g1), 3.1), self.p(next, b(g1), 6.7), self.p(a, b(g1), 6.7)], self.dir(0.0, 1.0), c);
            a = next;
            k += 1;
        }
        for (end, side) in [(a0, -1.0), (a1, 1.0)] {
            let mut x = g0;
            let mut k = 0;
            while x < g1 - 1e-6 {
                let next = (x + boards).min(g1);
                let c = if k % 2 == 0 { TIMBER } else { TIMBER_DARK };
                quad(m, [self.p(end, b(x), 3.1), self.p(end, b(next), 3.1), self.p(end, b(next), underside(next)), self.p(end, b(x), underside(x))], self.dir(side, 0.0), c);
                x = next;
                k += 1;
            }
        }
        self.emblem(m, a1 + 0.03, b(13.4), 5.0);
    }

    /// The track emblem on the pavilion's south end: four stripes round a tall loop, running on
    /// along the wall toward the front.
    fn emblem(&self, m: &mut TriMesh, a: f64, centre: f64, y: f64) {
        let (half_w, half_h, stripe) = (0.8, 1.55, 0.085);
        let facing = self.dir(1.0, 0.0);
        let at = |x: f64, y: f64| self.p(a, x, y);
        for (k, colour) in EMBLEM.iter().enumerate() {
            let r = half_w - k as f64 * 0.1;
            let reach = half_h - half_w;
            // The loop as a stadium: round at top and bottom, straight up the sides.
            let mut line: Vec<(f64, f64)> = Vec::new();
            for j in 0..=12 {
                let t = std::f64::consts::PI * j as f64 / 12.0;
                line.push((centre + r * t.cos(), y + reach + r * t.sin()));
            }
            for j in 0..=12 {
                let t = std::f64::consts::PI + std::f64::consts::PI * j as f64 / 12.0;
                line.push((centre + r * t.cos(), y - reach + r * t.sin()));
            }
            line.push(line[0]);
            for w in line.windows(2) {
                let ((x0, y0), (x1, y1)) = (w[0], w[1]);
                let (dx, dy) = (x1 - x0, y1 - y0);
                let len = dx.hypot(dy).max(1e-9);
                let (nx, ny) = (-dy / len * stripe / 2.0, dx / len * stripe / 2.0);
                quad(m, [at(x0 - nx, y0 - ny), at(x1 - nx, y1 - ny), at(x1 + nx, y1 + ny), at(x0 + nx, y0 + ny)], facing, *colour);
            }
            // And along the wall from the bottom of the loop.
            let low = y - reach - r;
            quad(
                m,
                [at(self.beside(55.8) + 6.6, low - stripe / 2.0), at(centre, low - stripe / 2.0), at(centre, low + stripe / 2.0), at(self.beside(55.8) + 6.6, low + stripe / 2.0)],
                facing,
                *colour,
            );
        }
    }

    /// Open steel bleachers with black benches, north of the pavilion by the finish line.
    fn grandstand(&self, m: &mut TriMesh) {
        let (a0, a1) = (-33.5, -13.5);
        let front = self.outside_edge() + 1.9;
        for k in 0..5 {
            let (b0, b1) = (front + 1.1 * k as f64, front + 1.1 * (k + 1) as f64);
            let y = 0.45 * (k + 1) as f64;
            self.block(m, (a0, a1), (b0, b1), (0.0, y), STAND, STAND);
            self.block(m, (a0 + 0.2, a1 - 0.2), (b0 + 0.25, b0 + 0.7), (y, y + 0.42), SEAT, SEAT);
        }
        let back = front + 5.5;
        let top = 0.45 * 5.0;
        for k in 0..=8 {
            let a = a0 + k as f64 * (a1 - a0) / 8.0;
            post(m, self.p(a, back - 0.05, top), 1.1, 0.05, GALVANISED);
        }
        beam(m, self.p(a0, back - 0.05, top + 1.1), self.p(a1, back - 0.05, top + 1.1), 0.05, 0.05, GALVANISED);
    }

    /// The club's stores behind the bleachers: a row of shipping containers, and a white
    /// tensile canopy on three masts over the space behind them.
    fn stores(&self, m: &mut TriMesh) {
        let (b0, b1) = (self.beside(59.0), self.beside(65.06));
        for k in 0..8 {
            let a = -33.5 + 2.5 * k as f64;
            let c = CONTAINERS[k % CONTAINERS.len()];
            self.block(m, (a, a + 2.44), (b0, b1), (0.0, 2.59), c, c.map(|v| v * 0.85));
            // Ribs down the sides, a shade darker.
            for r in 1..10 {
                let b = b0 + (b1 - b0) * r as f64 / 10.0;
                for (side, aa) in [(-1.0, a - 0.02), (1.0, a + 2.46)] {
                    quad(m, [self.p(aa, b - 0.08, 0.1), self.p(aa, b + 0.08, 0.1), self.p(aa, b + 0.08, 2.5), self.p(aa, b - 0.08, 2.5)], self.dir(side, 0.0), c.map(|v| v * 0.78));
                }
            }
        }
        // The canopy: a membrane pulled up into a peak over each mast.
        let (c0, c1, d0, d1) = (-33.5, -13.5, self.beside(65.4), self.beside(76.0));
        let masts = [-28.5, -23.5, -18.5].map(|a| (a, (d0 + d1) / 2.0));
        let height = |a: f64, bb: f64| -> f64 {
            let peak = masts.iter().map(|&(ma, mb)| (1.0 - (a - ma).hypot(bb - mb) / 6.5).max(0.0).powi(2)).fold(0.0, f64::max);
            3.0 + 4.2 * peak
        };
        let (na, nb) = (40, 20);
        let grid = |i: usize, j: usize| {
            let (a, bb) = (c0 + (c1 - c0) * i as f64 / na as f64, d0 + (d1 - d0) * j as f64 / nb as f64);
            self.p(a, bb, height(a, bb))
        };
        for i in 0..na {
            for j in 0..nb {
                quad(m, [grid(i, j), grid(i + 1, j), grid(i + 1, j + 1), grid(i, j + 1)], [0.0, 1.0, 0.0], MEMBRANE);
            }
        }
        for &(a, bb) in &masts {
            post(m, self.p(a, bb, 0.0), 7.4, 0.16, GALVANISED);
        }
        for a in [c0, (c0 + c1) / 2.0, c1] {
            for bb in [d0, d1] {
                post(m, self.p(a, bb, 0.0), 3.0, 0.12, GALVANISED);
            }
        }
    }

    /// In the middle: the junior track with hard standing inside it, the path in from the
    /// tunnel under the main track, and a small white shelter.
    fn infield(&self, m: &mut TriMesh) {
        let (ca, cb, half, r, w) = (-17.7, -0.4, 21.9, 25.5, 4.5);
        let n = 120;
        let point = |k: usize, off: f64| -> (f64, f64) {
            // Round a stadium: along the straight at +b, the bend at -a, back along -b, the bend at +a.
            let s = k as f64 / n as f64 * (4.0 * half + 2.0 * std::f64::consts::PI * r);
            let bend = std::f64::consts::PI * r;
            let rr = r + off;
            if s < 2.0 * half {
                (ca + half - s, cb + rr)
            } else if s < 2.0 * half + bend {
                let t = std::f64::consts::FRAC_PI_2 + (s - 2.0 * half) / r;
                (ca - half + rr * t.cos(), cb + rr * t.sin())
            } else if s < 4.0 * half + bend {
                (ca - half + (s - 2.0 * half - bend), cb - rr)
            } else {
                let t = -std::f64::consts::FRAC_PI_2 + (s - 4.0 * half - bend) / r;
                (ca + half + rr * t.cos(), cb + rr * t.sin())
            }
        };
        for k in 0..n {
            let (p0, p1, q0, q1) = (point(k, -w / 2.0), point(k, w / 2.0), point(k + 1, -w / 2.0), point(k + 1, w / 2.0));
            quad(m, [self.p(p0.0, p0.1, 0.03), self.p(q0.0, q0.1, 0.03), self.p(q1.0, q1.1, 0.03), self.p(p1.0, p1.1, 0.03)], [0.0, 1.0, 0.0], JUNIOR);
            for off in [-w / 2.0 + 0.1, w / 2.0 - 0.1] {
                let (e0, e1, f0, f1) = (point(k, off - 0.04), point(k, off + 0.04), point(k + 1, off - 0.04), point(k + 1, off + 0.04));
                quad(m, [self.p(e0.0, e0.1, 0.045), self.p(f0.0, f0.1, 0.045), self.p(f1.0, f1.1, 0.045), self.p(e1.0, e1.1, 0.045)], [0.0, 1.0, 0.0], LINE_WHITE);
            }
        }
        self.patch(m, (-33.0, -5.0), (-14.0, 14.0), 0.03, PAVING);
        self.patch(m, (-11.0, -6.5), (cb + r + w / 2.0, self.track.radius - self.half_width()), 0.03, PAVING);
        for (a, bb) in [(-21.0, 31.0), (-15.0, 31.0), (-15.0, 35.5), (-21.0, 35.5)] {
            post(m, self.p(a, bb, 0.0), 2.6, 0.1, COPING);
        }
        self.block(m, (-21.2, -14.8), (30.8, 35.7), (2.6, 2.8), SHELTER_ROOF, SHELTER_ROOF);
    }
}
