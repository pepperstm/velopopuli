# Building from source

You need [Rust](https://rustup.rs) (stable) and [Node.js](https://nodejs.org) 20+.

- **Linux** also needs: `sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev libudev-dev pkg-config`
- **Windows** needs the MSVC build tools.

```bash
npm install
npm run dev        # run with hot reload
npm test           # Rust unit tests
npm run build      # optimised app + installer for this platform (target/release/bundle/)
npm run build:mac  # macOS: one installer for Apple Silicon and Intel
```

Every push is built and tested by GitHub Actions on Windows, macOS and Linux; the installers are attached to each run.

## Tests

- `cargo test -p velopopuli-core` — the pure-Rust core: route building, physics, recording, workouts, fitness, segments, sensor protocols, terrain, buildings, scenery, surfaces, road widths and barriers.
- `cargo test -p velopopuli` — the app (diagnostics, GitHub issue links, smart plugs, …).
- `cargo test -p velopopuli-relay` — the group-ride relay.

## The automated check (smoke test)

A debug build can run itself: start a ride or open a screen, wait, save a picture and the visible text, and quit. Useful for checking a change visually without clicking through the app, and for screenshots.

```bash
npx tauri build --debug --no-bundle
VP_DATA_DIR=/tmp/vp VP_SMOKE=/tmp/vp/shot.png target/debug/velopopuli
```

| Variable | Effect |
|---|---|
| `VP_DATA_DIR` | Use this folder for all data (routes in `data/routes.json`, settings in `data/settings.json`) |
| `VP_SMOKE` | Path of the picture to save (the 3D canvas); also writes `<name>.txt` with the screen's text and the ride state |
| `VP_SMOKE_SCREEN` | `ride` (default), `home`, `explore`, `workouts`, `devices`, `history`, `settings` |
| `VP_SMOKE_MS` | How long to run before capturing (default 9000) |
| `VP_SMOKE_AT` | Start the ride this many metres along the route |
| `VP_SMOKE_CAM` | Camera: 0 behind, 1 high behind, 2 alongside, 3 overhead |
| `VP_SMOKE_WORKOUT` | Ride with this workout (e.g. `builtin-sweetspot`) |
| `VP_SMOKE_GHOSTS`, `VP_SMOKE_ROOM` | Race these ghosts / join this group-ride room |
| `VP_SMOKE_SURFACE` | Lay the whole route in one surface (0 tarmac, 1 concrete, 2 cobbles, 3 gravel, 4 dirt) |
| `VP_SMOKE_WINDOW` | Also save the whole window, HUD included, as `<name>.window.png` (macOS) |
| `VP_SMOKE_CLIP` | Save this many seconds of window frames (15 fps) to `<name>.frames/` for demo clips |
| `VP_SMOKE_REPORT` | Also save a diagnostics report here |
| `VP_SKIP_OVERPASS` | Don't ask the Overpass API (tests the fallbacks) |

The ride uses the newest route in the library (or the Demo Loop) and the simulated demo rider. A window that opens behind others gets few animation frames; the `.txt` reports how many were drawn.

Turn clip frames into a video with `ffmpeg -framerate 15 -i shot.frames/%04d.png -c:v libx264 -pix_fmt yuv420p clip.mp4`.

## Commit style

Plain, descriptive commit messages; one change per commit. Keep the README's status table honest — say what was checked and how.
