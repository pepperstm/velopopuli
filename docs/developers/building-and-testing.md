# Building and testing

## What you need

- [Rust](https://rustup.rs), stable, and [Node.js](https://nodejs.org) 20 or later.
- **Linux:** `sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev libudev-dev pkg-config`
- **Windows:** the MSVC build tools. (WebView2 is already part of Windows 10 and 11.)
- **macOS:** the Xcode command-line tools. For the universal build, also `rustup target add x86_64-apple-darwin`.

ANT+ uses a vendored libusb, so there's nothing extra to install for it.

## Commands

| Command | What it does |
|---|---|
| `npm install` | Installs the page's packages and the Tauri CLI |
| `npm run dev` | Runs the app, reloading the page as you edit it (Rust changes rebuild) |
| `npx tauri build --debug --no-bundle` | A quick debug binary at `target/debug/velopopuli`, which is what the smoke harness runs |
| `npm run build` | An optimised app and installer for this system, in `target/release/bundle/` |
| `npm run build:mac` | One macOS app and DMG for Apple silicon and Intel, ad-hoc signed |
| `npm test` | The core library's tests only (fast) |
| `npm run check` | Everything CI checks: formatting, type-checking, clippy (correctness lints as errors) and every test |
| `npm run format` | Formats Rust (`rustfmt.toml`) and the page (`.prettierrc.json`) |

A full debug build plus a universal release build need about 10 GB in `target/`. On a nearly full disk, `rm -rf target/debug` before `npm run build:mac`.

## Tests

There are 154 tests: 130 for the core, 1 for the relay and 23 for the app. `cargo test --workspace` runs all but four, which need the network and run only when asked for (`cargo test -p velopopuli -- --ignored`).

| Where | What it covers |
|---|---|
| `crates/core/tests/core.rs` | Route building, physics, the ride engine, laps, recording, gamification, the world (terrain, buildings, scenery, surfaces, widths, barriers, trees, fields, berms, fords) |
| `crates/core/tests/devices.rs` | Bluetooth and ANT+ encoding and decoding, and OpenBikeControl buttons, against hand-built packets |
| `crates/core/tests/features.rs` | Workouts, fitness and training load, streaks, the trainer's feel settings, segments and ghosts, landmarks, street photos, virtual gears |
| `crates/core/tests/velodrome.rs` | Herne Hill: lap length, banking, direction, the grounds |
| `crates/core/tests/uploads.rs` | Upload records and the migration of old Strava ids |
| Unit tests in `src-tauri/src` | Diagnostics summaries, GitHub issue links, smart plugs, connectors against fake servers, the https check, the native renderer's geometry |
| `crates/relay` | The relay, in-process |

The page has no unit tests. `npx tsc --noEmit` type-checks it, and the smoke harness checks it running.

A good test for the core builds a small route or world in code and checks one fact about it. See `crates/core/tests/velodrome.rs` for a short example.

## The smoke harness

A debug build can run itself: open a screen or start a ride, wait, save a picture and the text on screen, and quit. Most visual and end-to-end checking has been done this way. You can see a change without clicking through the app, and the README and wiki screenshots are taken with it.

```bash
npx tauri build --debug --no-bundle
VP_DATA_DIR=/tmp/vp VP_SMOKE=/tmp/vp/shot.png target/debug/velopopuli
```

| Variable | Effect |
|---|---|
| `VP_DATA_DIR` | Use this folder for all data. It works without `VP_SMOKE` too, for a separate test profile. |
| `VP_SMOKE` | Path of the picture to save (the 3D canvas). Also writes `<name>.txt` with the screen's text and the ride state. |
| `VP_SMOKE_SCREEN` | `ride` (the default), `home`, `explore`, `workouts`, `devices`, `history` or `settings` |
| `VP_SMOKE_PLAN` | On `explore`, plan through `lat,lon;lat,lon;…`, or make a loop with `loop:lat,lon,km` |
| `VP_SMOKE_SCROLL` | Scroll to the section with this heading before capturing (e.g. `Gears` on Settings) |
| `VP_SMOKE_MS` | How long to run before capturing (default 9000). Give maps and terrain longer. |
| `VP_SMOKE_WINDOW` | Also save the whole window, HUD included, as `<name>.window.png` (macOS) |
| `VP_SMOKE_ROUTE` | Ride this route id instead of the newest in the library (e.g. `herne-hill`, `herne-hill-clockwise`, `demo`) |
| `VP_SMOKE_AT` | Start this many metres along the route |
| `VP_SMOKE_CAM` | Camera: 0 behind, 1 high behind, 2 alongside, 3 overhead (good for junctions) |
| `VP_SMOKE_LAPS` | Ride a loop for this many laps (`0`: until stopped) |
| `VP_SMOKE_WORKOUT` | Ride with this workout (e.g. `builtin-sweetspot`) |
| `VP_SMOKE_GHOSTS` | Race these ghosts (comma-separated ids) |
| `VP_SMOKE_ROOM` | Join this group-ride room |
| `VP_SMOKE_SHIFT` | Shift this many virtual gears (negative: easier), one every 0.4 s from 3 s in |
| `VP_SMOKE_SURFACE` | Lay the whole route in one surface: 0 tarmac, 1 concrete, 2 cobbles, 3 gravel, 4 dirt |
| `VP_SMOKE_CLIP` | Save this many seconds of window frames (15 fps) to `<name>.frames/`, for demo clips |
| `VP_SMOKE_REPORT` | Also save a diagnostics report here |
| `VP_SKIP_OVERPASS` | Don't ask the Overpass API: tests the fallbacks |

The ride uses the simulated demo rider. With `"nativeRenderer": true` in the data folder's `settings.json`, the native renderer's own frame is saved too, as `<name>.native.png`. That's the only way to see it, since window snapshots capture just the web view.

**Tips:**

- **A test route** is two files in the data folder: `data/routes.json` (the library list) and `data/route-<id>.json` (`{"name": …, "gpx": …}`). The list's `distance`, `ascent` and `climbs` are only shown on the card. The app works them out when it imports a GPX, so fill them in by hand for a test library.
- **What to look at:** a smoke ride prints what the map matching decided: bridges and tunnels along the route, fords, berms, landmarks, other roads drawn, and how long the chunks in view took to build. The `[ride] surfaces … trails … berms … fords … widths …` line in the log says the same.
- **A window that opens behind others** gets few animation frames. The `.txt` says how many were drawn. A stale picture means a hidden window, not a bug.
- **Frames to video:** `ffmpeg -framerate 15 -i shot.frames/%04d.png -c:v libx264 -pix_fmt yuv420p clip.mp4`.
- **On a maintainer's Mac:** don't script other apps with `osascript`, or start a Bluetooth scan or `screencapture` from the terminal without warning. Each raises a macOS permission prompt.

### Good test routes

| Route | What it exercises |
|---|---|
| Bourg d'Oisans → Alpe d'Huez (route it with BRouter) | A town, a river, hairpins, barrier netting, the tree line |
| The Trouée d'Arenberg (OSM track "Drève des Boules d'Hérin") | Cobbles (`surface=sett`) |
| Meylan, near Grenoble | Roundabouts |
| Any city crossing | Viaducts, overbridges, dense buildings |
| Around Finale Ligure | Graded singletrack, switchbacks (berms), Ponte dell'Acqua (way 143650465, a log bridge), Strada Valle del Bosco (way 113195224, a ford) |
| A hike near Chamonix | Trail grades estimated from `sac_scale` |
| A field near Chartres | The off-map dirt track |
| Devon lanes (Bovey Tracey, Ilsington) | Hedge banks, made-up fields |
| `herne-hill` | The velodrome, banking, laps |

## Continuous integration

[`.github/workflows/build.yml`](../../.github/workflows/build.yml) runs on every push to `main`, every pull request and every version tag:

| Job | What it does |
|---|---|
| `test` | Type-check, `cargo fmt --check`, `prettier --check` and `cargo test --workspace`, on Ubuntu 22.04 |
| `app` (macOS, Windows, Linux) | Builds the installers: a universal DMG; `.msi` and `-setup.exe`; `.deb` and `.AppImage`; plus the relay. They're attached to the run as artifacts. |
| `arch` | Repackages the `.deb` as an Arch package in an Arch container, installs it, and checks every library the app needs is there and the ANT+ udev rule is installed |
| `release` | Only for `v*` tags: gathers everything into a GitHub Release (see [Releasing](releasing.md)) |

`main` is protected: `test`, `Linux`, `Windows` and `macOS` must pass on a branch that's up to date with `main`. The Arch package check is optional. GitHub's macOS runners are sometimes short; a macOS job that was cancelled before it started ("The job was not acquired by Runner") is a queue problem, so re-run it.
