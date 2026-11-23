//! Crowd barriers along both sides of the route, as if the road were closed for a race.
//! They set the road being ridden apart from the mapped road network laid on the ground.
//!
//! The barrier is one steel section (feet, end posts, two rails and uprights) in a few
//! looks: plain, or carrying a banner in one of the event colours. Sections are placed
//! every `SECTION_M` along each side; the renderers draw the ones near the rider. Like a
//! real one, each section is a rigid frame standing on its feet at both ends, so on a hill
//! it leans with the road, and neighbours meet at the same height.
//!
//! Rigid sections can't follow a hairpin: on the outside they fan apart and on the inside
//! they pile into each other. So round a tight bend there are none on either side, and a
//! netting fence runs round the outside instead, panel by panel along the outside edge.

use super::mesh::TriMesh;
use super::{World, OPEN, ROAD_LIFT};

/// Length of one barrier section, and the gap left between neighbours.
pub const SECTION_M: f64 = 2.5;
const LENGTH: f64 = 2.35;
const HEIGHT: f64 = 1.1;
/// How far beyond the edge of the tarmac the barriers stand.
const SET_BACK: f64 = 0.55;

/// Banner colours (sRGB), in the order of `look` 1, 2, 3, 4: the website's lime, paper, olive and pale lime.
pub const BANNER_COLOURS: [[f64; 3]; 4] = [[0.914, 1.0, 0.376], [0.969, 0.969, 0.937], [0.667, 0.737, 0.314], [0.855, 0.910, 0.455]];
/// The look of a netting fence panel, the last one.
pub const FENCE: usize = 1 + BANNER_COLOURS.len();
/// Tropical roadside props, drawn through the same instancing as the barriers: a tall lamp
/// with two arms on an avenue's median, a lamp with one arm over a town street, and a section
/// of concrete median barrier (a "New Jersey") down the middle of an open divided road.
pub const LAMP_DOUBLE: usize = FENCE + 1;
pub const LAMP_SINGLE: usize = FENCE + 2;
pub const JERSEY: usize = FENCE + 3;
/// From a route's open data: a lattice tower of a high-voltage line, a concrete pole of a minor
/// line (or one seen by Mapillary), and a water tower.
pub const TOWER: usize = FENCE + 4;
pub const POLE: usize = FENCE + 5;
pub const WATER_TOWER: usize = FENCE + 6;
pub const LOOKS: usize = FENCE + 7;

/// A bend tighter than this radius (metres, at the centre line) gets a fence, not barriers.
const TIGHT_RADIUS_M: f64 = 30.0;
/// The bend is measured over this far either side, so a single kink in the map isn't one.
const BEND_WINDOW_M: f64 = 4.0;
/// The fence runs on this far beyond where the bend is tight, into its entry and exit.
const BEND_MARGIN_M: f64 = 5.0;
/// Length and height of one fence panel.
const PANEL_M: f64 = 2.0;
const FENCE_HEIGHT: f64 = 1.4;

fn linear(c: [f64; 3]) -> [f32; 3] {
    c.map(|v| v.powf(2.2) as f32)
}

/// Where a texture lookup lands on plain white in the banner texture (its top-left corner).
const WHITE: [f32; 2] = [0.01, 0.02];

/// A box from `lo` to `hi` (local metres: x across the road, y up, z along it).
fn cuboid(mesh: &mut TriMesh, lo: [f64; 3], hi: [f64; 3], colour: [f32; 3]) {
    let c = [colour; 3];
    let w = [WHITE; 3];
    let (x0, y0, z0, x1, y1, z1) = (lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]);
    let faces: [([[f64; 3]; 4], [f64; 3]); 6] = [
        ([[x0, y0, z0], [x0, y1, z0], [x0, y1, z1], [x0, y0, z1]], [-1.0, 0.0, 0.0]),
        ([[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]], [1.0, 0.0, 0.0]),
        ([[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]], [0.0, 1.0, 0.0]),
        ([[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]], [0.0, -1.0, 0.0]),
        ([[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0]], [0.0, 0.0, -1.0]),
        ([[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]], [0.0, 0.0, 1.0]),
    ];
    for (p, facing) in faces {
        mesh.triangle([p[0], p[1], p[2]], facing, c, w);
        mesh.triangle([p[0], p[2], p[3]], facing, c, w);
    }
}

/// One barrier section in its own frame: centred on the origin, standing on y = 0, running
/// along z, with the road on its −x side. `look` 0 is plain steel; 1… carry a banner facing
/// the road, textured from the banner texture (white with the event name in grey) and tinted;
/// `FENCE` is a panel of netting fence.
pub fn barrier_model(look: usize) -> TriMesh {
    match look {
        FENCE => return fence_model(),
        LAMP_DOUBLE => return lamp_model(true),
        LAMP_SINGLE => return lamp_model(false),
        JERSEY => return jersey_model(),
        TOWER => return tower_model(),
        POLE => return pole_model(),
        WATER_TOWER => return water_tower_model(),
        _ => {}
    }
    let mut m = TriMesh::default();
    let steel = linear([0.74, 0.76, 0.78]);
    let dark = linear([0.42, 0.44, 0.46]);
    let half = LENGTH / 2.0;
    // Flat feet sticking out each side at both ends, end posts, top and bottom rails, uprights.
    for z in [-half + 0.06, half - 0.06] {
        cuboid(&mut m, [-0.32, 0.0, z - 0.03], [0.32, 0.04, z + 0.03], dark);
        cuboid(&mut m, [-0.025, 0.0, z - 0.025], [0.025, HEIGHT, z + 0.025], steel);
    }
    cuboid(&mut m, [-0.022, HEIGHT - 0.045, -half], [0.022, HEIGHT, half], steel);
    cuboid(&mut m, [-0.022, 0.16, -half], [0.022, 0.2, half], steel);
    let bars = 11;
    for k in 1..bars {
        let z = -half + LENGTH * k as f64 / bars as f64;
        cuboid(&mut m, [-0.008, 0.2, z - 0.008], [0.008, HEIGHT - 0.045, z + 0.008], steel);
    }
    if let Some(colour) = look.checked_sub(1).and_then(|i| BANNER_COLOURS.get(i)) {
        // The banner: a thin panel hung on the road side, the full length and most of the height.
        let tint = linear(*colour);
        let (x, y0, y1) = (-0.035, 0.24, HEIGHT - 0.07);
        let (z0, z1) = (-half + 0.05, half - 0.05);
        // Front face (towards the road) carries the texture, read left to right from the road.
        let p = [[x, y0, z0], [x, y0, z1], [x, y1, z1], [x, y1, z0]];
        // v runs up the banner (textures are uploaded bottom row first in both renderers).
        let uv = [[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        m.triangle([p[0], p[1], p[2]], [-1.0, 0.0, 0.0], [tint; 3], [uv[0], uv[1], uv[2]]);
        m.triangle([p[0], p[2], p[3]], [-1.0, 0.0, 0.0], [tint; 3], [uv[0], uv[2], uv[3]]);
        // Back of the banner: plain colour.
        cuboid(&mut m, [x + 0.002, y0, z0], [x + 0.012, y1, z1], tint);
    }
    m
}

/// A street lamp standing at the origin: a grey steel pole on a concrete plinth. With `double`,
/// a 10 m pole with an arm and a lamp head out to each side (across the road, along x), as on
/// an avenue's median; otherwise a 9 m pole with one arm reaching over the road (−x).
fn lamp_model(double: bool) -> TriMesh {
    let mut m = TriMesh::default();
    let (steel, concrete, glass) = (linear([0.55, 0.57, 0.60]), linear([0.70, 0.69, 0.66]), linear([0.92, 0.90, 0.80]));
    let top = if double { 10.0 } else { 9.0 };
    cuboid(&mut m, [-0.25, 0.0, -0.25], [0.25, 0.5, 0.25], concrete);
    cuboid(&mut m, [-0.09, 0.5, -0.09], [0.09, top, 0.09], steel);
    let arms: &[f64] = if double { &[-1.0, 1.0] } else { &[-1.0] };
    for &side in arms {
        let end = side * 2.2;
        // The arm, out from the pole to the lamp head.
        cuboid(&mut m, [end.min(0.0), top - 0.3, -0.05], [end.max(0.0), top - 0.2, 0.05], steel);
        cuboid(&mut m, [end - 0.35, top - 0.42, -0.16], [end + 0.35, top - 0.22, 0.16], steel);
        cuboid(&mut m, [end - 0.3, top - 0.46, -0.13], [end + 0.3, top - 0.42, 0.13], glass);
    }
    m
}

/// A section of concrete median barrier ("New Jersey"), `SECTION_M` long along z, centred on
/// the origin: wide at the foot and narrow on top, its road face (−x) painted with a yellow and
/// a black band so that a row of them reads as stripes.
fn jersey_model() -> TriMesh {
    let mut m = TriMesh::default();
    let concrete = linear([0.72, 0.71, 0.68]);
    let half = LENGTH / 2.0;
    cuboid(&mut m, [-0.30, 0.0, -half], [0.30, 0.25, half], concrete);
    cuboid(&mut m, [-0.18, 0.25, -half], [0.18, 0.55, half], concrete);
    cuboid(&mut m, [-0.09, 0.55, -half], [0.09, 0.80, half], concrete);
    cuboid(&mut m, [-0.305, 0.02, -half], [-0.295, 0.23, 0.0], linear([0.90, 0.72, 0.10]));
    cuboid(&mut m, [-0.305, 0.02, 0.0], [-0.295, 0.23, half], linear([0.08, 0.08, 0.08]));
    m
}

/// A lattice tower of a high-voltage line, about 30 m tall, standing on the origin with the line
/// along z: four legs leaning in, cross-bracing, and three arms across (x) to hang the wires on.
fn tower_model() -> TriMesh {
    let mut m = TriMesh::default();
    let steel = linear([0.62, 0.64, 0.66]);
    let (base, top, height) = (2.4, 0.6, 30.0);
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    let at = |h: f64, (cx, cz): (f64, f64)| {
        let r = base + (top - base) * h / height;
        (cx * r, cz * r)
    };
    // The legs, in five straight stretches each, and a ring of bracing at every joint.
    for k in 0..5 {
        let (h0, h1) = (k as f64 * 6.0, (k + 1) as f64 * 6.0);
        for c in corners {
            let (p, q) = (at(h0, c), at(h1, c));
            let (x0, z0) = ((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0);
            cuboid(&mut m, [x0 - 0.12, h0, z0 - 0.12], [x0 + 0.12, h1, z0 + 0.12], steel);
        }
        let r = at(h1, (1.0, 1.0)).0;
        cuboid(&mut m, [-r, h1 - 0.1, -r], [r, h1, -r + 0.1], steel);
        cuboid(&mut m, [-r, h1 - 0.1, r - 0.1], [r, h1, r], steel);
        cuboid(&mut m, [-r, h1 - 0.1, -r], [-r + 0.1, h1, r], steel);
        cuboid(&mut m, [r - 0.1, h1 - 0.1, -r], [r, h1, r], steel);
    }
    for (y, reach) in [(26.0, 6.4), (29.5, 1.2)] {
        cuboid(&mut m, [-reach, y - 0.3, -0.25], [reach, y, 0.25], steel);
    }
    m
}

/// A concrete pole of a minor power line, 11 m, with a cross-arm along x near the top.
fn pole_model() -> TriMesh {
    let mut m = TriMesh::default();
    let (concrete, steel) = (linear([0.68, 0.67, 0.64]), linear([0.40, 0.41, 0.42]));
    cuboid(&mut m, [-0.14, 0.0, -0.14], [0.14, 11.0, 0.14], concrete);
    cuboid(&mut m, [-1.2, 10.2, -0.06], [1.2, 10.35, 0.06], steel);
    m
}

/// A water tower: four legs and a tank, about 22 m tall.
fn water_tower_model() -> TriMesh {
    let mut m = TriMesh::default();
    let (concrete, tank) = (linear([0.70, 0.69, 0.66]), linear([0.78, 0.80, 0.82]));
    for (x, z) in [(-2.5, -2.5), (2.5, -2.5), (2.5, 2.5), (-2.5, 2.5)] {
        cuboid(&mut m, [x - 0.3, 0.0, z - 0.3], [x + 0.3, 15.0, z + 0.3], concrete);
    }
    cuboid(&mut m, [-3.6, 15.0, -3.6], [3.6, 21.0, 3.6], tank);
    cuboid(&mut m, [-2.4, 21.0, -2.4], [2.4, 22.2, 2.4], tank);
    m
}

/// One panel of netting fence, `PANEL_M` long: a post at each end, a top rail, and orange
/// diamond netting between, seen from both sides.
fn fence_model() -> TriMesh {
    let mut m = TriMesh::default();
    let post = linear([0.30, 0.33, 0.30]);
    let net = linear([0.96, 0.45, 0.12]);
    let half = PANEL_M / 2.0;
    // Posts set in a little from each end, so where two panels meet their posts stand side by side.
    for z in [-half + 0.04, half - 0.04] {
        cuboid(&mut m, [-0.03, 0.0, z - 0.03], [0.03, FENCE_HEIGHT, z + 0.03], post);
    }
    cuboid(&mut m, [-0.018, FENCE_HEIGHT - 0.05, -half], [0.018, FENCE_HEIGHT - 0.014, half], post);
    // The netting: two sets of diagonal strands, crossing in diamonds.
    let (y0, y1, x) = (0.08, FENCE_HEIGHT - 0.05, -0.035);
    let (gap, width) = (0.25, 0.024);
    let mut strand = |a: [f64; 2], b: [f64; 2]| {
        // a and b are (z, y); the strand is a thin flat ribbon between them.
        let (dz, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dz.hypot(dy);
        let (nz, ny) = (-dy / len * width / 2.0, dz / len * width / 2.0);
        for (x, facing) in [(x, -1.0), (x + 0.003, 1.0)] {
            let p = [[x, a[1] + ny, a[0] + nz], [x, b[1] + ny, b[0] + nz], [x, b[1] - ny, b[0] - nz], [x, a[1] - ny, a[0] - nz]];
            m.triangle([p[0], p[1], p[2]], [facing, 0.0, 0.0], [net; 3], [WHITE; 3]);
            m.triangle([p[0], p[2], p[3]], [facing, 0.0, 0.0], [net; 3], [WHITE; 3]);
        }
    };
    for slope in [1.0, -1.0] {
        // Strands z = start + slope·(y − y0), for starts every `gap` along the panel and beyond it,
        // clipped to the panel.
        let mut start = -half - (y1 - y0);
        while start < half + (y1 - y0) {
            // The heights at which the strand reaches the two ends of the panel (less its width,
            // so it stays within them).
            let end = half - width;
            let (ya, yb) = (y0 + (-end - start) / slope, y0 + (end - start) / slope);
            let (lo, hi) = (ya.min(yb).max(y0), ya.max(yb).min(y1));
            if hi - lo > 0.02 {
                strand([start + slope * (lo - y0), lo], [start + slope * (hi - y0), hi]);
            }
            start += gap;
        }
    }
    m
}

/// A stretch of road bending tighter than `TIGHT_RADIUS_M`, from and to metres along the
/// route, with the side of its outside (1 is the route's right, as in `barriers`).
struct Bend {
    from: f64,
    to: f64,
    side: f64,
}

impl World {
    /// The route's direction at sample `i`, smoothed over a couple of metres either way.
    fn heading_at(&self, i: usize) -> (f64, f64) {
        let r = &self.route;
        let w = ((2.0 / r.step).round() as usize).max(1);
        let (a, b) = (i.saturating_sub(w), (i + w).min(r.n - 1));
        let (tx, tz) = (r.x[b] - r.x[a], r.z[b] - r.z[a]);
        let len = tx.hypot(tz).max(1e-9);
        (tx / len, tz / len)
    }

    /// The route's tight bends, in order along it.
    fn tight_bends(&self) -> Vec<Bend> {
        let r = &self.route;
        let w = ((BEND_WINDOW_M / r.step).round() as usize).max(1);
        let span = w as f64 * r.step;
        let mut bends: Vec<Bend> = Vec::new();
        for i in w..r.n.saturating_sub(w) {
            let (tx, tz) = self.heading_at(i);
            let curvature = ((r.x[i + w] - 2.0 * r.x[i] + r.x[i - w]) * tz - (r.z[i + w] - 2.0 * r.z[i] + r.z[i - w]) * tx) / (span * span);
            if curvature.abs() * TIGHT_RADIUS_M < 1.0 {
                continue;
            }
            let (d, side) = (i as f64 * r.step, -curvature.signum());
            match bends.last_mut() {
                Some(b) if b.side == side && d - BEND_MARGIN_M <= b.to => b.to = d + BEND_MARGIN_M,
                _ => bends.push(Bend { from: (d - BEND_MARGIN_M).max(0.0), to: (d + BEND_MARGIN_M).min(r.total), side }),
            }
        }
        // Where one bend runs straight into another the other way, they meet halfway, so neither
        // fence reaches round the inside of the next bend.
        for k in 1..bends.len() {
            if bends[k].from < bends[k - 1].to {
                let mid = (bends[k].from + bends[k - 1].to) / 2.0;
                bends[k - 1].to = mid;
                bends[k].from = mid;
            }
        }
        bends
    }

    /// Netting fence panels round the outside of `bend`, as `barriers` gives them.
    fn bend_fence(&self, bend: &Bend, blocked: &impl Fn(usize) -> bool, out: &mut Vec<[f32; 7]>) {
        let r = &self.route;
        // The outside edge every half metre (x, y, z, distance along the route), broken where
        // the road has its own sides or is a trail.
        let mut runs: Vec<Vec<[f64; 4]>> = vec![Vec::new()];
        let mut d = bend.from;
        while d <= bend.to {
            let i = ((d / r.step).round() as usize).min(r.n - 1);
            if blocked(i) {
                if runs.last().is_some_and(|run| !run.is_empty()) {
                    runs.push(Vec::new());
                }
            } else {
                let (tx, tz) = self.heading_at(i);
                let p = r.sample_at(d);
                let off = bend.side * (self.half_width(i) + SET_BACK);
                runs.last_mut().unwrap().push([p.x + tz * off, p.ele + ROAD_LIFT - 0.12, p.z - tx * off, d]);
            }
            d += 0.5;
        }
        for run in runs.iter().filter(|run| run.len() >= 2) {
            // Distance along the edge at each point, and the point that far along it.
            let mut along = vec![0.0];
            for w in run.windows(2) {
                along.push(along.last().unwrap() + (w[1][0] - w[0][0]).hypot(w[1][2] - w[0][2]));
            }
            let length = *along.last().unwrap();
            let at = |s: f64| {
                let k = along.partition_point(|&a| a < s).clamp(1, run.len() - 1);
                let t = ((s - along[k - 1]) / (along[k] - along[k - 1]).max(1e-9)).clamp(0.0, 1.0);
                std::array::from_fn::<f64, 4, _>(|c| run[k - 1][c] + (run[k][c] - run[k - 1][c]) * t)
            };
            if length < 1.0 {
                continue;
            }
            // Panels evenly spaced along the edge, a whole number of them, so neighbours meet
            // to within a few centimetres.
            let panels = (length / PANEL_M).round().max(1.0) as usize;
            let spacing = length / panels as f64;
            for k in 0..panels {
                let mid = (k as f64 + 0.5) * spacing;
                let (a, b) = (at(mid - PANEL_M / 2.0), at(mid + PANEL_M / 2.0));
                let (dx, dy, dz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
                let climb = dy.atan2(dx.hypot(dz).max(1e-9));
                let yaw = dx.atan2(dz) + if bend.side > 0.0 { 0.0 } else { std::f64::consts::PI };
                let pitch = if bend.side > 0.0 { -climb } else { climb };
                let c = std::array::from_fn::<f64, 4, _>(|i| (a[i] + b[i]) / 2.0);
                out.push([c[0] as f32, c[1] as f32, c[2] as f32, yaw as f32, c[3] as f32, FENCE as f32, pitch as f32]);
            }
        }
    }

    /// Barrier sections along both sides of the route, nearest first along it:
    /// `[x, y, z, yaw, distance along the route, look, pitch]` each, where the model is
    /// tipped by `pitch` about its own x axis (Three.js's sense: positive lowers its +z end)
    /// before being turned by `yaw`. None on bridges (they have parapets) or in tunnels, nor
    /// on the inside of a bend too tight to stand them; round a tight bend, fence panels
    /// (look `FENCE`) on the outside instead.
    pub fn barriers(&self) -> Vec<[f32; 7]> {
        // A velodrome has its own railing.
        if self.venue.is_some() {
            return Vec::new();
        }
        // An open road has no crowd barriers; a tropical one has its lamps and median barriers.
        let mut out = if self.roadside.as_ref().is_some_and(|p| p.style.open_road) { Vec::new() } else { self.crowd_barriers() };
        if let Some(plan) = self.roadside.as_ref().filter(|p| p.tropical) {
            out.extend(super::roadside::props(self, plan));
        }
        out.sort_by(|a, b| a[4].total_cmp(&b[4]));
        out
    }

    /// The crowd barriers and bend fences alone.
    fn crowd_barriers(&self) -> Vec<[f32; 7]> {
        let r = &self.route;
        let kinds = self.field.kinds();
        let blocked = |i: usize| kinds.get(i).is_some_and(|&kind| kind != OPEN) || self.scene.trails.get(i).is_some_and(Option::is_some);
        let bends = self.tight_bends();
        let mut out = Vec::new();
        let count = ((r.total - 8.0) / SECTION_M).floor().max(0.0) as usize;
        for k in 0..count {
            let d = 4.0 + (k as f64 + 0.5) * SECTION_M;
            let i = ((d / r.step).round() as usize).min(r.n - 1);
            // None on bridges and in tunnels (they have their own sides), nor along a trail.
            if blocked(i) || bends.iter().any(|b| d + LENGTH / 2.0 > b.from && d - LENGTH / 2.0 < b.to) {
                continue;
            }
            let (a, b) = (i.saturating_sub(1), (i + 1).min(r.n - 1));
            let (mut tx, mut tz) = (r.x[b] - r.x[a], r.z[b] - r.z[a]);
            let len = tx.hypot(tz).max(1e-9);
            tx /= len;
            tz /= len;
            let curvature = if i > 0 && i + 1 < r.n { ((r.x[i + 1] - 2.0 * r.x[i] + r.x[i - 1]) * tz - (r.z[i + 1] - 2.0 * r.z[i] + r.z[i - 1]) * tx) / (r.step * r.step) } else { 0.0 };
            let p = r.sample_at(d);
            // Standing on its feet at both ends: the height of the road under each end, and the
            // slope between them.
            let (behind, ahead) = (r.sample_at(d - LENGTH / 2.0).ele, r.sample_at(d + LENGTH / 2.0).ele);
            let y = (behind + ahead) / 2.0 + ROAD_LIFT - 0.12;
            let climb = (ahead - behind).atan2(LENGTH);
            let offset = self.half_width(i) + SET_BACK;
            let heading = tx.atan2(tz);
            // Banners come in runs of four sections, each run in the next colour, with plain runs between.
            let run = k / 4;
            let look = if run % 3 == 1 { 1 + (run / 3) % BANNER_COLOURS.len() } else { 0 };
            for side in [1.0f64, -1.0] {
                // On the inside of a tight bend the barrier would stand in the road.
                if side * curvature > 0.0 && offset + 0.4 > 0.8 / curvature.abs() {
                    continue;
                }
                let (x, z) = (p.x + tz * side * offset, p.z - tx * side * offset);
                // The far side's barrier is turned round, so both have the road on their −x side.
                let yaw = if side > 0.0 { heading } else { heading + std::f64::consts::PI };
                // The model's +z end points up the road on this side and down it on the far side.
                let pitch = if side > 0.0 { -climb } else { climb };
                out.push([x as f32, y as f32, z as f32, yaw as f32, d as f32, look as f32, pitch as f32]);
            }
        }
        for bend in &bends {
            self.bend_fence(bend, &blocked, &mut out);
        }
        out.sort_by(|a, b| a[4].total_cmp(&b[4]));
        out
    }
}
