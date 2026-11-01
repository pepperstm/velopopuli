//! Trees: a handful of kinds, each one shared low-poly model drawn many times over, and
//! which kind grows where.
//!
//! The mix follows altitude and latitude: conifers towards the tree line and in the north,
//! broadleaf woods in temperate lowlands, umbrella pines and cypresses in the south. Woods
//! grow in stands (a spruce plantation, a beech wood) rather than as a random mixture.

use super::height::{fbm, hash2, smoothstep};
use super::mesh::TriMesh;

/// Spruce or fir: a narrow stack of dark cones.
pub const SPRUCE: u8 = 0;
/// Pine: a tall bare trunk and a flattened crown at the top (Scots pine, or a stone pine in the south).
pub const PINE: u8 = 1;
/// Oak, beech and the like: a stout trunk and a broad rounded crown.
pub const BROADLEAF: u8 = 2;
/// Birch: a slim pale trunk and a light, narrow crown.
pub const BIRCH: u8 = 3;
/// A tall narrow column: cypress in the south, poplar elsewhere.
pub const COLUMN: u8 = 4;
/// A bush: no trunk, low and round.
pub const BUSH: u8 = 5;
pub const KINDS: usize = 6;

/// Numbers per tree in a chunk's list: x, y, z, yaw, width scale, height scale, kind, shade.
pub const TREE_FLOATS: usize = 8;

fn linear(c: [f64; 3]) -> [f32; 3] {
    c.map(|v| v.powf(2.2) as f32)
}

/// A ring of `sides` quads from radius `r0` at height `y0` to radius `r1` at `y0 + h`,
/// closed underneath (crowns are seen from below on a slope); a point when `r1` is 0.
fn frustum(m: &mut TriMesh, y0: f64, h: f64, (r0, r1): (f64, f64), sides: usize, colour: [f32; 3]) {
    let at = |k: usize, r: f64, y: f64| {
        let a = k as f64 / sides as f64 * std::f64::consts::TAU;
        [a.sin() * r, y, a.cos() * r]
    };
    for k in 0..sides {
        let mid = (k as f64 + 0.5) / sides as f64 * std::f64::consts::TAU;
        let out = [mid.sin(), (r0 - r1) / h.max(1e-6), mid.cos()];
        let (b0, b1, t0, t1) = (at(k, r0, y0), at(k + 1, r0, y0), at(k, r1, y0 + h), at(k + 1, r1, y0 + h));
        m.triangle([b0, b1, t1], out, [colour; 3], [[0.5; 2]; 3]);
        if r1 > 0.0 {
            m.triangle([b0, t1, t0], out, [colour; 3], [[0.5; 2]; 3]);
        }
        m.triangle([[0.0, y0, 0.0], b1, b0], [0.0, -1.0, 0.0], [colour; 3], [[0.5; 2]; 3]);
    }
}

/// A lumpy round mass of leaves: an icosahedron stretched to `radii`, each corner pushed in
/// or out a little (by `seed`) so no two clumps match.
fn clump(m: &mut TriMesh, centre: [f64; 3], radii: [f64; 3], colour: [f32; 3], seed: i32) {
    let t = (1.0 + 5f64.sqrt()) / 2.0;
    let corners: [[f64; 3]; 12] = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ];
    const FACES: [[usize; 3]; 20] = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    let norm = (1.0 + t * t).sqrt();
    let p: Vec<[f64; 3]> = corners
        .iter()
        .enumerate()
        .map(|(k, c)| {
            let bump = 0.86 + hash2(seed, k as i32) * 0.28;
            [centre[0] + c[0] / norm * radii[0] * bump, centre[1] + c[1] / norm * radii[1] * bump, centre[2] + c[2] / norm * radii[2] * bump]
        })
        .collect();
    for f in FACES {
        let tri = [p[f[0]], p[f[1]], p[f[2]]];
        let mid = [0, 1, 2].map(|a| (tri[0][a] + tri[1][a] + tri[2][a]) / 3.0 - centre[a]);
        m.triangle(tri, mid, [colour; 3], [[0.5; 2]; 3]);
    }
}

/// One kind of tree in its own frame, standing on y = 0, about life size (instances scale it
/// by roughly 0.75–1.3). Colours are linear; each instance's shade multiplies them.
pub fn tree_model(kind: u8) -> TriMesh {
    let mut m = TriMesh::default();
    let bark = linear([0.36, 0.26, 0.18]);
    match kind {
        SPRUCE => {
            frustum(&mut m, 0.0, 2.0, (0.26, 0.18), 5, bark);
            frustum(&mut m, 1.3, 5.0, (2.3, 0.0), 7, linear([0.15, 0.30, 0.19]));
            frustum(&mut m, 4.0, 4.4, (1.8, 0.0), 7, linear([0.16, 0.33, 0.2]));
            frustum(&mut m, 6.6, 3.8, (1.2, 0.0), 7, linear([0.18, 0.37, 0.22]));
        }
        PINE => {
            frustum(&mut m, 0.0, 7.6, (0.26, 0.15), 5, linear([0.48, 0.30, 0.2]));
            let needles = linear([0.2, 0.36, 0.2]);
            clump(&mut m, [0.0, 8.4, 0.0], [2.8, 1.3, 2.8], needles, 1);
            clump(&mut m, [1.0, 9.3, -0.5], [1.7, 0.95, 1.7], linear([0.23, 0.4, 0.22]), 2);
            clump(&mut m, [-1.1, 9.0, 0.7], [1.6, 0.9, 1.6], needles, 3);
        }
        BROADLEAF => {
            frustum(&mut m, 0.0, 4.0, (0.38, 0.26), 6, linear([0.32, 0.25, 0.19]));
            let (leaf, light) = (linear([0.28, 0.45, 0.19]), linear([0.33, 0.5, 0.21]));
            clump(&mut m, [0.0, 6.6, 0.0], [3.6, 2.9, 3.6], leaf, 4);
            clump(&mut m, [1.8, 5.7, 0.9], [2.4, 2.0, 2.4], light, 5);
            clump(&mut m, [-1.5, 5.9, -1.1], [2.5, 2.1, 2.5], leaf, 6);
            clump(&mut m, [0.3, 8.3, -0.4], [2.3, 1.8, 2.3], light, 7);
        }
        BIRCH => {
            frustum(&mut m, 0.0, 7.5, (0.15, 0.09), 5, linear([0.86, 0.84, 0.78]));
            clump(&mut m, [0.0, 7.4, 0.0], [1.9, 3.4, 1.9], linear([0.45, 0.58, 0.25]), 8);
            clump(&mut m, [0.5, 9.6, 0.3], [1.2, 1.9, 1.2], linear([0.5, 0.62, 0.28]), 9);
        }
        COLUMN => {
            frustum(&mut m, 0.0, 1.2, (0.22, 0.2), 5, bark);
            let dark = linear([0.16, 0.31, 0.18]);
            frustum(&mut m, 0.8, 3.0, (0.7, 1.25), 8, dark);
            frustum(&mut m, 3.8, 8.0, (1.25, 0.0), 8, dark);
        }
        _ => {
            let leaf = linear([0.3, 0.44, 0.2]);
            clump(&mut m, [0.0, 0.85, 0.0], [1.5, 1.05, 1.5], leaf, 10);
            clump(&mut m, [0.9, 0.7, 0.5], [1.0, 0.8, 1.0], linear([0.34, 0.48, 0.22]), 11);
        }
    }
    m
}

/// Where trees are growing, for choosing their kinds.
#[derive(Clone, Copy)]
pub struct Climate {
    /// Latitude, degrees from the equator.
    pub lat: f64,
    /// Height above which no trees grow, metres.
    pub tree_line: f64,
}

impl Climate {
    /// The kind of tree at a point `height` metres up, in a mapped or generated wood
    /// (`wood`) or standing on its own; `roll` and `pick` are the tree's own random numbers
    /// in [0, 1).
    pub fn kind_at(self, (x, z): (f64, f64), height: f64, wood: bool, roll: f64, pick: f64) -> u8 {
        // How far towards the tree line (0 in the valley, 1 just below it).
        let alpine = smoothstep(self.tree_line - 1300.0, self.tree_line - 350.0, height);
        let boreal = smoothstep(52.0, 62.0, self.lat);
        let south = (1.0 - smoothstep(40.0, 46.0, self.lat)) * (1.0 - alpine);
        // Woods come in stands: a few hundred metres of mostly one thing.
        let stand = (fbm(x / 210.0 + 3.1, z / 210.0 - 7.7) + 1.0) / 2.0;
        let conifers = if wood { (0.2 + 0.5 * boreal + (stand - 0.5) * 1.6).clamp(0.0, 1.0).max(alpine) } else { (0.08 + 0.3 * boreal).max(alpine * 0.9) };
        if roll < conifers {
            // Pines on dry southern slopes and among northern spruce; spruce in the mountains.
            let pines = 0.2 + 0.55 * south + 0.15 * boreal - 0.15 * alpine;
            return if pick < pines { PINE } else { SPRUCE };
        }
        let bushes = if wood { 0.07 } else { 0.3 };
        let columns = if wood { 0.08 * south } else { 0.06 + 0.16 * south };
        let birches = 0.08 + 0.4 * boreal + 0.1 * alpine;
        if pick < bushes {
            BUSH
        } else if pick < bushes + columns {
            COLUMN
        } else if pick < bushes + columns + birches {
            BIRCH
        } else {
            BROADLEAF
        }
    }
}

/// Trees further than this from the camera (metres, along the ground) are drawn as sprites:
/// a picture of their model, turned to face the camera, two triangles instead of a hundred.
/// Each tree switches at its own distance, up to `SPRITE_SPREAD / 2` either side, so there is
/// no line where the woods change. Both renderers use the same rule (`spriteFrom` in the
/// page's `trees.ts`, `sprite_from` in the native shader).
pub const SPRITES_FROM: f64 = 200.0;
pub const SPRITE_SPREAD: f64 = 30.0;

/// The rectangle a kind's sprite covers in its own frame: half its width across the picture
/// (x), and its lowest and highest points. The sprite pictures are drawn to fit it exactly,
/// one per kind side by side in the atlas (`kind / KINDS` to `(kind + 1) / KINDS` across).
pub fn sprite_frame(kind: u8) -> [f64; 3] {
    let m = tree_model(kind);
    m.positions.chunks(3).fold([0.0f64, f64::MAX, f64::MIN], |[w, lo, hi], p| [w.max((p[0] as f64).abs()), lo.min(p[1] as f64), hi.max(p[1] as f64)])
}
