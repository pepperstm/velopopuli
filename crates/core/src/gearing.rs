//! Virtual gears: leave the bike in one real gear on the trainer and shift with keys or
//! buttons instead. The app changes the resistance so each virtual gear feels like the
//! real thing, with the same speed for the same power but more force per pedal stroke in a
//! harder gear.
//!
//! How: the trainer's flywheel turns at the speed the *real* gear gives for the rider's
//! cadence, `v_trainer = v × r_real / r_virtual`, and to feel the virtual gear the force
//! it applies must be the road force scaled by the ratio, `F_trainer = F_road(v) × r_virtual
//! / r_real`. Trainers in simulation mode work out their force from a gradient, rolling
//! resistance and wind coefficient at their own speed, so the gradient sent is the one that
//! makes that force come out right. In the real gear itself nothing changes.

use crate::types::{GearSettings, GearsMode};

const G: f64 = 9.80665;
/// The steepest gradients sent, whatever the gear: trainers ignore or misbehave beyond these.
const MIN_GRADE: f64 = -0.25;
const MAX_GRADE: f64 = 0.40;
/// Combinations closer together than this (as a ratio) count as one gear.
const SAME_GEAR: f64 = 1.025;

#[derive(Debug, Clone, PartialEq)]
pub struct Gear {
    /// Chainring teeth ÷ cog teeth.
    pub ratio: f64,
    /// "50×17", or "Gear 7" for presets that are just a range of ratios.
    pub label: String,
}

/// A ready-made gear set: an id, a name, and either chainrings and cogs, or evenly spaced ratios.
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub chainrings: &'static [u32],
    pub cogs: &'static [u32],
    /// For a ratio-only set: (count, easiest, hardest).
    pub even: Option<(usize, f64, f64)>,
}

pub const PRESETS: [Preset; 6] = [
    Preset { id: "even-24", name: "24 evenly spaced gears", chainrings: &[], cogs: &[], even: Some((24, 0.75, 4.8)) },
    Preset { id: "road-compact-12", name: "Road compact: 50/34 × 11–34 (12-speed)", chainrings: &[50, 34], cogs: &[11, 12, 13, 14, 15, 17, 19, 21, 24, 27, 30, 34], even: None },
    Preset { id: "road-semi-12", name: "Road semi-compact: 52/36 × 11–30 (12-speed)", chainrings: &[52, 36], cogs: &[11, 12, 13, 14, 15, 16, 17, 19, 21, 24, 27, 30], even: None },
    Preset { id: "road-standard-11", name: "Road standard: 53/39 × 11–28 (11-speed)", chainrings: &[53, 39], cogs: &[11, 12, 13, 14, 15, 16, 17, 19, 21, 24, 28], even: None },
    Preset { id: "gravel-1x-12", name: "Gravel 1×: 40 × 10–44 (12-speed)", chainrings: &[40], cogs: &[10, 11, 13, 15, 17, 19, 21, 24, 28, 32, 38, 44], even: None },
    Preset { id: "mtb-1x-12", name: "Mountain bike 1×: 32 × 10–52 (12-speed)", chainrings: &[32], cogs: &[10, 12, 14, 16, 18, 21, 24, 28, 32, 36, 42, 52], even: None },
];

fn valid_teeth(t: &[u32]) -> Vec<u32> {
    t.iter().copied().filter(|t| (6..=90).contains(t)).collect()
}

/// Every chainring × cog combination, easiest first, with near-duplicates merged — the order a
/// rider shifting through the whole range would want (like sequential shifting).
fn from_teeth(chainrings: &[u32], cogs: &[u32]) -> Vec<Gear> {
    let (rings, cogs) = (valid_teeth(chainrings), valid_teeth(cogs));
    let mut all: Vec<(f64, u32, u32)> = rings.iter().flat_map(|&r| cogs.iter().map(move |&c| (r as f64 / c as f64, r, c))).collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut out: Vec<Gear> = Vec::new();
    for (ratio, r, c) in all {
        if out.last().is_some_and(|g| ratio < g.ratio * SAME_GEAR) {
            continue;
        }
        out.push(Gear { ratio, label: format!("{r}×{c}") });
    }
    out
}

/// The virtual gears a rider has set up, easiest first. Empty when virtual gears are off.
pub fn gears(s: &GearSettings) -> Vec<Gear> {
    if s.mode != GearsMode::Virtual {
        return Vec::new();
    }
    let preset = PRESETS.iter().find(|p| p.id == s.preset);
    match preset {
        Some(Preset { even: Some((n, lo, hi)), .. }) => {
            let n = (*n).max(2);
            (0..n).map(|i| Gear { ratio: lo * (hi / lo).powf(i as f64 / (n - 1) as f64), label: format!("Gear {}", i + 1) }).collect()
        }
        Some(p) => from_teeth(p.chainrings, p.cogs),
        None => from_teeth(&s.chainrings, &s.cogs),
    }
}

/// The ratio of the real gear the bike stays in.
pub fn real_ratio(s: &GearSettings) -> f64 {
    s.real_chainring.clamp(6, 90) as f64 / s.real_cog.clamp(6, 90) as f64
}

/// The gear to start in: the one set, or else the one closest to the real gear (0-based).
pub fn start_gear(s: &GearSettings, gears: &[Gear]) -> usize {
    if gears.is_empty() {
        return 0;
    }
    if (1..=gears.len()).contains(&s.start_gear) {
        return s.start_gear - 1;
    }
    let real = real_ratio(s);
    (0..gears.len()).min_by(|&a, &b| (gears[a].ratio / real).ln().abs().total_cmp(&(gears[b].ratio / real).ln().abs())).unwrap_or(0)
}

/// What the trainer is told, given the road gradient it would otherwise get.
pub struct Load {
    /// Gradient (fraction) the trainer applies.
    pub grade: f64,
    /// Rolling resistance coefficient it is told.
    pub crr: f64,
    /// Wind resistance coefficient (kg/m) it is told.
    pub cw: f64,
    /// Rider + bike, kg — what the trainer was configured with.
    pub mass: f64,
}

/// The gradient to send so the trainer feels like virtual gear `gear` at virtual speed `speed`
/// (m/s). The trainer's own force model is taken as `m·g·(grade + crr) + cw·v²` at its speed.
pub fn effective_grade(s: &GearSettings, gears: &[Gear], gear: usize, speed: f64, load: &Load) -> f64 {
    let Some(g) = gears.get(gear) else { return load.grade };
    let grade = match s.feel.as_str() {
        // A fixed step of gradient per gear either side of the real gear: predictable, for
        // trainers that behave oddly with the realistic version.
        "steps" => load.grade + s.step * (gear as f64 - start_gear(&GearSettings { start_gear: 0, ..s.clone() }, gears) as f64),
        _ => {
            let k = g.ratio / real_ratio(s);
            let mg = load.mass.max(30.0) * G;
            let v_trainer = speed / k;
            // m·g·(g_eff + crr) + cw·v_t² = k·(m·g·(g + crr) + cw·v²)
            k * (load.grade + load.crr) + (k * load.cw * speed * speed - load.cw * v_trainer * v_trainer) / mg - load.crr
        }
    };
    grade.clamp(MIN_GRADE, MAX_GRADE)
}

/// Cadence (rpm) for a road speed (m/s) in a gear, on a wheel of `wheel_mm` circumference.
pub fn cadence(speed: f64, ratio: f64, wheel_mm: f64) -> f64 {
    let wheel = (wheel_mm / 1000.0).clamp(1.0, 3.0);
    if ratio <= 0.0 {
        0.0
    } else {
        speed / (ratio * wheel) * 60.0
    }
}
