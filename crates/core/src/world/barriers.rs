//! Crowd barriers along both sides of the route, as if the road were closed for a race.
//! They set the road being ridden apart from the mapped road network laid on the ground.
//!
//! The barrier is one steel section (feet, end posts, two rails and uprights) in a few
//! looks: plain, or carrying a banner in one of the event colours. Sections are placed
//! every `SECTION_M` along each side; the renderers draw the ones near the rider.

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
pub const LOOKS: usize = 1 + BANNER_COLOURS.len();

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
/// the road, textured from the banner texture (white with the event name in grey) and tinted.
pub fn barrier_model(look: usize) -> TriMesh {
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

impl World {
    /// Barrier sections along both sides of the route, nearest first along it:
    /// `[x, y, z, yaw, distance along the route, look]` each. None on bridges (they have
    /// parapets) or in tunnels, nor on the inside of a bend too tight to stand them.
    pub fn barriers(&self) -> Vec<[f32; 6]> {
        let r = &self.route;
        let kinds = self.field.kinds();
        let mut out = Vec::new();
        let count = ((r.total - 8.0) / SECTION_M).floor().max(0.0) as usize;
        for k in 0..count {
            let d = 4.0 + (k as f64 + 0.5) * SECTION_M;
            let i = ((d / r.step).round() as usize).min(r.n - 1);
            if kinds.get(i).is_some_and(|&kind| kind != OPEN) {
                continue;
            }
            let (a, b) = (i.saturating_sub(1), (i + 1).min(r.n - 1));
            let (mut tx, mut tz) = (r.x[b] - r.x[a], r.z[b] - r.z[a]);
            let len = tx.hypot(tz).max(1e-9);
            tx /= len;
            tz /= len;
            let curvature = if i > 0 && i + 1 < r.n { ((r.x[i + 1] - 2.0 * r.x[i] + r.x[i - 1]) * tz - (r.z[i + 1] - 2.0 * r.z[i] + r.z[i - 1]) * tx) / (r.step * r.step) } else { 0.0 };
            let p = r.sample_at(d);
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
                out.push([x as f32, (p.ele + ROAD_LIFT - 0.12) as f32, z as f32, yaw as f32, d as f32, look as f32]);
            }
        }
        out
    }
}
