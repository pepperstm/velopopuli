# The ride

One ride in progress is a `RideEngine` ([`crates/core/src/ride.rs`](../../crates/core/src/ride.rs)), wrapped by a `Session` ([`src-tauri/src/session.rs`](../../src-tauri/src/session.rs)), which owns the thread, talks to the devices and saves the result. The loop's timing is in [Architecture](architecture.md#from-ride-to-the-summary).

## The engine

`RideEngine::tick(dt, input, wall_ms)` is called every 20 ms with the merged power, cadence and heart rate. In order, it:

1. **Holds still** if paused or finished, or if the rider isn't pedalling from a standstill.
2. **Advances the rider:** physics at the current sample (`params_at`: the surface's rolling resistance, any trail or ford speed limit), then `distance`, `ridden` and `elapsed`.
3. **Handles the end of the route:** on a lap ride it finishes anything ending at the line, wraps `distance`, and starts the next lap. Otherwise it stops at `total` and marks the ride finished.
4. **Updates** position, ascent and descent.
5. **Moves the rivals**, then **times climbs and segments**.
6. **Records** one sample per second (and a last one at the finish).

What the engine keeps:

| Field | Meaning |
|---|---|
| `distance` | Where the rider is on the route, `0…total`. On a lap ride, where they are on the loop. |
| `ridden` | How far they've ridden in all. The same as `distance` unless it's a lap ride. |
| `lap`, `laps` | The current lap (from 1), and how many were asked for (`None`: not a lap ride; `Some(0)`: until stopped) |
| `speed`, `elapsed`, `moving`, `user_paused`, `finished` | |
| `pos` | The sampled position, height and gradient at `distance` |
| `rivals` | Pace riders, ghosts and group riders |
| `samples`, `trace` | The recording, and `ridden` at each whole second (the ride's own ghost if it's a best) |
| `gear`, `intensity`, `workout_time` | Virtual gear, workout intensity (1.0 = as written), time into the workout |

The rule for laps: **anything meaning "where on the road" uses `distance`; anything meaning "how far ridden" (FIT, summary, goals, badges) uses `ridden`.** Without laps they're equal, so ordinary rides are unaffected by construction.

## Rivals

| Kind | How it moves |
|---|---|
| **Pace riders** (`Bot`) | Simulated with the same physics at a fixed power: W/kg × 75 kg, on 83.5 kg of rider and bike. Defaults 1.5, 2.2 and 3.0 W/kg, named from a fixed list ("Tempo Tom", "Steady Sam", …). On a lap ride they go round and round. |
| **Ghosts** (`Ghost`) | Replay a trace: distance along the route at each second, interpolated (`trace_at`). Speed is the trace's slope. "Your best" is the route's personal best; others are ghosts the rider imported. |
| **Group riders** (`Remote`) | Positions from the relay (`set_remote`, `remove_remote`) |

**Ghosts are off on lap rides**: a trace covers one trip along the route. The live gap to your best (`ghost_gap`) is found by searching the best trace for where it was at your distance.

### Importing a ghost

`ghost_trace` ([`segments.rs`](../../crates/core/src/segments.rs)) turns a recorded ride (timestamped GPS points) into a trace for this route:

1. **Follow the track along the route** (`follow_track`): each point gets the nearest distance along the route within 40 m. Distances never go backwards, so a route passing the same place twice is followed in order. Where the track first meets the route, the earliest pass counts, so a ride starting at a loop's line is at the start, not the finish.
2. **Find the start:** the ride must come within 150 m of the start. The trace begins where it last leaves the first 5 m.
3. **Sample every second.** The ghost ends where the track leaves the route for more than 30 seconds.

A ghost from a GPX or FIT file is `ghost_import_file`; from a connector, `ghost_import_connector`. Before downloading full tracks, a connector's list of rides is sorted with each ride's simplified outline (`outline_follow_distance`): rides that follow at least half the route (or a kilometre) from its start are listed first. Ghosts are stored per route as `ghosts-<route id>`.

## Climbs and segments

Both are `Segment`s with a start and end on the route:

- **Climbs** are the route's own (see [Routes](routes.md#climbs)), named "Climb 2 · Cat 1" and so on.
- **Remote segments** are found by a connector along the route (`segments_fetch`), such as Strava segments, and stored as `segments-<route id>`. `place_segment` puts a segment's polyline on the route: its start must be within 25 m, every point within 30 m, its end close, and its length within 15% (plus 20 m) of the segment's own.

Timing (`track_segments`):

- A segment is only timed if the rider **enters within 40 m of its start**, so one joined part-way isn't.
- While on it, the elapsed time is noted every 10 m (`SPLIT_M`). A best keeps these splits, so the next attempt shows a live gap to it at every point (`segment_live`).
- At the end the result is shown for 9 seconds. A new best is kept if it beats the stored best and, on a lap ride, this ride's earlier laps.
- **Climbs** are timed the same way, by entering and leaving them (`track_climbs`), with the fastest per climb kept.

On a lap ride everything is timed again each lap. A climb or segment ending exactly on the line must finish on the crossing tick, before `distance` wraps.

## Laps

Laps are offered only on a closed route (`Route.closed`, asked through `route_is_loop`), and the engine ignores the setting otherwise.

- `distance` wraps from `total` back to the overshoot at the line; `lap` goes up; climbs and segments reset.
- The ride finishes after the last lap, or never with `Some(0)`.
- A lap ride **never sets the route's best time** (`counts_for_route_best`), and has no ghosts.

Anything comparing distances on a lap ride must allow for the wrap. That includes the chase camera, rivals just behind the rider, the barriers and the profile behind them, and a group rider sitting at exactly 0 or `total`. The page has `wrapDist` and `wrapGap` for this (`web/src/core/gpx.ts`).

## Recording

Every second, `record` appends a `FitSample`: time, position, height, `ridden`, speed, power (rounded), heart rate and cadence. Timestamps strictly increase even if two samples fall in the same wall-clock second, as FIT requires. Alongside, it keeps running totals:

- average and maximum power, heart rate and cadence (zeros excluded), and the maximum speed;
- **energy** (kJ) as the sum of power;
- **normalised power**: the fourth-power mean of the 30-second rolling average;
- **TSS**: `elapsed × NP × (NP / FTP) / (FTP × 3600) × 100`.

[`fit.rs`](../../crates/core/src/fit.rs) writes the FIT file: `file_id` (activity), timer start and stop events, one record per sample, one lap, the session totals and the activity, with the local time offset. Laps on a lap ride aren't written as separate FIT laps yet. [`crates/core/examples/fitinfo.rs`](../../crates/core/examples/fitinfo.rs) prints a FIT file with an independent decoder, for checking.

## The end of a ride

`Session::finish`:

1. **Too short?** Under 60 samples (a minute), nothing is saved and `None` comes back. The page warns before that ([`Ride.tsx`](../../web/src/ui/Ride.tsx), `leaving`).
2. **The FIT file** goes to `activities/velopopuli-<YYYY-MM-DD-HHMM>-<route slug>.fit` in the data folder, and is copied to the export folder if one is set.
3. **Personal bests** (`pb-<route id>`): if it counts for the route's best (not a lap ride) and beats the stored time, it becomes the best, and its trace becomes the ghost. Climb and segment bests are merged in either way. The `bests` key keeps each route's best time for the library.
4. **XP and badges** ([`gamify.rs`](../../crates/core/src/gamify.rs)) from the ride's `RideFacts`:
   - XP = 10 a km + 1 per 5 m climbed + bonuses: completing the route 50, a personal best 100, a finished workout 75, a group ride 50, 25 for each segment best (up to 10), reaching the season goal 500.
   - Level `L` starts at `200 × (L − 1)²` XP.
   - 23 badges (`BADGES`), each with a rule in `earned`: first kilometre, a whole route, an hour, 500 m and 1,500 m climbed, a Cat 1 or HC climb, ten different routes, streaks of 4 and 12 weeks, and so on.
5. **The history** (`rides`): a `RideSummary` goes on the front of the list, which is kept to 1,000 rides.
6. **An FTP estimate** is offered if this ride suggests a higher one (below).

Back on the summary screen, connectors set to upload automatically get the ride straight away ([`Summary.tsx`](../../web/src/ui/Summary.tsx), `UploadButtons`). The others show an upload button. Each upload is recorded in `RideSummary.uploads` by connector id (see [Connectors](connectors-and-services.md#uploads)).

## Streaks and the season goal

- **Streaks** (`streaks`): consecutive weeks with a ride (weeks start on Monday), ending this week or last, and consecutive days ending today or yesterday.
- **The season goal** (`Goal`): a distance by a deadline, 1,000 km by 20 March by default. Progress counts rides from the goal's start (`library::goal_progress`).

## Fitness

[`fitness.rs`](../../crates/core/src/fitness.rs):

- **Power curve:** best average power over 5, 15 and 30 s, 1, 2, 5, 10, 20 and 40 minutes and an hour, worked out with prefix sums.
- **FTP estimate:** the highest of 95% of the best 20 minutes and the best hour, or, after the ramp test, 75% of its best minute. It's offered when it's more than 2 W above the rider's FTP, or always after a completed ramp test.
- **Power zones:** Coggan's seven, at 55, 75, 90, 105, 120 and 150% of FTP.
- **Training load:** fitness (CTL) and fatigue (ATL) as exponentially weighted daily TSS over 42 and 7 days. Form is fitness minus fatigue, as it stood that morning.

`fitness_overview` gathers it for the Progress page from the stored rides' curves and TSS.

## Workouts

[`workout.rs`](../../crates/core/src/workout.rs):

- **Targets are fractions of FTP**, so a workout scales to the rider. A step is steady, a ramp from one target to another, or free ride.
- **Zwift `.zwo`:** `SteadyState`, `Warmup`, `Cooldown`, `Ramp`, `IntervalsT` and `FreeRide`, with text cues.
- **`.erg` / `.mrc`:** minutes against watts (converted with the rider's FTP) or against percent.
- **Built in:** endurance hour, sweet spot, threshold, VO₂max 5 × 3, over-unders, sprints and the FTP ramp test (one-minute rungs, each 6.25% of FTP harder, until the rider stops).
- During a ride `workout_pos` gives the step and the target. `↑` and `↓` change `intensity` by 5%, and **Skip step** moves `workout_time` to the next step.
- Imported workouts are stored in the library ([`library.rs`](../../src-tauri/src/library.rs)).

With **Hold workout targets** on (the default), the trainer holds each target in ERG. With it off, the targets are shown and the gradient drives the trainer.
