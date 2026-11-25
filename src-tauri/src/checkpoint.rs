//! Rides cut short: while riding, the ride so far is saved every half a minute, so if the app
//! quits (a crash, a power cut, the window closed by mistake) it can be carried on or saved
//! on the next launch.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use velopopuli_core::ride::{RideEngine, RideProgress};

use crate::now_ms;
use crate::session::{RideRequest, MIN_SAMPLES};
use crate::store::Store;

const KEY: &str = "ride-checkpoint";
/// How often the ride is saved while riding.
const EVERY: Duration = Duration::from_secs(30);
/// An older checkpoint can still be saved, but not carried on: the rider has moved on.
const CARRY_ON_FOR_MS: u64 = 3 * 24 * 3600 * 1000;

#[derive(Serialize, Deserialize)]
pub struct Checkpoint {
    pub request: RideRequest,
    pub route_name: String,
    /// Unix ms.
    pub saved_at: u64,
    pub progress: RideProgress,
}

/// What the launch prompt shows about a ride cut short.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointInfo {
    pub request: RideRequest,
    pub route_name: String,
    pub saved_at: u64,
    pub duration_s: f64,
    pub distance_m: f64,
    /// Recent enough to carry on; otherwise it can only be saved or discarded.
    pub can_carry_on: bool,
}

/// Checkpoints are kept except in automated runs, which are stopped part-way on purpose
/// (`VP_SMOKE_CHECKPOINT=1` keeps them, to check this).
pub fn enabled() -> bool {
    std::env::var_os("VP_SMOKE").is_none() || std::env::var_os("VP_SMOKE_CHECKPOINT").is_some()
}

pub fn load(store: &Store) -> Option<Checkpoint> {
    store.get(KEY)
}

pub fn info(store: &Store) -> Option<CheckpointInfo> {
    let c = load(store)?;
    Some(CheckpointInfo {
        can_carry_on: now_ms().saturating_sub(c.saved_at) < CARRY_ON_FOR_MS && !c.progress.finished,
        duration_s: c.progress.elapsed,
        distance_m: c.progress.ridden,
        route_name: c.route_name,
        saved_at: c.saved_at,
        request: c.request,
    })
}

pub fn clear(store: &Store) {
    let _ = store.del(KEY);
}

/// Saves the ride so far, once it's long enough to keep and whenever it has moved on.
fn save(store: &Store, request: &RideRequest, engine: &Mutex<RideEngine>, saved_samples: &mut usize) {
    let (progress, route_name) = {
        let engine = engine.lock().unwrap();
        if engine.samples.len() < MIN_SAMPLES || engine.samples.len() == *saved_samples {
            return;
        }
        (engine.progress(), engine.route.name.clone())
    };
    *saved_samples = progress.samples.len();
    let checkpoint = Checkpoint { request: request.clone(), route_name, saved_at: now_ms(), progress };
    if let Err(e) = store.set(KEY, &checkpoint) {
        crate::diag::event("ride", format!("checkpoint not saved: {e}"));
    }
}

/// Saves the ride every [`EVERY`] until `stop` is set.
pub fn spawn(dir: PathBuf, request: RideRequest, engine: Arc<Mutex<RideEngine>>, stop: Arc<AtomicBool>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let store = Store::new(dir);
        let mut saved_samples = 0;
        let mut last = Instant::now();
        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(250));
            if last.elapsed() >= EVERY {
                last = Instant::now();
                save(&store, &request, &engine, &mut saved_samples);
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use velopopuli_core::gpx::demo_route;
    use velopopuli_core::ride::RideInput;
    use velopopuli_core::types::Settings;

    #[test]
    fn a_ride_is_saved_once_long_enough_and_read_back() {
        let dir = std::env::temp_dir().join(format!("vp-checkpoint-{}", std::process::id()));
        let store = Store::new(dir.clone());
        let engine = Mutex::new(RideEngine::new(Arc::new(demo_route()), &Settings::default(), None));
        let request = RideRequest { route_id: "demo".into(), laps: Some(0), ..Default::default() };
        let ride = |seconds: u64, clock: &mut u64| {
            let mut e = engine.lock().unwrap();
            for _ in 0..seconds * 10 {
                *clock += 100;
                e.tick(0.1, RideInput { power: 200.0, cadence: Some(90.0), hr: None }, *clock);
            }
        };
        let mut clock = now_ms();
        let mut saved = 0;
        ride(30, &mut clock);
        save(&store, &request, &engine, &mut saved);
        assert!(info(&store).is_none(), "too short to keep yet");

        ride(60, &mut clock);
        save(&store, &request, &engine, &mut saved);
        let found = info(&store).expect("saved");
        assert!(found.can_carry_on);
        assert_eq!(found.request.laps, Some(0));
        assert!((found.duration_s - engine.lock().unwrap().elapsed).abs() < 1e-9);
        assert_eq!(load(&store).unwrap().progress.samples.len(), engine.lock().unwrap().samples.len());

        // Save what you rode: the checkpoint onto a fresh engine, saved like an ended ride.
        let mut fresh = RideEngine::new(Arc::new(demo_route()), &Settings::default(), None);
        fresh.resume(load(&store).unwrap().progress);
        let summary = crate::session::save_ride(&store, &fresh, &Settings::default(), "demo", false, &[]).unwrap().expect("long enough to keep");
        assert!((summary.duration_s - engine.lock().unwrap().elapsed).abs() < 1e-9);
        assert!(std::path::Path::new(&summary.file_path).exists(), "the FIT file is written");
        assert!(!summary.completed);

        clear(&store);
        assert!(info(&store).is_none());
        std::fs::remove_dir_all(dir).ok();
    }
}
