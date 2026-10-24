//! Berms: the banked outside of tight turns on mountain-bike trails.
//!
//! Turn radii are those of the route as ridden, after its corners are rounded off (a
//! switchback of a few metres becomes a bend of about 12–15 m), so the thresholds are set
//! for that. The bank is a cross-slope per route sample, in metres of rise per metre across (positive
//! when the +offset side is higher). The inside edge stays at the trail's height, so the
//! ground beside it is undisturbed; the outside edge is raised. Riders stand on the slope at
//! their place across the trail and lean with it.

use super::surface::Trail;
use super::{World, OPEN};
use crate::gpx::Route;

/// Turns wider than this (radius, metres) are ridden flat.
const FLAT_RADIUS: f64 = 30.0;
/// Turns this tight or tighter get the full bank.
const FULL_RADIUS: f64 = 12.0;
/// The steepest bank, in degrees.
const MAX_ANGLE: f64 = 35.0;
/// The most the outside edge is raised, in metres, however wide the trail.
const MAX_RISE: f64 = 1.0;
/// Banks gentler than this (degrees) are left flat.
const MIN_ANGLE: f64 = 2.0;

/// Signed curvature (1/m) at sample `i`, positive when the bend's centre is on the +offset side.
pub(super) fn curvature(route: &Route, i: usize) -> f64 {
    let n = route.n;
    if i == 0 || i + 1 >= n {
        return 0.0;
    }
    let (mut tx, mut tz) = (route.x[i + 1] - route.x[i - 1], route.z[i + 1] - route.z[i - 1]);
    let len = tx.hypot(tz).max(1e-9);
    tx /= len;
    tz /= len;
    ((route.x[i + 1] - 2.0 * route.x[i] + route.x[i - 1]) * tz - (route.z[i + 1] - 2.0 * route.z[i] + route.z[i - 1]) * tx) / (route.step * route.step)
}

/// The cross-slope at every route sample: banked on tight turns of trails with a mapped
/// mountain-bike grade (built trails have berms; walking paths and farm tracks don't), flat
/// elsewhere and on bridges. It builds up and eases off over about ten metres either side.
pub(super) fn banks(route: &Route, trails: &[Option<Trail>], widths: &[f64], kinds: &[u8]) -> Vec<f64> {
    let n = route.n;
    let bermed = |i: usize| trails.get(i).copied().flatten().is_some_and(|t| t.grade.is_some() && !t.estimated) && kinds.get(i).is_none_or(|&k| k == OPEN);
    let raw: Vec<f64> = (0..n)
        .map(|i| {
            let k = curvature(route, i);
            if !bermed(i) || k.abs() < 1e-9 {
                return 0.0;
            }
            let share = ((FLAT_RADIUS - 1.0 / k.abs()) / (FLAT_RADIUS - FULL_RADIUS)).clamp(0.0, 1.0);
            let half = widths.get(i).copied().unwrap_or(super::ROAD_WIDTH) / 2.0;
            let slope = (share * MAX_ANGLE).to_radians().tan().min(MAX_RISE / (2.0 * half));
            // The outside of the bend (the side away from its centre) is the high side.
            -k.signum() * slope
        })
        .collect();
    // Hold the steepest bank a sample either way (a turn rarely lines up with the samples),
    // then ease it in and out.
    let held: Vec<f64> = (0..n).map(|i| raw[i.saturating_sub(1)..(i + 2).min(n)].iter().copied().max_by(|a, b| a.abs().total_cmp(&b.abs())).unwrap_or(0.0)).collect();
    (0..n)
        .map(|i| {
            let window = &held[i.saturating_sub(2)..(i + 3).min(n)];
            let mean = window.iter().sum::<f64>() / window.len() as f64;
            // Never on a stretch that isn't bermed itself (the approach to a bridge, a road).
            if bermed(i) && mean.abs() >= MIN_ANGLE.to_radians().tan() {
                mean
            } else {
                0.0
            }
        })
        .collect()
}

impl World {
    /// The trail's cross-slope at sample `i` (rise per metre across; 0 where flat).
    pub fn bank(&self, i: usize) -> f64 {
        self.scene.banks.get(i).copied().unwrap_or(0.0)
    }

    /// How far above the trail's centre-line height the surface is at `offset` metres across,
    /// at sample `i`: the inside edge stays put and the outside edge rises; less any dip (a ford).
    pub fn surface_lift(&self, i: usize, offset: f64) -> f64 {
        let slope = self.bank(i);
        slope.abs() * self.half_width(i) + offset * slope - self.dip(i)
    }

    /// As `bank` and `surface_lift`, smoothly between samples, at `distance` metres along the
    /// route: the cross-slope, and the lift at `offset` metres across. For placing riders.
    pub fn bank_at(&self, distance: f64, offset: f64) -> (f64, f64) {
        let n = self.route.n;
        let f = (distance / self.route.step).clamp(0.0, (n - 1) as f64);
        let i = (f.floor() as usize).min(n.saturating_sub(2));
        let t = f - i as f64;
        let lerp = |a: f64, b: f64| a + (b - a) * t;
        (lerp(self.bank(i), self.bank(i + 1)), lerp(self.surface_lift(i, offset), self.surface_lift(i + 1, offset)))
    }
}
