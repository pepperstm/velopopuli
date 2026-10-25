//! The built things along the route that aren't buildings: bridge parapets and
//! piers (or, on a trail, a log bridge), tunnel bores with their portals and the
//! land over them, and the bridges that carry other roads and railways across overhead;
//! and the landmarks (`sights.rs`), which span the route the same way.

use super::height::{smoothstep, BLEND, BRIDGE, TUNNEL};
use super::mesh::TriMesh;
use super::scenery::{runs, Deck, Overbridge};
use super::sights::Rotor;
use super::{World, DECK_DEPTH, ROAD_LIFT};

/// One mesh for the whole route, like the road.
#[derive(Default)]
pub struct Structures {
    /// Lit by the sun like everything else outdoors.
    pub lit: TriMesh,
    /// Tunnel interiors, which daylight doesn't reach: drawn in exactly the colours given.
    pub unlit: TriMesh,
    /// Wind turbine rotors, for the renderer to turn (their towers are in `lit`).
    pub rotors: Vec<Rotor>,
}

type P3 = [f64; 3];
type Rgb = [f32; 3];

fn rgb(c: [f64; 3]) -> Rgb {
    c.map(|v| v.powf(2.2) as f32)
}

// Colours, authored in sRGB.
const CONCRETE: [f64; 3] = [0.70, 0.69, 0.66];
const CONCRETE_DARK: [f64; 3] = [0.50, 0.50, 0.48];
const ROCK: [f64; 3] = [0.47, 0.45, 0.42];
const ASPHALT: [f64; 3] = [0.30, 0.30, 0.32];
const BALLAST: [f64; 3] = [0.45, 0.41, 0.37];
const BANK: [f64; 3] = [0.36, 0.48, 0.24];
const LAMP: [f64; 3] = [1.0, 0.90, 0.66];
const TIMBER: [f64; 3] = [0.52, 0.39, 0.26];
const TIMBER_DARK: [f64; 3] = [0.36, 0.26, 0.17];

/// A log bridge: the logs under its planks, its handrail, and the trestles under a longer span.
const LOG_RADIUS: f64 = 0.17;
const RAIL_OUT: f64 = 0.22;
const RAIL_TOP: f64 = 1.05;
const TRESTLE_EVERY_M: f64 = 8.0;

/// Half-width of the bridge deck, out to the slab edge.
/// A bridge deck reaches this far beyond the edge of the road on it.
const DECK_BEYOND: f64 = 3.0;
/// The tunnel bore, as (sideways offset, height above the road) from one foot of the arch to the other.
const BORE: [(f64, f64); 7] = [(-4.7, -1.6), (-4.7, 3.4), (-3.2, 5.2), (0.0, 5.8), (3.2, 5.2), (4.7, 3.4), (4.7, -1.6)];
/// How bright the lining is at each of those points: the lamps hang from the crown and light the walls.
const BORE_SHADE: [f64; 7] = [0.36, 0.40, 0.27, 0.17, 0.27, 0.40, 0.36];
/// Sideways offsets at which the land over a tunnel is sampled. They include the bore's
/// own, so the portal wall can be cut neatly round it, and reach past the trench's rim.
const LID: [f64; 13] = [-46.0, -34.0, -23.0, -12.0, -4.7, -3.2, 0.0, 3.2, 4.7, 12.0, 23.0, 34.0, 46.0];

impl World {
    /// Unit direction of travel at a route sample, in (x, z).
    fn heading(&self, i: usize) -> (f64, f64) {
        let r = &self.route;
        let (a, b) = (i.saturating_sub(1), (i + 1).min(r.n - 1));
        let (tx, tz) = (r.x[b] - r.x[a], r.z[b] - r.z[a]);
        let len = tx.hypot(tz).max(1e-9);
        (tx / len, tz / len)
    }

    /// A point `offset` metres to the side of route sample `i` and `up` metres above the road there.
    fn beside(&self, i: usize, offset: f64, up: f64) -> P3 {
        let (tx, tz) = self.heading(i);
        [self.route.x[i] + tz * offset, self.route.ele[i] as f64 + ROAD_LIFT + up, self.route.z[i] - tx * offset]
    }

    pub fn structures(&self) -> Structures {
        let mut out = Structures::default();
        for (kind, from, to) in runs(self.field.kinds()) {
            match kind {
                // A trail crosses on a log bridge; a road on a concrete one.
                BRIDGE if (from..to).filter(|&i| self.timber(i)).count() * 2 >= to - from => self.log_bridge(&mut out.lit, from, to),
                BRIDGE => self.bridge(&mut out.lit, from, to),
                TUNNEL => self.tunnel(&mut out, from, to),
                _ => {}
            }
        }
        for over in &self.scene.overbridges {
            self.overbridge(&mut out.lit, over);
        }
        for deck in &self.scene.decks {
            self.deck(&mut out.lit, deck);
        }
        self.sight_models(&mut out.lit, &mut out.rotors);
        out
    }

    /// Four walls of an upright block with the given footprint.
    fn block(&self, mesh: &mut TriMesh, corners: [(f64, f64); 4], bottom: f64, top: f64, colour: [f64; 3]) {
        let (light, dark) = (rgb(colour), rgb(colour.map(|v| v * 0.8)));
        let centre = (corners.iter().map(|c| c.0).sum::<f64>() / 4.0, corners.iter().map(|c| c.1).sum::<f64>() / 4.0);
        for k in 0..4 {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            let out = [(a.0 + b.0) / 2.0 - centre.0, 0.0, (a.1 + b.1) / 2.0 - centre.1];
            mesh.quad([[a.0, bottom, a.1], [b.0, bottom, b.1], [b.0, top, b.1], [a.0, top, a.1]], out, [dark, dark, light, light]);
        }
    }

    /// The parts of a bridge the road ribbon doesn't supply: underside, parapets and piers.
    fn bridge(&self, mesh: &mut TriMesh, from: usize, to: usize) {
        let (concrete, dark) = (rgb(CONCRETE), rgb(CONCRETE_DARK));
        for i in from..to {
            let (h0, h1) = (self.half_width(i) + DECK_BEYOND, self.half_width(i + 1) + DECK_BEYOND);
            let under = [self.beside(i, -h0, -DECK_DEPTH), self.beside(i, h0, -DECK_DEPTH), self.beside(i + 1, h1, -DECK_DEPTH), self.beside(i + 1, -h1, -DECK_DEPTH)];
            mesh.quad(under, [0.0, -1.0, 0.0], [dark; 4]);
            for side in [-1.0, 1.0] {
                let (a0, a1) = (side * (h0 - 0.25), side * (h1 - 0.25));
                let (tx, tz) = self.heading(i);
                let wall = [self.beside(i, a0, -0.25), self.beside(i + 1, a1, -0.25), self.beside(i + 1, a1, 0.85), self.beside(i, a0, 0.85)];
                mesh.quad(wall, [-tz * side, 0.0, tx * side], [dark, dark, concrete, concrete]);
            }
        }
        // Piers under longer bridges, wherever the ground is far enough below to need one.
        let span = (to - from) as f64 * self.route.step;
        let piers = (span / 28.0).floor() as usize;
        for k in 1..=piers.min(200) {
            let i = from + ((to - from) * k) / (piers + 1);
            let (tx, tz) = self.heading(i);
            let top = self.route.ele[i] as f64 + ROAD_LIFT - DECK_DEPTH;
            let foot = |across: f64, along: f64| (self.route.x[i] + tz * across + tx * along, self.route.z[i] - tx * across + tz * along);
            let corners = [foot(-4.2, -0.7), foot(4.2, -0.7), foot(4.2, 0.7), foot(-4.2, 0.7)];
            let ground = corners.iter().map(|c| self.field.height(c.0, c.1).0).fold(f64::MAX, f64::min);
            if top - ground > 1.5 {
                self.block(mesh, corners, ground - 1.5, top, CONCRETE);
            }
        }
    }

    /// Whether sample `i` is on a log bridge: a bridge on a trail.
    pub(super) fn timber(&self, i: usize) -> bool {
        self.field.kinds().get(i) == Some(&BRIDGE) && self.scene.trails.get(i).is_some_and(Option::is_some)
    }

    /// The parts of a log bridge the planks don't supply: logs along the span underneath, a
    /// handrail on posts each side, and timber trestles where the span is long and high.
    fn log_bridge(&self, mesh: &mut TriMesh, from: usize, to: usize) {
        let (wood, dark) = (rgb(TIMBER), rgb(TIMBER_DARK));
        let to = to.min(self.route.n - 1);
        let across = |i: usize| {
            let (tx, tz) = self.heading(i);
            [tz, 0.0, -tx]
        };
        for i in from..to {
            let (h0, h1) = (self.half_width(i), self.half_width(i + 1));
            // Three logs: one under each edge of the planks and one down the middle.
            for place in [-1.0, 0.0, 1.0] {
                let (c0, c1) = (place * (h0 - 0.2), place * (h1 - 0.2));
                let below = -0.1 - LOG_RADIUS;
                for k in 0..8 {
                    let (a, b) = (k as f64 * std::f64::consts::FRAC_PI_4, (k + 1) as f64 * std::f64::consts::FRAC_PI_4);
                    let at = |i: usize, c: f64, t: f64| self.beside(i, c + LOG_RADIUS * t.cos(), below + LOG_RADIUS * t.sin());
                    let mid = (a + b) / 2.0;
                    let n = across(i).map(|v| v * mid.cos());
                    mesh.quad([at(i, c0, a), at(i + 1, c1, a), at(i + 1, c1, b), at(i, c0, b)], [n[0], mid.sin(), n[2]], [if mid.sin() > 0.0 { wood } else { dark }; 4]);
                }
            }
            // The handrail: a top rail and a middle one each side, faced both ways.
            for side in [-1.0, 1.0] {
                let (a0, a1) = (side * (h0 + RAIL_OUT), side * (h1 + RAIL_OUT));
                for (lo, hi) in [(RAIL_TOP - 0.1, RAIL_TOP), (0.48, 0.56)] {
                    let face = [self.beside(i, a0, lo), self.beside(i + 1, a1, lo), self.beside(i + 1, a1, hi), self.beside(i, a0, hi)];
                    let out = across(i).map(|v| v * side);
                    mesh.quad(face, out, [dark, dark, wood, wood]);
                    mesh.quad(face, out.map(|v| -v), [dark, dark, wood, wood]);
                    mesh.quad([self.beside(i, a0 - 0.04, hi), self.beside(i + 1, a1 - 0.04, hi), self.beside(i + 1, a1 + 0.04, hi), self.beside(i, a0 + 0.04, hi)], [0.0, 1.0, 0.0], [wood; 4]);
                }
            }
        }
        // Posts for the handrail at every sample, from the timber edge to the top rail.
        let r = &self.route;
        let foot = |i: usize, across: f64, along: f64| {
            let (tx, tz) = self.heading(i);
            (r.x[i] + tz * across + tx * along, r.z[i] - tx * across + tz * along)
        };
        let deck = |i: usize| r.ele[i] as f64 + ROAD_LIFT;
        for i in from..=to {
            for side in [-1.0, 1.0] {
                let c = side * (self.half_width(i) + RAIL_OUT);
                let corners = [foot(i, c - 0.06, -0.06), foot(i, c + 0.06, -0.06), foot(i, c + 0.06, 0.06), foot(i, c - 0.06, 0.06)];
                self.block(mesh, corners, deck(i) - 0.3, deck(i) + RAIL_TOP + 0.05, TIMBER_DARK);
            }
        }
        // Trestles under a longer span, where the ground is far enough below: a pair of posts.
        let span = (to - from) as f64 * r.step;
        let trestles = (span / TRESTLE_EVERY_M).floor() as usize;
        for k in 1..=trestles.min(200) {
            let i = from + ((to - from) * k) / (trestles + 1);
            let top = deck(i) - 0.1 - 2.0 * LOG_RADIUS;
            for side in [-1.0, 1.0] {
                let c = side * (self.half_width(i) - 0.2);
                let corners = [foot(i, c - 0.13, -0.13), foot(i, c + 0.13, -0.13), foot(i, c + 0.13, 0.13), foot(i, c - 0.13, 0.13)];
                let ground = corners.iter().map(|p| self.field.height(p.0, p.1).0).fold(f64::MAX, f64::min);
                if top - ground > 0.8 {
                    self.block(mesh, corners, ground - 0.5, top, TIMBER_DARK);
                }
            }
        }
    }

    /// A tunnel: the bore and its lamps, the land over the trench it runs in, and a wall round each mouth.
    fn tunnel(&self, out: &mut Structures, from: usize, to: usize) {
        let r = &self.route;
        let ring = |i: usize| BORE.map(|(offset, up)| self.beside(i, offset, up));
        // Daylight reaches a little way in from each mouth; beyond that there are only the lamps.
        let lining = |i: usize| -> [Rgb; 7] {
            let depth = (i - from).min(to - i) as f64 * r.step;
            let dim = 1.0 - 0.45 * smoothstep(0.0, 40.0, depth);
            BORE_SHADE.map(|s| rgb([s * dim, s * dim * 0.97, s * dim * 0.9]))
        };
        let lamp = rgb(LAMP);
        for i in from..to {
            let (a, b) = (ring(i), ring(i + 1));
            let (la, lb) = (lining(i), lining(i + 1));
            for k in 0..BORE.len() - 1 {
                // Facing the road: the bore is only ever seen from inside.
                let inward = [r.x[i] - a[k][0], r.ele[i] as f64 + 2.5 - a[k][1], r.z[i] - a[k][2]];
                out.unlit.quad([a[k], a[k + 1], b[k + 1], b[k]], inward, [la[k], la[k + 1], lb[k + 1], lb[k]]);
            }
            // A lamp every twenty metres or so.
            if (i - from) % ((20.0 / r.step).round() as usize).max(1) == 0 {
                let (c, d) = (self.beside(i, 0.0, 5.72), self.beside(i + 1, 0.0, 5.72));
                let (tx, tz) = self.heading(i);
                let (sx, sz) = (tz * 0.45, -tx * 0.45);
                let mid = [(c[0] + d[0]) / 2.0, (c[1] + d[1]) / 2.0, (c[2] + d[2]) / 2.0];
                out.unlit.quad([[c[0] - sx, c[1], c[2] - sz], [c[0] + sx, c[1], c[2] + sz], [mid[0] + sx, mid[1], mid[2] + sz], [mid[0] - sx, mid[1], mid[2] - sz]], [0.0, -1.0, 0.0], [lamp; 4]);
            }
        }

        // The land above, from the same height function as the terrain around it, so the two meet at the trench's rim.
        let rows: Vec<Vec<P3>> = (from..=to)
            .map(|i| {
                LID.iter()
                    .enumerate()
                    .map(|(k, &offset)| {
                        let p = self.beside(i, offset, 0.0);
                        // Tucked under the terrain at its edges, and a touch proud of it in between.
                        let lift = if k == 0 || k == LID.len() - 1 { -0.8 } else { 0.15 };
                        [p[0], self.field.surface(p[0], p[2]).0 + lift, p[2]]
                    })
                    .collect()
            })
            .collect();
        let at = |row: usize, k: usize| rows[row.min(rows.len() - 1)][k.min(LID.len() - 1)];
        let vertex = |row: usize, k: usize| {
            let (p, along, across) = (at(row, k), (at(row.saturating_sub(1), k), at(row + 1, k)), (at(row, k.saturating_sub(1)), at(row, k + 1)));
            let (u, v) = ([along.1[0] - along.0[0], along.1[1] - along.0[1], along.1[2] - along.0[2]], [across.1[0] - across.0[0], across.1[1] - across.0[1], across.1[2] - across.0[2]]);
            let mut n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
            if n[1] < 0.0 {
                n = n.map(|c| -c);
            }
            let n = n.map(|c| c / len);
            (p, n, self.shade(p[0], p[2], p[1], n[1], BLEND, None))
        };
        for row in 0..rows.len() - 1 {
            for k in 0..LID.len() - 1 {
                let corners = [vertex(row, k), vertex(row + 1, k), vertex(row, k + 1), vertex(row, k + 1), vertex(row + 1, k), vertex(row + 1, k + 1)];
                // Wound to face upward whichever way the road is heading.
                let (a, b, c) = (corners[0].0, corners[1].0, corners[2].0);
                let up = (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]) > 0.0;
                let order = if up { [0, 1, 2, 3, 4, 5] } else { [0, 2, 1, 3, 5, 4] };
                for o in order {
                    out.lit.vertex(corners[o].0, corners[o].1, corners[o].2, [0.0; 2]);
                }
            }
        }

        // A wall across the trench at each mouth, with the bore cut out of it.
        let (concrete, rock) = (rgb(CONCRETE), rgb(ROCK));
        for (i, row, sign) in [(from, 0usize, -1.0), (to, rows.len() - 1, 1.0)] {
            let (tx, tz) = self.heading(i);
            let facing = [tx * sign, 0.0, tz * sign];
            // Outside the bore the wall goes down into the ground; across it, only down to the arch.
            // (`LID[4..=8]` are the offsets of `BORE[1..=5]`.)
            let foot = |k: usize, arch: bool| self.beside(i, LID[k], if arch { BORE[k - 3].1 } else { -1.5 });
            for k in 0..LID.len() - 1 {
                let (top_a, top_b) = (rows[row][k], rows[row][k + 1]);
                let colour = if LID[k].abs().max(LID[k + 1].abs()) <= 12.0 { concrete } else { rock };
                let arch = (4..8).contains(&k);
                let (a, b) = (foot(k, arch), foot(k + 1, arch));
                out.lit.quad([a, b, top_b, top_a], facing, [colour; 4]);
            }
        }
    }

    /// A bridge on another road: a deck from bank to bank, wherever the ground between falls away.
    fn deck(&self, mesh: &mut TriMesh, deck: &Deck) {
        let line = &deck.line;
        let half = deck.kind.width() / 2.0 + if deck.kind.width() < 3.0 { 0.2 } else { 0.7 };
        // A footbridge is a slimmer thing than a road bridge.
        let (slab, parapet) = if deck.kind.width() < 3.0 { (0.35, 1.1) } else { (0.9, 0.9) };
        let (surface, concrete, dark) = (rgb(deck.kind.colour()), rgb(CONCRETE), rgb(CONCRETE_DARK));
        let ground = |p: (f64, f64)| self.field.surface(p.0, p.1).0;
        // Level with the banks at each end and straight between, but never under the ground.
        let lengths: Vec<f64> = std::iter::once(0.0)
            .chain(line.windows(2).scan(0.0, |sum, w| {
                *sum += (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
                Some(*sum)
            }))
            .collect();
        let total = lengths[line.len() - 1].max(1e-9);
        let (first, last) = (ground(line[0]), ground(line[line.len() - 1]));
        let top: Vec<f64> = (0..line.len()).map(|i| (first + (last - first) * lengths[i] / total).max(ground(line[i])) + 0.25).collect();
        let side = |i: usize| {
            let (a, b) = (line[i.saturating_sub(1)], line[(i + 1).min(line.len() - 1)]);
            let len = (b.0 - a.0).hypot(b.1 - a.1).max(1e-9);
            ((b.1 - a.1) / len, -(b.0 - a.0) / len)
        };
        let edge = |i: usize, s: f64, up: f64| -> P3 {
            let n = side(i);
            [line[i].0 + n.0 * half * s, top[i] + up, line[i].1 + n.1 * half * s]
        };
        for i in 0..line.len() - 1 {
            mesh.quad([edge(i, -1.0, 0.0), edge(i, 1.0, 0.0), edge(i + 1, 1.0, 0.0), edge(i + 1, -1.0, 0.0)], [0.0, 1.0, 0.0], [surface; 4]);
            mesh.quad([edge(i, -1.0, -slab), edge(i, 1.0, -slab), edge(i + 1, 1.0, -slab), edge(i + 1, -1.0, -slab)], [0.0, -1.0, 0.0], [dark; 4]);
            for s in [-1.0, 1.0] {
                let n = side(i);
                mesh.quad([edge(i, s, -slab), edge(i + 1, s, -slab), edge(i + 1, s, parapet), edge(i, s, parapet)], [n.0 * s, 0.0, n.1 * s], [dark, dark, concrete, concrete]);
            }
        }
        // A pier every thirty metres or so, where there is far enough to the ground to want one.
        let every = (30.0 / 6.0) as usize;
        for i in (every..line.len().saturating_sub(every)).step_by(every) {
            let (n, c) = (side(i), line[i]);
            let foot = |across: f64, along: f64| (c.0 + n.0 * across - n.1 * along, c.1 + n.1 * across + n.0 * along);
            let reach = (half - 0.4).max(0.4);
            let corners = [foot(-reach, -0.5), foot(reach, -0.5), foot(reach, 0.5), foot(-reach, 0.5)];
            let low = corners.iter().map(|&p| self.field.height(p.0, p.1).0).fold(f64::MAX, f64::min);
            if top[i] - slab - low > 2.5 {
                self.block(mesh, corners, low - 1.5, top[i] - slab, CONCRETE);
            }
        }
    }

    /// A bridge that crosses over the route, with the banks that lead up to it.
    fn overbridge(&self, mesh: &mut TriMesh, over: &Overbridge) {
        let (line, half) = (&over.line, over.width / 2.0);
        let (under, top) = (over.deck, over.deck + 0.9);
        let surface = rgb(if over.rail { BALLAST } else { ASPHALT });
        let (concrete, dark, bank) = (rgb(CONCRETE), rgb(CONCRETE_DARK), rgb(BANK));
        let side = |i: usize| {
            let (a, b) = (line[i.saturating_sub(1)], line[(i + 1).min(line.len() - 1)]);
            let len = (b.0 - a.0).hypot(b.1 - a.1).max(1e-9);
            ((b.1 - a.1) / len, -(b.0 - a.0) / len)
        };
        let edge = |i: usize, s: f64, y: f64| -> P3 {
            let n = side(i);
            [line[i].0 + n.0 * half * s, y, line[i].1 + n.1 * half * s]
        };
        for i in 0..line.len() - 1 {
            mesh.quad([edge(i, -1.0, top), edge(i, 1.0, top), edge(i + 1, 1.0, top), edge(i + 1, -1.0, top)], [0.0, 1.0, 0.0], [surface; 4]);
            mesh.quad([edge(i, -1.0, under), edge(i, 1.0, under), edge(i + 1, 1.0, under), edge(i + 1, -1.0, under)], [0.0, -1.0, 0.0], [dark; 4]);
            for s in [-1.0, 1.0] {
                let n = side(i);
                // Slab edge and parapet in one face.
                mesh.quad([edge(i, s, under), edge(i + 1, s, under), edge(i + 1, s, top + 1.0), edge(i, s, top + 1.0)], [n.0 * s, 0.0, n.1 * s], [dark, dark, concrete, concrete]);
            }
        }

        let ground = |p: (f64, f64)| self.field.height(p.0, p.1).0;
        // Piers along a long bridge, clear of the road it crosses.
        let mut since = 0.0;
        for i in 0..line.len() - 1 {
            let (a, b) = (line[i], line[i + 1]);
            let len = (b.0 - a.0).hypot(b.1 - a.1);
            let (dx, dz) = ((b.0 - a.0) / len.max(1e-9), (b.1 - a.1) / len.max(1e-9));
            let mut s = 30.0 - since;
            while s < len {
                let c = (a.0 + dx * s, a.1 + dz * s);
                let foot = |across: f64, along: f64| (c.0 + dz * across + dx * along, c.1 - dx * across + dz * along);
                let corners = [foot(-half + 0.6, -0.7), foot(half - 0.6, -0.7), foot(half - 0.6, 0.7), foot(-half + 0.6, 0.7)];
                let low = corners.iter().map(|&p| ground(p)).fold(f64::MAX, f64::min);
                if self.field.height(c.0, c.1).1 > half + 8.0 && under - low > 1.5 {
                    self.block(mesh, corners, low - 1.5, under, CONCRETE);
                }
                s += 30.0;
            }
            since = (since + len) % 30.0;
        }

        // At each end, a wall down to the ground and an embankment sloping away to meet it.
        for end in [false, true] {
            let (tip, next) = if end { (line.len() - 1, line.len() - 2) } else { (0, 1) };
            let (dx, dz) = (line[tip].0 - line[next].0, line[tip].1 - line[next].1);
            let len = dx.hypot(dz).max(1e-9);
            let (dx, dz) = (dx / len, dz / len);
            // Cross-sections of the embankment every few metres until it has come down to the ground.
            let section = |s: f64| {
                let c = (line[tip].0 + dx * s, line[tip].1 + dz * s);
                let (l, r) = ((c.0 + dz * half, c.1 - dx * half), (c.0 - dz * half, c.1 + dx * half));
                let crest = top - s * 0.07;
                (l, r, crest, ground(l).min(ground(r)).min(ground(c)))
            };
            let (l, r, _, low) = section(0.0);
            mesh.quad([[l.0, low - 1.5, l.1], [r.0, low - 1.5, r.1], [r.0, under, r.1], [l.0, under, l.1]], [-dx, 0.0, -dz], [dark, dark, concrete, concrete]);
            let mut s = 0.0;
            while s < 120.0 {
                let (a, b) = (section(s), section(s + 6.0));
                if a.2 <= a.3 + 0.3 {
                    break;
                }
                let foot = |sec: &((f64, f64), (f64, f64), f64, f64)| sec.2.min(sec.3 - 1.5);
                mesh.quad([[a.0 .0, a.2, a.0 .1], [a.1 .0, a.2, a.1 .1], [b.1 .0, b.2, b.1 .1], [b.0 .0, b.2, b.0 .1]], [0.0, 1.0, 0.0], [surface; 4]);
                mesh.quad([[a.0 .0, foot(&a), a.0 .1], [b.0 .0, foot(&b), b.0 .1], [b.0 .0, b.2, b.0 .1], [a.0 .0, a.2, a.0 .1]], [dz, 0.0, -dx], [bank; 4]);
                mesh.quad([[a.1 .0, foot(&a), a.1 .1], [b.1 .0, foot(&b), b.1 .1], [b.1 .0, b.2, b.1 .1], [a.1 .0, a.2, a.1 .1]], [-dz, 0.0, dx], [bank; 4]);
                s += 6.0;
            }
        }
    }
}
