//! The simulation and recorder for one ride.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::fit::{FitLap, FitSample, FitSession, FitWorkout, FitWorkoutStep, LapTrigger};
use crate::fitness::power_curve;
use crate::gpx::{Climb, Route, RoutePos};
use crate::physics::{step_speed, wind_coefficient, PhysicsParams};
use crate::segments::{Segment, SegmentBest, SegmentSource, SPLIT_M};
use crate::types::{PersonalBest, Settings, TrainerSettings};
use crate::workout::{StepKind, Workout, WorkoutPos};
use crate::world::{Surface, Trail, FORD_MAX_SPEED, FORD_ROLLING};

#[derive(Debug, Clone, Copy, Default)]
pub struct RideInput {
    pub power: f64,
    pub cadence: Option<f64>,
    pub hr: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RivalKind {
    Ghost,
    Bot,
    /// Another rider in a group ride; their position arrives over the network.
    Remote,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rival {
    pub id: String,
    pub name: String,
    pub kind: RivalKind,
    pub color: u32,
    pub distance: f64,
    pub speed: f64,
    /// Bots hold a constant power; the ghost replays a recorded trace.
    #[serde(skip)]
    pub power: f64,
    #[serde(skip)]
    pub mass: f64,
    /// A ghost's distance at each elapsed second.
    #[serde(skip)]
    pub trace: Option<Arc<Vec<f64>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentResult {
    pub id: String,
    pub name: String,
    pub source: SegmentSource,
    pub time_s: f64,
    pub previous_best: Option<f64>,
}

/// The segment being ridden, for the live banner.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentLive {
    pub id: String,
    pub name: String,
    pub source: SegmentSource,
    pub to_go: f64,
    pub length: f64,
    pub elapsed: f64,
    /// Seconds behind (+) or ahead (−) of the best effort at this point.
    pub gap: Option<f64>,
    pub best: Option<f64>,
    pub record: Option<String>,
}

/// Someone else's ride on this route, replayed as a ghost.
#[derive(Debug, Clone)]
pub struct GhostTrace {
    pub id: String,
    pub name: String,
    pub color: u32,
    pub trace: Arc<Vec<f64>>,
}

/// Optional extras for a ride: a workout, extra ghosts and segments.
#[derive(Debug, Clone, Default)]
pub struct RideOptions {
    pub workout: Option<Workout>,
    pub ghosts: Vec<GhostTrace>,
    pub segments: Vec<Segment>,
    pub segment_bests: BTreeMap<String, SegmentBest>,
    /// Laps on a closed route: `None` rides it once, `Some(0)` goes round until stopped,
    /// `Some(n)` finishes after `n` laps. Ignored on a route that is not a loop.
    pub laps: Option<u32>,
}

#[derive(Debug, Clone)]
enum SegState {
    Waiting,
    Active { entered: f64, splits: Vec<f64> },
    Done,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClimbResult {
    pub index: usize,
    pub climb: Climb,
    pub time_s: f64,
    pub previous_best: Option<f64>,
}

/// Where a lap starts or ends in the recorded ride.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LapMark {
    sample: usize,
    elapsed: f64,
    ridden: f64,
    ascent: f64,
    descent: f64,
    /// The workout step being ridden at this point.
    workout_step: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LapDone {
    start: LapMark,
    end: LapMark,
    trigger: LapTrigger,
}

/// A lap just finished, for the ride screen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LapResult {
    /// From 1.
    pub number: usize,
    /// The rider pressed Lap (rather than a workout step or a loop ending it).
    pub manual: bool,
    pub time_s: f64,
    pub distance_m: f64,
    pub avg_power: f64,
    pub avg_hr: Option<f64>,
}

/// A ride so far, saved now and then so a ride cut short (a crash, a power cut) can be carried
/// on or saved. The running totals aren't kept: they're worked out again from the samples.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RideProgress {
    pub distance: f64,
    pub lap: u32,
    pub ridden: f64,
    pub elapsed: f64,
    pub ascent: f64,
    pub descent: f64,
    pub finished: bool,
    pub samples: Vec<FitSample>,
    pub climb_times: BTreeMap<usize, f64>,
    climbs_done: BTreeSet<usize>,
    pub new_segment_bests: BTreeMap<String, SegmentBest>,
    laps_done: Vec<LapDone>,
    lap_start: Option<LapMark>,
    pub workout_time: f64,
    pub intensity: f64,
    pub gear: usize,
    /// Where each pace rider was, by id.
    bots: BTreeMap<String, f64>,
}

/// A manual lap shorter than this is ignored, so a double press doesn't make an empty lap.
const MIN_MANUAL_LAP_S: usize = 3;

const BOT_NAMES: [&str; 5] = ["Tempo Tom", "Steady Sam", "Punchy Pia", "Diesel Dana", "Rocket Ro"];
const BOT_COLORS: [u32; 5] = [0x4aa3ff, 0xffb347, 0xc56bff, 0x4ddb8a, 0xff6b9a];

pub struct RideEngine {
    pub route: Arc<Route>,
    pub pb: Option<PersonalBest>,
    /// Position on the route, in `[0, route.total]`; on a lap ride it starts again at each lap.
    pub distance: f64,
    /// The lap being ridden, from 1.
    pub lap: u32,
    /// Total metres ridden. Equal to `distance` on a ride without laps.
    pub ridden: f64,
    /// Laps to ride (`Some(0)` until stopped), or `None` when the route is ridden once.
    pub laps: Option<u32>,
    pub speed: f64,
    /// Moving time in seconds; the clock stops when the rider does.
    pub elapsed: f64,
    pub moving: bool,
    pub user_paused: bool,
    pub finished: bool,
    pub power: f64,
    pub cadence: Option<f64>,
    pub hr: Option<f64>,
    pub ascent: f64,
    pub descent: f64,
    pub pos: RoutePos,
    pub rivals: Vec<Rival>,
    /// What the trainer was last asked to hold (None in simulation mode), recorded with each sample.
    pub target_power: Option<f64>,
    pub samples: Vec<FitSample>,
    /// Distance at each whole second of moving time — becomes the next ghost.
    pub trace: Vec<f64>,
    pub climb_times: BTreeMap<usize, f64>,
    pub climb_index: Option<usize>,
    pub last_climb_result: Option<ClimbResult>,
    pub segments: Vec<Segment>,
    pub segment_bests: BTreeMap<String, SegmentBest>,
    /// New bests set on this ride, by segment id.
    pub new_segment_bests: BTreeMap<String, SegmentBest>,
    pub last_segment_result: Option<SegmentResult>,
    pub workout: Option<Workout>,
    /// Moving time into the workout.
    pub workout_time: f64,
    /// Scales the workout's targets (1.0 = as written).
    pub intensity: f64,
    /// The virtual gear (index into `gearing::gears`), when virtual gears are on.
    pub gear: usize,
    /// Set on a shift, so the new resistance is sent straight away.
    pub gear_shifted: bool,
    seg_state: Vec<SegState>,
    /// Climbs already timed on this lap.
    climbs_done: BTreeSet<usize>,
    /// Laps finished so far: by the Lap button, a workout step ending, or a loop's line.
    laps_done: Vec<LapDone>,
    /// Where the lap being ridden started.
    lap_start: LapMark,
    /// The last lap finished, for the ride screen.
    pub last_lap: Option<LapResult>,

    params: PhysicsParams,
    /// The road surface at each route sample; empty means tarmac all the way.
    surfaces: Vec<Surface>,
    /// Where the route is on a trail, per route sample; empty means road all the way.
    trails: Vec<Option<Trail>>,
    /// Whether each route sample is in a ford; empty means none.
    fords: Vec<bool>,
    road_surfaces: bool,
    cda: f64,
    ftp: f64,
    acc: f64,
    last_sample_time: u64,
    climb_entered_at: f64,
    n: u32,
    sum_power: f64,
    max_power: f64,
    sum_hr: f64,
    n_hr: u32,
    max_hr: f64,
    sum_cad: f64,
    n_cad: u32,
    max_cad: f64,
    max_speed: f64,
    joules: f64,
    window: Vec<f64>,
    np4: f64,
    n_np: u32,
}

impl RideEngine {
    pub fn new(route: Arc<Route>, settings: &Settings, pb: Option<PersonalBest>) -> Self {
        Self::with_options(route, settings, pb, RideOptions::default())
    }

    pub fn with_options(route: Arc<Route>, settings: &Settings, pb: Option<PersonalBest>, options: RideOptions) -> Self {
        let params = PhysicsParams { mass: settings.rider.weight_kg + settings.rider.bike_kg, crr: settings.physics.crr, cda: settings.physics.cda, max_speed: settings.physics.max_speed_kph / 3.6 };
        let laps = options.laps.filter(|_| route.closed);
        let mut rivals = Vec::new();
        // A recorded trace covers one trip along the route, so there are no ghosts on a lap ride.
        if laps.is_none() && pb.as_ref().is_some_and(|p| p.trace.len() > 1) {
            let trace = Arc::new(pb.as_ref().unwrap().trace.clone());
            rivals.push(Rival { id: "ghost".into(), name: "Your best".into(), kind: RivalKind::Ghost, color: 0xffffff, distance: 0.0, speed: 0.0, power: 0.0, mass: 0.0, trace: Some(trace) });
        }
        for g in options.ghosts.iter().filter(|_| laps.is_none()) {
            rivals.push(Rival { id: g.id.clone(), name: g.name.clone(), kind: RivalKind::Ghost, color: g.color, distance: 0.0, speed: 0.0, power: 0.0, mass: 0.0, trace: Some(g.trace.clone()) });
        }
        if settings.bots.enabled {
            for (i, wkg) in settings.bots.wkg.iter().enumerate() {
                rivals.push(Rival {
                    id: format!("bot{i}"),
                    name: format!("{} · {:.1} W/kg", BOT_NAMES[i % BOT_NAMES.len()], wkg),
                    kind: RivalKind::Bot,
                    color: BOT_COLORS[i % BOT_COLORS.len()],
                    distance: 0.0,
                    speed: 0.0,
                    power: wkg * 75.0,
                    mass: 75.0 + 8.5,
                    trace: None,
                });
            }
        }
        let pos = route.sample_at(0.0);
        let first_step = options.workout.as_ref().and_then(|w| w.at(0.0)).map(|p| p.index);
        Self {
            route,
            pb,
            distance: 0.0,
            lap: 1,
            ridden: 0.0,
            laps,
            speed: 0.0,
            elapsed: 0.0,
            moving: false,
            user_paused: false,
            finished: false,
            power: 0.0,
            cadence: None,
            hr: None,
            ascent: 0.0,
            descent: 0.0,
            pos,
            rivals,
            samples: Vec::new(),
            target_power: None,
            trace: vec![0.0],
            climb_times: BTreeMap::new(),
            climb_index: None,
            last_climb_result: None,
            seg_state: vec![SegState::Waiting; options.segments.len()],
            climbs_done: BTreeSet::new(),
            laps_done: Vec::new(),
            lap_start: LapMark { sample: 0, elapsed: 0.0, ridden: 0.0, ascent: 0.0, descent: 0.0, workout_step: first_step },
            last_lap: None,
            segments: options.segments,
            segment_bests: options.segment_bests,
            new_segment_bests: BTreeMap::new(),
            last_segment_result: None,
            workout: options.workout,
            workout_time: 0.0,
            intensity: 1.0,
            gear: crate::gearing::start_gear(&settings.gears, &crate::gearing::gears(&settings.gears)),
            gear_shifted: false,
            params,
            surfaces: Vec::new(),
            trails: Vec::new(),
            fords: Vec::new(),
            road_surfaces: settings.physics.road_surfaces,
            cda: settings.physics.cda,
            ftp: settings.rider.ftp,
            acc: 0.0,
            last_sample_time: 0,
            climb_entered_at: 0.0,
            n: 0,
            sum_power: 0.0,
            max_power: 0.0,
            sum_hr: 0.0,
            n_hr: 0,
            max_hr: 0.0,
            sum_cad: 0.0,
            n_cad: 0,
            max_cad: 0.0,
            max_speed: 0.0,
            joules: 0.0,
            window: Vec::with_capacity(31),
            np4: 0.0,
            n_np: 0,
        }
    }

    /// Advances the ride by `dt` seconds. `wall_ms` is the current Unix time, used to stamp samples.
    pub fn tick(&mut self, dt: f64, input: RideInput, wall_ms: u64) {
        self.power = input.power;
        self.cadence = input.cadence;
        self.hr = input.hr;
        if self.finished || self.user_paused {
            self.moving = false;
            return;
        }
        // Each workout step is a lap of its own.
        if self.workout.is_some() && self.workout_pos().map(|p| p.index) != self.lap_start.workout_step {
            self.mark_lap(LapTrigger::Time);
        }
        // Hold the bike still until the rider pedals, even if the start points downhill.
        if self.speed < 0.3 && input.power < 5.0 {
            self.speed = 0.0;
            self.moving = false;
            return;
        }
        // After a stall (sleep, debugger) don't teleport down the road.
        let dt = dt.min(0.25);
        self.moving = true;

        let prev_ele = self.pos.ele;
        let params = self.params_at(self.distance, self.params.mass);
        self.speed = step_speed(self.speed, input.power, self.pos.grade, self.pos.ele, &params, dt);
        self.distance += self.speed * dt;
        self.ridden += self.speed * dt;
        self.elapsed += dt;
        if self.workout.is_some() {
            self.workout_time += dt;
        }
        if self.distance >= self.route.total {
            let overshoot = self.distance - self.route.total;
            self.distance = self.route.total;
            if self.lap_ride() && self.laps != Some(self.lap) {
                // Finish anything that ends at the line before the position wraps.
                self.pos = self.route.sample_at(self.distance);
                self.track_climbs();
                self.track_segments();
                self.distance = overshoot;
                self.mark_lap(LapTrigger::PositionLap);
                self.lap += 1;
                self.climbs_done.clear();
                self.climb_index = None;
                self.seg_state.fill(SegState::Waiting);
            } else {
                self.ridden -= overshoot;
                self.finished = true;
            }
        }
        self.pos = self.route.sample_at(self.distance);
        let d_ele = self.pos.ele - prev_ele;
        if d_ele > 0.0 {
            self.ascent += d_ele;
        } else {
            self.descent -= d_ele;
        }
        self.max_speed = self.max_speed.max(self.speed);

        self.step_rivals(dt);
        self.track_climbs();
        self.track_segments();

        self.acc += dt;
        // The epsilon keeps accumulated float error from delaying a sample by a tick.
        while self.acc >= 1.0 - 1e-9 {
            self.acc -= 1.0;
            self.record(wall_ms);
        }
        if self.finished {
            self.record(wall_ms);
        }
    }

    fn step_rivals(&mut self, dt: f64) {
        let lap_ride = self.lap_ride();
        let params: Vec<PhysicsParams> = self.rivals.iter().map(|r| self.params_at(r.distance, r.mass)).collect();
        for (r, params) in self.rivals.iter_mut().zip(params) {
            match r.kind {
                RivalKind::Ghost => {
                    let d = r.trace.as_deref().map_or(0.0, |t| trace_at(t, self.elapsed)).min(self.route.total);
                    r.speed = if dt > 0.0 { (d - r.distance) / dt } else { 0.0 };
                    r.distance = d;
                }
                RivalKind::Remote => {}
                RivalKind::Bot if lap_ride => {
                    let p = self.route.sample_at(r.distance);
                    r.speed = step_speed(r.speed, r.power, p.grade, p.ele, &params, dt);
                    r.distance = (r.distance + r.speed * dt).rem_euclid(self.route.total);
                }
                RivalKind::Bot if r.distance < self.route.total => {
                    let p = self.route.sample_at(r.distance);
                    r.speed = step_speed(r.speed, r.power, p.grade, p.ele, &params, dt);
                    r.distance = (r.distance + r.speed * dt).min(self.route.total);
                }
                RivalKind::Bot => r.speed = 0.0,
            }
        }
    }

    fn track_climbs(&mut self) {
        let climbs = &self.route.climbs;
        if let Some(i) = self.climb_index {
            let c = &climbs[i];
            if self.distance >= c.end {
                let time_s = self.elapsed - self.climb_entered_at;
                let fastest = self.climb_times.get(&i).map_or(time_s, |t| t.min(time_s));
                self.climb_times.insert(i, fastest);
                self.climbs_done.insert(i);
                self.last_climb_result = Some(ClimbResult { index: i, climb: c.clone(), time_s, previous_best: self.pb.as_ref().and_then(|p| p.climbs.get(&i).copied()) });
                self.climb_index = None;
            }
            return;
        }
        for (i, c) in climbs.iter().enumerate() {
            if !self.climbs_done.contains(&i) && self.distance >= c.start && self.distance < c.end {
                self.climb_index = Some(i);
                self.climb_entered_at = self.elapsed;
                return;
            }
        }
    }

    fn record(&mut self, wall_ms: u64) {
        // FIT timestamps must strictly increase even if two samples land in one wall-clock second.
        let time = (self.last_sample_time + 1000).max(wall_ms / 1000 * 1000);
        self.last_sample_time = time;
        let sample = FitSample {
            time,
            lat: self.pos.lat,
            lon: self.pos.lon,
            ele: self.pos.ele,
            dist: self.ridden,
            speed: self.speed,
            power: self.power.round(),
            hr: self.hr,
            cadence: self.cadence,
            target_power: self.target_power,
        };
        self.count(sample);
    }

    /// Adds a sample to the ride and its running totals.
    fn count(&mut self, sample: FitSample) {
        let (power, hr, cadence) = (sample.power, sample.hr, sample.cadence);
        self.trace.push(sample.dist);
        self.samples.push(sample);
        self.n += 1;
        self.sum_power += power;
        self.joules += power;
        self.max_power = self.max_power.max(power);
        if let Some(hr) = hr.filter(|h| *h > 0.0) {
            self.sum_hr += hr;
            self.n_hr += 1;
            self.max_hr = self.max_hr.max(hr);
        }
        if let Some(cad) = cadence.filter(|c| *c > 0.0) {
            self.sum_cad += cad;
            self.n_cad += 1;
            self.max_cad = self.max_cad.max(cad);
        }
        // Normalised power: 4th-power mean of the 30 s rolling average.
        self.window.push(power);
        if self.window.len() > 30 {
            self.window.remove(0);
        }
        if self.window.len() == 30 {
            let mean = self.window.iter().sum::<f64>() / 30.0;
            self.np4 += mean.powi(4);
            self.n_np += 1;
        }
    }

    fn track_segments(&mut self) {
        for (i, seg) in self.segments.iter().enumerate() {
            match &mut self.seg_state[i] {
                SegState::Waiting => {
                    // Only time a segment ridden from its start, not one joined part-way.
                    if self.distance >= seg.start && self.distance < seg.end && self.distance - seg.start < 40.0 {
                        self.seg_state[i] = SegState::Active { entered: self.elapsed, splits: vec![0.0] };
                    } else if self.distance >= seg.end {
                        self.seg_state[i] = SegState::Done;
                    }
                }
                SegState::Active { entered, splits } => {
                    let offset = (self.distance - seg.start).min(seg.end - seg.start);
                    while (splits.len() as f64) * SPLIT_M <= offset {
                        splits.push(self.elapsed - *entered);
                    }
                    if self.distance >= seg.end {
                        let time_s = self.elapsed - *entered;
                        splits.push(time_s);
                        let previous_best = self.segment_bests.get(&seg.id).map(|b| b.time_s).filter(|t| *t > 0.0);
                        // Within a lap ride the bar to beat includes this ride's earlier laps.
                        let bar = [previous_best, self.new_segment_bests.get(&seg.id).map(|b| b.time_s)].into_iter().flatten().reduce(f64::min);
                        if bar.is_none_or(|b| time_s < b) {
                            self.new_segment_bests.insert(seg.id.clone(), SegmentBest { time_s, splits: std::mem::take(splits) });
                        }
                        self.last_segment_result = Some(SegmentResult { id: seg.id.clone(), name: seg.name.clone(), source: seg.source, time_s, previous_best });
                        self.seg_state[i] = SegState::Done;
                    }
                }
                SegState::Done => {}
            }
        }
    }

    /// The segment being ridden that started most recently.
    pub fn segment_live(&self) -> Option<SegmentLive> {
        let (i, entered) = self
            .seg_state
            .iter()
            .enumerate()
            .filter_map(|(i, s)| if let SegState::Active { entered, .. } = s { Some((i, *entered)) } else { None })
            .max_by(|a, b| a.1.total_cmp(&b.1).then(self.segments[b.0].end.total_cmp(&self.segments[a.0].end)))?;
        let seg = &self.segments[i];
        let elapsed = self.elapsed - entered;
        let best = self.segment_bests.get(&seg.id);
        Some(SegmentLive {
            id: seg.id.clone(),
            name: seg.name.clone(),
            source: seg.source,
            to_go: (seg.end - self.distance).max(0.0),
            length: seg.end - seg.start,
            elapsed,
            gap: best.and_then(|b| b.at(self.distance - seg.start)).map(|t| elapsed - t),
            best: best.map(|b| b.time_s).filter(|t| *t > 0.0),
            record: seg.record.clone(),
        })
    }

    /// Where the workout is now, if one is loaded and not yet over.
    pub fn workout_pos(&self) -> Option<WorkoutPos> {
        self.workout.as_ref()?.at(self.workout_time)
    }

    /// Watts the trainer should hold for the workout, or `None` when the gradient applies.
    pub fn workout_target(&self) -> Option<f64> {
        let pos = self.workout_pos()?;
        (pos.kind == StepKind::Power).then(|| pos.target * self.ftp * self.intensity)
    }

    pub fn set_ftp(&mut self, ftp: f64) {
        self.ftp = ftp;
    }

    /// Best efforts of this ride so far, for the power curve.
    pub fn power_curve(&self) -> Vec<f64> {
        power_curve(&self.samples.iter().map(|s| s.power).collect::<Vec<_>>())
    }

    /// Sets a group-ride partner's position (adds them if new).
    pub fn set_remote(&mut self, id: &str, name: &str, color: u32, distance: f64, speed: f64) {
        let distance = distance.clamp(0.0, self.route.total);
        if let Some(r) = self.rivals.iter_mut().find(|r| r.id == id) {
            r.distance = distance;
            r.speed = speed;
            r.name = name.to_string();
        } else {
            self.rivals.push(Rival { id: id.into(), name: name.into(), kind: RivalKind::Remote, color, distance, speed, power: 0.0, mass: 0.0, trace: None });
        }
    }

    pub fn remove_remote(&mut self, id: &str) {
        self.rivals.retain(|r| !(r.kind == RivalKind::Remote && r.id == id));
    }

    /// Moving time spent on the climb currently being ridden.
    pub fn climb_elapsed(&self) -> f64 {
        if self.climb_index.is_some() {
            self.elapsed - self.climb_entered_at
        } else {
            0.0
        }
    }

    /// The rider pressed Lap: ends the lap being ridden without ending the ride. Returns false
    /// when the lap so far is too short to keep.
    pub fn lap(&mut self) -> bool {
        self.mark_lap(LapTrigger::Manual)
    }

    /// How many laps have been finished.
    pub fn laps_finished(&self) -> usize {
        self.laps_done.len()
    }

    /// The ride so far, to save as a checkpoint.
    pub fn progress(&self) -> RideProgress {
        RideProgress {
            distance: self.distance,
            lap: self.lap,
            ridden: self.ridden,
            elapsed: self.elapsed,
            ascent: self.ascent,
            descent: self.descent,
            finished: self.finished,
            samples: self.samples.clone(),
            climb_times: self.climb_times.clone(),
            climbs_done: self.climbs_done.clone(),
            new_segment_bests: self.new_segment_bests.clone(),
            laps_done: self.laps_done.clone(),
            lap_start: Some(self.lap_start),
            workout_time: self.workout_time,
            intensity: self.intensity,
            gear: self.gear,
            bots: self.rivals.iter().filter(|r| r.kind == RivalKind::Bot).map(|r| (r.id.clone(), r.distance)).collect(),
        }
    }

    /// Carries on from a checkpoint, on a freshly set-up engine for the same route. The clock and
    /// the file carry on where they stopped; the gap reads like a pause.
    pub fn resume(&mut self, p: RideProgress) {
        let total = self.route.total;
        let last = p.samples.len();
        // Leave out anything that doesn't fit this route any more (it was edited, say).
        let fits = |m: &LapMark| m.sample <= last;
        self.distance = p.distance.clamp(0.0, total);
        self.lap = p.lap.max(1);
        self.ridden = p.ridden.max(0.0);
        self.elapsed = p.elapsed.max(0.0);
        self.ascent = p.ascent;
        self.descent = p.descent;
        self.finished = p.finished;
        self.pos = self.route.sample_at(self.distance);
        let climbs = self.route.climbs.len();
        self.climb_times = p.climb_times.into_iter().filter(|(i, _)| *i < climbs).collect();
        self.climbs_done = p.climbs_done.into_iter().filter(|i| *i < climbs).collect();
        // A climb half ridden can't be timed fairly any more.
        for (i, c) in self.route.climbs.iter().enumerate() {
            if self.distance > c.start && self.distance < c.end {
                self.climbs_done.insert(i);
            }
        }
        self.climb_index = None;
        // Segments already finished stay finished; one half ridden isn't timed.
        self.seg_state = self.segments.iter().map(|s| if self.distance > s.start { SegState::Done } else { SegState::Waiting }).collect();
        self.new_segment_bests = p.new_segment_bests;
        self.laps_done = p.laps_done.into_iter().filter(|l| fits(&l.start) && fits(&l.end)).collect();
        if let Some(start) = p.lap_start.filter(fits) {
            self.lap_start = start;
        }
        if self.workout.is_some() {
            self.workout_time = p.workout_time.max(0.0);
        }
        if p.intensity > 0.0 {
            self.intensity = p.intensity;
        }
        self.gear = p.gear;
        for r in self.rivals.iter_mut().filter(|r| r.kind == RivalKind::Bot) {
            if let Some(d) = p.bots.get(&r.id) {
                r.distance = d.clamp(0.0, total);
            }
        }
        // The running totals, from the samples.
        self.samples.clear();
        self.trace = vec![0.0];
        for s in p.samples {
            self.max_speed = self.max_speed.max(s.speed);
            self.count(s);
        }
        self.last_sample_time = self.samples.last().map_or(0, |s| s.time);
        self.acc = 0.0;
        self.speed = 0.0;
    }

    fn mark_here(&self) -> LapMark {
        LapMark { sample: self.samples.len(), elapsed: self.elapsed, ridden: self.ridden, ascent: self.ascent, descent: self.descent, workout_step: self.workout_pos().map(|p| p.index) }
    }

    /// Ends the lap being ridden and starts the next one here.
    fn mark_lap(&mut self, trigger: LapTrigger) -> bool {
        let here = self.mark_here();
        let start = self.lap_start;
        let min = if trigger == LapTrigger::Manual { MIN_MANUAL_LAP_S } else { 1 };
        if here.sample < start.sample + min {
            // Nothing ridden since the last lap (a step skipped straight away): this lap
            // carries on, but belongs to the step the rider is on now.
            self.lap_start.workout_step = here.workout_step;
            return false;
        }
        let ridden = &self.samples[start.sample..here.sample];
        let hr: Vec<f64> = ridden.iter().filter_map(|p| p.hr).filter(|h| *h > 0.0).collect();
        self.laps_done.push(LapDone { start, end: here, trigger });
        self.last_lap = Some(LapResult {
            number: self.laps_done.len(),
            manual: trigger == LapTrigger::Manual,
            time_s: here.elapsed - start.elapsed,
            distance_m: here.ridden - start.ridden,
            avg_power: ridden.iter().map(|p| p.power).sum::<f64>() / ridden.len() as f64,
            avg_hr: (!hr.is_empty()).then(|| hr.iter().sum::<f64>() / hr.len() as f64),
        });
        self.lap_start = here;
        true
    }

    /// The ride's laps for the FIT file: the finished ones, then the one ridden at the end.
    fn fit_laps(&self) -> Vec<FitLap> {
        let lap = |start: LapMark, end: LapMark, trigger| FitLap {
            samples: start.sample..end.sample,
            timer_s: end.elapsed - start.elapsed,
            distance_m: end.ridden - start.ridden,
            ascent_m: end.ascent - start.ascent,
            descent_m: end.descent - start.descent,
            trigger,
            workout_step: start.workout_step,
        };
        let mut laps: Vec<FitLap> = self.laps_done.iter().map(|l| lap(l.start, l.end, l.trigger)).collect();
        let here = self.mark_here();
        if laps.is_empty() || here.sample > self.lap_start.sample {
            laps.push(lap(self.lap_start, here, LapTrigger::SessionEnd));
        }
        laps
    }

    /// Whether this ride goes round a loop for laps.
    pub fn lap_ride(&self) -> bool {
        self.laps.is_some()
    }

    /// Whether finishing this ride can set the route's best time: only a single complete trip does.
    pub fn counts_for_route_best(&self) -> bool {
        self.finished && !self.lap_ride()
    }

    /// Seconds behind (+) or ahead (−) of the personal-best ghost at the current position.
    pub fn ghost_gap(&self) -> Option<f64> {
        if self.lap_ride() {
            return None;
        }
        let trace = &self.pb.as_ref()?.trace;
        if trace.len() < 2 {
            return None;
        }
        let mut hi = trace.len() - 1;
        if self.distance >= trace[hi] {
            return Some(self.elapsed - hi as f64);
        }
        let mut lo = 0;
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if trace[mid] <= self.distance {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let span = trace[hi] - trace[lo];
        let ghost_t = lo as f64 + if span > 0.0 { (self.distance - trace[lo]) / span } else { 0.0 };
        Some(self.elapsed - ghost_t)
    }

    /// Gradient to send to the trainer: scaled, softened downhill and capped as the settings say.
    /// The gradient to send the trainer. A rough surface's extra rolling resistance is added
    /// as gradient (the same force, `m·g·Δcrr`), since every trainer follows the gradient.
    pub fn trainer_grade(&self, trainer: &TrainerSettings) -> f64 {
        if self.finished || self.user_paused {
            0.0
        } else {
            trainer.trainer_grade(self.pos.grade) + self.crr_at(self.distance) - self.params.crr
        }
    }

    /// Sets the road surface at each route sample, as the world was built.
    pub fn set_surfaces(&mut self, surfaces: Vec<Surface>) {
        self.surfaces = surfaces;
    }

    /// Sets where the route is on a trail, as the world was built.
    pub fn set_trails(&mut self, trails: Vec<Option<Trail>>) {
        self.trails = trails;
    }

    /// Where the route fords streams: whether each route sample is in a ford.
    pub fn set_fords(&mut self, fords: Vec<bool>) {
        self.fords = fords;
    }

    /// Whether the rider is riding through a ford.
    pub fn ford(&self) -> bool {
        self.ford_at(self.distance)
    }

    fn ford_at(&self, distance: f64) -> bool {
        let i = (distance / self.route.step).round().max(0.0) as usize;
        self.fords.get(i).copied().unwrap_or(false)
    }

    /// The trail the rider is on, if any.
    pub fn trail(&self) -> Option<Trail> {
        self.trail_at(self.distance)
    }

    fn trail_at(&self, distance: f64) -> Option<Trail> {
        let i = (distance / self.route.step).round().max(0.0) as usize;
        self.trails.get(i.min(self.trails.len().saturating_sub(1))).copied().flatten()
    }

    /// The road surface at the rider's position.
    pub fn surface(&self) -> Surface {
        self.surface_at(self.distance)
    }

    fn surface_at(&self, distance: f64) -> Surface {
        let i = (distance / self.route.step).round().max(0.0) as usize;
        self.surfaces.get(i.min(self.surfaces.len().saturating_sub(1))).copied().unwrap_or_default()
    }

    /// The physics for a rider of `mass` at a distance along the route: the road's rolling
    /// resistance, and a graded trail's speed limit.
    fn params_at(&self, distance: f64, mass: f64) -> PhysicsParams {
        let trail_limit = self.trail_at(distance).and_then(Trail::max_speed).unwrap_or(f64::INFINITY);
        let ford_limit = if self.ford_at(distance) { FORD_MAX_SPEED } else { f64::INFINITY };
        PhysicsParams { mass, crr: self.crr_at(distance), max_speed: self.params.max_speed.min(trail_limit).min(ford_limit), ..self.params }
    }

    /// Rolling resistance at a distance along the route.
    fn crr_at(&self, distance: f64) -> f64 {
        if self.road_surfaces {
            // Water and stones in a ford; a graded trail's roots and rocks; otherwise what it's made of.
            let trail = || self.trail_at(distance).and_then(Trail::rolling_factor).unwrap_or_else(|| self.surface_at(distance).rolling_factor());
            let factor = if self.ford_at(distance) { FORD_ROLLING.max(trail()) } else { trail() };
            self.params.crr * factor
        } else {
            self.params.crr
        }
    }

    pub fn wind_coefficient(&self) -> f64 {
        wind_coefficient(self.cda, self.pos.ele)
    }

    pub fn avg_power(&self) -> f64 {
        if self.n > 0 {
            self.sum_power / self.n as f64
        } else {
            0.0
        }
    }

    pub fn np(&self) -> f64 {
        if self.n_np > 0 {
            (self.np4 / self.n_np as f64).powf(0.25)
        } else {
            self.avg_power()
        }
    }

    pub fn kj(&self) -> f64 {
        self.joules / 1000.0
    }

    pub fn tss(&self) -> f64 {
        if self.ftp <= 0.0 || self.elapsed <= 0.0 {
            return 0.0;
        }
        let np = self.np();
        self.elapsed * np * (np / self.ftp) / (self.ftp * 3600.0) * 100.0
    }

    pub fn session(&self, now_ms: u64, utc_offset_s: i64) -> FitSession {
        FitSession {
            start_time: self.samples.first().map_or(now_ms, |s| s.time),
            end_time: self.samples.last().map_or(now_ms, |s| s.time),
            timer_s: self.elapsed,
            distance_m: self.ridden,
            ascent_m: self.ascent,
            descent_m: self.descent,
            avg_power: self.avg_power(),
            max_power: self.max_power,
            np: self.np(),
            avg_hr: if self.n_hr > 0 { self.sum_hr / self.n_hr as f64 } else { 0.0 },
            max_hr: self.max_hr,
            avg_cadence: if self.n_cad > 0 { self.sum_cad / self.n_cad as f64 } else { 0.0 },
            max_cadence: self.max_cad,
            max_speed: self.max_speed,
            kj: self.kj(),
            utc_offset_s,
            laps: self.fit_laps(),
            workout: self.workout.as_ref().map(|w| FitWorkout { name: w.name.clone(), steps: w.steps.iter().map(fit_step).collect() }),
        }
    }
}

/// A workout step for the FIT file, named for what it asks of the rider.
fn fit_step(step: &crate::workout::Step) -> FitWorkoutStep {
    let pct = |f: f64| (f * 100.0).round();
    let (name, power_pct) = match step.kind {
        StepKind::Power if (step.from - step.to).abs() < 0.005 => (format!("{}% FTP", pct(step.from)), Some((pct(step.from), pct(step.to)))),
        StepKind::Power => (format!("Ramp {}-{}% FTP", pct(step.from), pct(step.to)), Some((pct(step.from.min(step.to)), pct(step.from.max(step.to))))),
        StepKind::Free => ("Free ride".to_string(), None),
        StepKind::Max => ("Max effort".to_string(), None),
    };
    FitWorkoutStep { name, duration_s: step.duration_s, power_pct }
}

/// A recorded trace's distance at `t` seconds, interpolated.
fn trace_at(trace: &[f64], t: f64) -> f64 {
    if trace.is_empty() {
        return 0.0;
    }
    let i = t.max(0.0).floor() as usize;
    if i + 1 >= trace.len() {
        return trace[trace.len() - 1];
    }
    trace[i] + (trace[i + 1] - trace[i]) * (t - i as f64)
}
