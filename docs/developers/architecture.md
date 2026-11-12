# Architecture

## Three layers

```
┌──────────────────────────────────────────────────────────────────────────┐
│ web/  (TypeScript: React screens, three.js world)                        │
│   draws what it's given · sends what the rider pressed                    │
└───────────────▲──────────────────────────────┬───────────────────────────┘
     events      │ ride:state 20×/s, devices…    │ commands (invoke)
                 │                               ▼
┌───────────────┴──────────────────────────────────────────────────────────┐
│ src-tauri/  (the app)                                                     │
│   commands · ride session (50 Hz) · Bluetooth/ANT+ · downloads + caches  │
│   connectors · group rides · fan & lights · diagnostics · native renderer │
└───────────────┬──────────────────────────────────────────────────────────┘
                │ plain function calls
                ▼
┌──────────────────────────────────────────────────────────────────────────┐
│ crates/core/  (pure computation, no I/O)                                  │
│   gpx · physics · ride · workout · fitness · gamify · segments · fit      │
│   gearing · waymarked · velodrome · devices (protocols) · world (3D)      │
└──────────────────────────────────────────────────────────────────────────┘
```

- **`crates/core`** has no I/O at all: no files, network, clocks or threads. Everything takes data in and gives data back, so it can all be unit-tested and run anywhere. Time comes in as arguments (`wall_ms`, `now_ms`).
- **`src-tauri`** is the app. It owns the window, the data folder, the network, the Bluetooth radio and the USB stick. It feeds the core and hands its results to the page. Most modules are thin adapters; [`commands.rs`](../../src-tauri/src/commands.rs) is the list of everything the page can ask for.
- **`web`** draws the screens and the 3D world. It holds no ride logic: speed, distance, gradient, timing and scoring all arrive from Rust. Its only maths is presentation: gliding between updates, placing the camera, posing the rider.

## Where things are

```
crates/core/src/
  gpx.rs            GPX → resampled route; smoothing, gradients, climbs, loops       → Routes
  physics.rs        Speed from power; air, rolling and slope                          → Physics
  gearing.rs        Virtual gears: gear sets, the gradient that makes a gear felt     → Physics
  ride.rs           The ride engine: rivals, laps, climbs, segments, recording        → The ride
  workout.rs        .zwo, .erg and .mrc workouts; built-in sessions                   → The ride
  fitness.rs        Power curve, FTP estimate, zones, training load                   → The ride
  gamify.rs         XP, levels, badges, streaks                                       → The ride
  segments.rs       Other rides and segments matched onto the route; ghosts; photos   → The ride
  fit.rs            FIT activity encoder                                              → The ride
  waymarked.rs      Signposted and MTB route relations joined into one line           → Routes
  velodrome.rs      Built-in tracks (Herne Hill) as routes                            → Routes
  types.rs          Settings and stored shapes shared with the app
  devices/          Bluetooth and ANT+ protocols, OpenBikeControl, the sensor hub     → Devices
  world/            The 3D world                                                      → The world
    dem.rs            elevation tiles          height.rs     the ground's height
    mod.rs            chunks, far sheet, road  scenery.rs    map matching, forests, water
    surface.rs        surfaces, widths, trails roads.rs      other roads on the ground
    structures.rs     bridges, tunnels         buildings.rs  footprints → blocks
    trees.rs          kinds and placement      fields.rs     hedges, walls, fences
    barriers.rs       crowd barriers, netting  landmarks.rs  signs along the route
    berms.rs          banked trail turns       fords.rs      trails through streams
    venue.rs          the velodrome            mvt.rs        vector-tile decoding
crates/relay/       The group-ride relay (WebSocket, nothing stored)                  → Connectors
src-tauri/src/
  main.rs           Start-up, AppState, the command list
  commands.rs       Everything the page can call
  session.rs        Ride start, the 50 Hz loop, trainer commands, saving              → The ride
  devices/          Bluetooth (btleplug), ANT+ (libusb), the demo rider               → Devices
  terrain.rs        Downloading elevation tiles                                       → The world
  mapdata.rs        Downloading and caching map data                                  → The world
  connectors/       Strava, Dreeve                                                    → Connectors
  group.rs          The group-ride client                                             → Connectors
  geo.rs            Place search, route planning, waymarked routes                    → Connectors
  photos.rs smart.rs issues.rs   Street photos; fan and lights; GitHub reports        → Connectors
  library.rs        Ghosts, segments, workouts, goal progress in the store
  store.rs secrets.rs diag.rs    Data folder; keychain; diagnostics                   → Data
  native/           The native renderer (wgpu → Metal)                                → Drawing
  snapshot.rs       Window pictures for the smoke harness                             → Building
web/src/
  bridge.ts         The only place that calls Rust
  types.ts          The shapes Rust sends, mirrored
  ui/               Screens (React)                                                   → Drawing
  world/            The three.js scene, ground, trees, rider, weather, native assets  → Drawing
  core/gpx.ts       Sampling the route on a curve, lean, lap wrapping                 → Routes
```

## How the page and Rust talk

All calls from the page go through one file, [`web/src/bridge.ts`](../../web/src/bridge.ts). Nothing else calls `invoke`.

- **Commands** are Tauri commands (`#[tauri::command]` in `commands.rs`, registered in `main.rs`). Most take and return JSON. The data shapes are Rust structs with `#[serde(rename_all = "camelCase")]`, mirrored by hand in [`web/src/types.ts`](../../web/src/types.ts). The Rust side is the source of truth.
- **Bulk data is binary.** Terrain chunks, the road ribbon, structures, barriers, tree models and the route's per-sample arrays go back as `tauri::ipc::Response` byte buffers: a few `u32` counts, then `f32` (or `f64`) arrays end to end. The page wraps them in typed arrays with no parsing. The layout of each is in the doc comment of its command (`terrain_chunk`, `ride_road_mesh`, `ride_structures`, `ride_barriers`, `tree_models`, `ride_route_data`).
- **Events** go from Rust to the page:

| Event | When | What |
|---|---|---|
| `ride:state` | 20 times a second during a ride | `RideState`: distance, lap, speed, power, HR, cadence, gear, gradient, rivals, workout and segment status, and more |
| `ride:loading` | While a ride starts | Progress text ("Downloading terrain and map data…", "Building the world…") |
| `devices:changed` | A device connects, drops out or changes | The device list, with live readings |
| `ble:candidates` | While searching for Bluetooth devices | What has been heard so far |
| `controller:action` | A handlebar-controller button press | The action, which the page runs like the matching key |

Heavy commands (building a terrain chunk, the road, structures) run on Tauri's blocking pool (`spawn_blocking`) so they never hold up the window.

## From *Ride* to the summary

1. **Setup** ([`RideSetup.tsx`](../../web/src/ui/RideSetup.tsx)): the rider picks laps, workout, ghosts, segments and a group room. The page calls `ride_start` with a `RideRequest`.
2. **Start** ([`session.rs`](../../src-tauri/src/session.rs), `Session::start`):
   - load the route from the store and rebuild it (see [Routes](routes.md));
   - fetch, at the same time, elevation tiles ([`terrain.rs`](../../src-tauri/src/terrain.rs)) and map data ([`mapdata.rs`](../../src-tauri/src/mapdata.rs)), from cache where possible;
   - a route without its own heights takes them from the terrain, levelled across bridges and tunnels;
   - build the `World` (see [The world](the-world.md)), or the velodrome's world for a built-in track;
   - set up the `RideEngine` with the surfaces, trails and fords the world found, the climbs and segments, ghosts, workout and laps;
   - join the group room if asked, write two `[ride]` lines to the diagnostics log, send the trainer the rider's weight;
   - start the ride thread, and return `RideInit` (route name, climbs, layout constants, rivals, landmarks, and so on) to the page.
3. **The page builds the scene** ([`Ride.tsx`](../../web/src/ui/Ride.tsx), [`scene.ts`](../../web/src/world/scene.ts)): it fetches the route arrays, road ribbon, structures, barriers and tree models, then streams terrain chunks around the rider.
4. **The ride loop** (`run` in `session.rs`) on its own thread:
   - every **20 ms**: merge the sensor readings, advance the engine (`RideEngine::tick`);
   - every **second** (or within 250 ms of a gear change): send the trainer a command and write a `[ride]` line to the log; update the fan and lights;
   - every **500 ms** in a group ride: send this rider's position and take in everyone else's;
   - every **50 ms**: emit `ride:state` to the page.
5. **The page draws** at the display's refresh rate. Between `ride:state` updates it moves the rider on at the last speed for up to 250 ms, so motion is smooth at any frame rate.
6. **End** (`ride_finish` → `Session::finish`): under 60 recorded seconds nothing is saved. Otherwise the FIT file is written, personal bests, climb and segment bests are updated, XP and badges are worked out, the ride is added to the history and the `RideSummary` goes back to the page. The page may then upload it to connected services (see [The ride](the-ride.md#the-end-of-a-ride)).

## Threads and locks

- The **window and Tauri's sync commands** run on the main thread. Never block a sync command waiting for something that also needs the main thread (a WebKit snapshot, say): it deadlocks. See how `smoke_done` hands off to `snapshot::window_png`.
- **Async commands** and network work run on Tauri's tokio runtime.
- **The ride loop** is a plain `std::thread`. It shares the `RideEngine` and the settings with commands through `Arc<Mutex<…>>` (`Session::engine`, `Session::settings`).
- **Bluetooth** runs on tokio tasks, one per connected device (see [Devices](devices.md)). **ANT+** has its own thread reading the USB stick.
- `AppState` (in `main.rs`) holds the store, the device registry, the demo rider, the current session, the native renderer, one shared `reqwest::Client` and `rides_lock`, which serialises read-change-write of the ride list during uploads.

## Design choices

- **Rust and Tauri, not Electron**, for low overhead: the app should run on an ordinary laptop beside a trainer.
- **Speed comes from power, never from the trainer**, so every trainer gives the same speed for the same effort.
- **Heavy work happens once per route**, then it's cached. Drawing only streams what's near the rider.
- **No central service.** Map data comes from public OpenStreetMap services, the relay is self-hosted, and every rider makes their own Strava key.
- **Diagnostics are always on**, so a rider can send a useful report after something has gone wrong.

More decisions, with their reasons, are in [HANDOFF.md](../../HANDOFF.md#9-decisions-and-the-reasons-for-them).
