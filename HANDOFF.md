# VeloPopuli — handoff notes

Everything someone needs to pick up the project: what it is, where it stands, how it is built and tested, how releases work, what is unfinished, and the decisions and traps that aren't obvious from the code. Keep this file current when you change something it describes.

- Website: <https://velopopuli.com> · Repository: <https://github.com/pepperstm/velopopuli> · Guides: [wiki](https://github.com/pepperstm/velopopuli/wiki) (source in [`docs/wiki/`](docs/wiki))
- Maintainer: Graham Barber (GitHub `pepperstm`)
- Licence: GPL-3.0-or-later for all versions (versions up to v0.2.0 were first published as MIT and relicensed on 2026-10-19 — see *Decisions*)

## 1. What it is

A free, open-source indoor-cycling app: import any GPX route, ride it on a smart trainer, and see it as a 3D world built from open data (OpenStreetMap, open elevation tiles). The trainer's resistance follows the real gradient (or holds ERG targets in workouts); speed is simulated from power. Gamified with pace riders, ghosts, timed climbs, Strava segments, XP, badges, a season goal and streaks. Group rides go through a small self-hostable relay. Windows, macOS and Linux.

It started on 2026-10-13 as "WinterMiles" (an icTrainer alternative for riding GPX routes over winter), was rewritten the same day from Electron/TypeScript into Rust + Tauri for low overhead, renamed "Velo Populi" and then "VeloPopuli" to match the website.

## 2. Where it stands (October 2026, v0.2.0)

**Solid and tested:** route building (GPX parsing, resampling, smoothing, climb detection), physics, ride recording and FIT export, workouts (.zwo/.erg/.mrc and built-ins), fitness maths, gamification, the 3D world (terrain, buildings, forests, water, the surrounding road network, bridges, tunnels, overpasses, road surface/width/markings, race barriers, landmarks, weather), diagnostics, the relay. 73 automated tests, plus visual checks through the automated smoke harness (section 6).

**Built but not proven with real use:**

| Area | State |
|---|---|
| **Trainers and sensors** | Protocols unit-tested against the specs. The first real trainer — a **Tacx Vortex Smart (T2180) over Bluetooth** — connected but **kept dropping out and showed no power**. Not yet diagnosed: a diagnostics report from a ride is needed. This is the top priority. |
| **Strava** (OAuth, upload, segments, activities as ghosts) | Never run against the live API. Users must create their own API key (open source can't ship a secret). Scope is `read,activity:read_all,activity:write`. |
| **Windows and Linux** | Built and packaged by CI on every push (and Arch's package is installed in a container and its libraries checked), but **never run by a person**. |
| **Native renderer** (macOS, opt-in) | wgpu/Metal in an NSView under a transparent webview. Matches three.js in screenshots; HUD-over-native never checked in a real window; no clean release-build performance comparison. |
| **Mapillary street photos** | Needs a user token; never tried with a real one. |
| **Fan and lights** (Home Assistant, Shelly, Tasmota, webhook) | Tested against mocks only. |

**Removed for now:** Apple Health / Apple Watch integration (the maintainer asked; it's in git history before commit "Remove Apple Health…" if wanted back). A watch that broadcasts heart rate over Bluetooth still works as an HR sensor.

## 3. Repository map

```
crates/core/        Pure Rust, no I/O, fully unit-tested (tests in crates/core/tests/)
  gpx, physics      Route building; speed from power (CdA, Crr, altitude-dependent air density)
  ride, gamify      50 Hz ride engine, rivals/ghosts, recorder; XP, levels, badges, streaks
  workout, fitness  Workouts; power curve, FTP estimate, zones, CTL/ATL/TSB
  gearing           Virtual gears: gear sets (presets, custom teeth), the effective gradient that makes the trainer feel like a gear
  segments, fit     Strava segments/ghost matching, photo picking; FIT encoder
  devices/          BLE GATT (FTMS, CPS, CSC, HR, Tacx FE-C tunnel) and ANT+ (framing, FE-C, power, HR, S&C) codecs; sensor hub (most direct source wins)
  world/            mod.rs: World, chunk/horizon meshes, the route road ribbon
                    height.rs: height field pinned to the road; dem.rs: elevation tiles
                    mvt.rs: vector-tile decoder; scenery.rs: forests, water, other roads, route↔map matching, Kerb lookups
                    roads.rs: draping other roads on terrain; structures.rs: decks, bores, overbridges
                    buildings.rs: OSM footprints → meshes; surface.rs: route surface/width/markings from OSM tags
                    barriers.rs: race barriers; landmarks.rs: signs, summits, POIs
crates/relay/       Group-ride relay (lib + bin `velopopuli-relay`, WebSocket, port 47800, stateless)
src-tauri/          The app (Tauri 2)
  main.rs           App state, command registration, data dir (VP_DATA_DIR override), diag init
  commands.rs       Every command the page calls (routes, ride data in binary form, settings, smoke hooks…)
  session.rs        Ride start (loads map data, builds World), the simulation loop, trainer commands, saving
  devices/          ble.rs (btleplug), ant.rs (rusb, vendored libusb), sim.rs (demo rider), mod.rs registry
  mapdata.rs        Downloading + caching vector tiles and Overpass data per route
  native/           Native renderer (mod.rs, host.rs NSView, shader.wgsl)
  diag.rs           Diagnostics log + reports; issues.rs: GitHub new-issue links; snapshot.rs: window PNGs (macOS)
  strava, geo, photos, smart, group, library, terrain, secrets, store
web/                React + three.js (Vite). ui/ screens, world/ the 3D scene (scene.ts, terrain.ts, rider.ts,
                    barriers.ts, surfaces.ts, weather.ts, native.ts = assets handed to the native renderer)
docs/images/        README screenshots; docs/wiki/ wiki source
packaging/          arch/PKGBUILD, RELEASE_NOTES.md (body of every GitHub Release)
.github/workflows/build.yml   CI: tests, installers for every platform, Arch package, releases
resources/          99-ant-usb.rules (Linux udev rule for ANT+ sticks)
```

How a ride flows end to end is in the wiki's [Architecture](https://github.com/pepperstm/velopopuli/wiki/Architecture) page.

## 4. Data and formats

- **App data folder:** macOS `~/Library/Application Support/org.velopopuli.VeloPopuli`, Windows `%APPDATA%\org.velopopuli.VeloPopuli`, Linux `~/.local/share/org.velopopuli.VeloPopuli`. Override with `VP_DATA_DIR`.
- `data/*.json` (store keys): `settings`, `routes` (library list) + `route-<id>` (name + GPX), `rides`, `profile`, `ghosts-<route>`, `scenery-<route>`, `buildings-<route>`, workouts, segments.
- **Caches have versions:** buildings `FORMAT` (2) and `SCENERY_FORMAT` (4) in `mapdata.rs`; road tags `WAY_TAGS` (2) in `world/surface.rs`. Bump the relevant one when the cached shape changes, or old caches will be read wrongly.
- **Secrets** (Strava, Home Assistant, GitHub none): OS keychain on macOS/Windows under the service name **"Velo Populi"** (with the space — kept on purpose so saved logins survive the rename), files with 0600 permissions on Linux.
- **Logs:** `logs/diagnostics-<time>.log`, last 12 runs kept.

## 5. Building

Rust stable + Node 20+. Linux needs `libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev libudev-dev pkg-config`.

```bash
npm install
npm run dev          # hot reload
npm test             # Rust tests
npm run build        # this platform's installer
npm run build:mac    # universal (arm64 + x86_64) app + DMG, ad-hoc signed
npx tauri build --debug --no-bundle   # quick debug binary at target/debug/velopopuli
```

Notes from experience:

- A full debug build plus a universal release build need ~10 GB in `target/`. On a nearly full disk, `rm -rf target/debug` before `npm run build:mac`.
- macOS builds are ad-hoc signed (`bundle.macOS.signingIdentity = "-"`, hardened runtime off), not notarised. Users need *Open Anyway* the first time. A Developer ID certificate and notarisation would remove that.
- Windows builds are unsigned (SmartScreen warns).
- The Linux binary is built on Ubuntu 22.04 (glibc 2.35); it runs on newer systems, including Arch.

## 5a. How changes reach `main`

The full process is in [CONTRIBUTING.md](CONTRIBUTING.md): issue first → branch (`fix/…`, `feature/…`, `docs/…`) → one logical change with tests and docs → pull request from the template → CI green → maintainer review → **squash merge**. `main` is protected: changes from contributors arrive only through pull requests with passing checks. `npm run format` formats everything (rustfmt.toml, .prettierrc.json); `npm run check` runs what CI runs. After merging, sync `docs/wiki/` to the wiki repository and tag releases as below.

## 6. Testing

- `cargo test --workspace` — 73 tests (core, app, relay); `npx tsc --noEmit` for the page.
- **The smoke harness** is how most visual and end-to-end checking has been done: a debug build that rides (or opens a screen) by itself, saves pictures and the visible text, and quits. All the `VP_SMOKE_*` switches are documented in the wiki's [Building from source](https://github.com/pepperstm/velopopuli/wiki/Building-from-source). The most useful: `VP_SMOKE_AT` (start part-way), `VP_SMOKE_CAM=3` (overhead — good for junctions), `VP_SMOKE_WINDOW=1` (whole window with HUD), `VP_SMOKE_CLIP=6` (frames for a demo clip), `VP_SMOKE_SURFACE`, `VP_SKIP_OVERPASS`, `VP_SMOKE_REPORT`.
- The ride-start diagnostics line `[ride] surfaces … widths …` shows what the map matching decided.
- Good test routes: Bourg d'Oisans → Alpe d'Huez (town, river, hairpins — route it with BRouter); the Trouée d'Arenberg (OSM track "Drève des Boules d'Hérin", `surface=sett`); Meylan near Grenoble (roundabouts); any city crossing (viaducts).
- A smoke window that opens behind other windows gets few animation frames — the `.txt` says how many were drawn; a stale picture means a hidden window, not a bug.
- **Things not to do on a maintainer's Mac:** don't script other apps with `osascript`/System Events, and don't run a Bluetooth scan or `screencapture` from the terminal without warning — each raises a macOS permission prompt (one hung a session).

## 7. Releases

1. Bump the version in `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `crates/core/Cargo.toml`, `crates/relay/Cargo.toml` (and `pkgver` in `packaging/arch/PKGBUILD`). Update `packaging/RELEASE_NOTES.md` if anything about installing changed.
2. Commit, then `git tag vX.Y.Z && git push origin vX.Y.Z`.
3. CI builds everything and publishes a release named "VeloPopuli vX.Y.Z" — the latest release for a plain version, a **pre-release** for a version with a suffix (`v0.3.0-beta.1`) — with: the universal DMG, Windows `.msi` and `-setup.exe`, `.deb`, `.AppImage`, the Arch `.pkg.tar.zst` and its `PKGBUILD` (with real checksums), the relay for each OS, and `SHA256SUMS.txt`.
4. **AUR:** the attached PKGBUILD is ready to publish as `velopopuli-bin` (not done yet — needs an AUR account and `.SRCINFO` via `makepkg --printsrcinfo`).

CI also builds every push to `main`; installers can be downloaded from the run's artifacts.

## 8. Next steps, in priority order

1. **Get the Tacx Vortex working.** Ask for a diagnostics report (Report a problem → Report on GitHub); check whether the Vortex offers FTMS or only the Tacx FE-C tunnel (`6e40fec1…`) plus Cycling Power, whether notifications stop or the link drops (`[ble] dropped: …` lines and their cause), and whether page 0x19 power is present. Likely suspects: another app holding the connection; the FE-C-over-BLE path (written to spec, never tried); write-with-response on the Tacx characteristic. Then build up the tested-hardware table in the wiki.
2. **Try the Windows and Linux builds** on real machines (ANT+ on Windows needs WinUSB via Zadig).
3. **Strava** against the live API.
4. Pairing to a specific ANT+ device and more than one device per type.
5. Rolling resistance per road surface (cobbles/gravel heavier) — agreed as a natural next step, not started.
5a. Virtual gears — built (keyboard, Bluetooth remotes, on-screen buttons, rider-defined gear sets); next: Di2 and Zwift Click/Play shifters, Zwift Hub native shifting, and feeling it on real trainers.
6. Native renderer: verify HUD in a real window, measure release-build overhead, then Windows/Linux hosts.
7. **Stretch — self-hosted group servers and clubs.** Design agreed in discussion: no central server; identity is a keypair created on the device (no accounts), friends lists stored locally and exchanged by friend codes/QR including the friend's home server address, clubs run their own server (the relay plus a small database: members, invites, scheduled rides, shared routes, leaderboards), optional community server, federation later. Package the relay as a Docker image with `wss://`. Leaderboards can't be cheat-proof (power comes from the rider's device).
8. More OpenStreetMap detail: roof shapes, other land cover, broadleaf trees, squares, cuttings/embankments for side roads.

## 9. Decisions and the reasons for them

- **Rust + Tauri, not Electron:** the maintainer's stated priority is low overhead. Everything computational or hardware-facing is Rust; the page only draws.
- **GPLv3 licence:** the project began under MIT. On 2026-10-19 the maintainer, as sole copyright holder, relicensed all of it — earlier versions included — under GPL-3.0-or-later so that everything built from it must also be open source. (AGPL was considered earlier; plain GPL was chosen. Note GPL's copyleft applies when software is *distributed* — someone running a modified relay server for others without distributing it isn't required to share the changes; AGPL would cover that.) Copies taken under MIT before the change keep MIT rights. Dependencies are MIT/Apache-2.0/BSD-style, all GPLv3-compatible; the bundled fonts are under the OFL, which is fine alongside GPL code.
- **Name "VeloPopuli"** (one word, as on the website; header wordmark lowercase "velo**populi**"). The bundle id `org.velopopuli.VeloPopuli` and the keychain service "Velo Populi" were left unchanged so user data and logins carry over.
- **Colours and type** follow velopopuli.com: paper `#f7f7ef`, ink `#24271f`, dark `#252d24`, lime accent `#e9ff60` (with ink text on it); DM Sans and Manrope, bundled under the OFL in `web/src/fonts/`. Checked against cycling brands: the old orange was too close to Strava/Zwift; the lime stays clear of Strava/Zwift/Tacx orange, Wahoo/Garmin blue-black and TrainerRoad red (nearest: Visma–Lease a Bike's pure yellow on black).
- **No central services:** map data from public OSM services; relay is self-hosted; Strava keys are per user.
- **Speed from power, never from the trainer**, so every trainer gives the same speed for the same effort.
- **Map data:** vector tiles (`vector.openstreetmap.org/shortbread_v1`) for scenery and roads — reliable; Overpass for building detail and the route's road tags (`surface`, `width`, `lanes`, `highway`) — unreliable. Only `overpass.private.coffee` answered consistently in testing; `overpass-api.de` rejects clients without a proper User-Agent (the app sends one). Failures fall back gracefully and are retried later (buildings after two weeks, road tags after a day).
- **The route's road is a "closed race road"**: barriers both sides, real surface and width, centre line only on main roads; the mapped copy of the route's own road is hidden.
- **Diagnostics are always on** (raw sensor data optional) so a user can send a useful report after the fact; reports strip names, tokens and locations.
- **GitHub reporting opens a pre-filled issue in the browser** rather than signing in from the app: a device-flow sign-in was started and dropped in favour of the simpler approach.
- **Apple Health removed** for now (no desktop HealthKit; export import was clunky).
- **Commits are under the maintainer's name only.**

## 10. Known traps

- **ANT+ network key** is in `devices/ant.rs`; distributors should register as an ANT+ adopter (free) at thisisant.com.
- **Tauri sync commands run on the main thread**: never block one waiting for something that completes on the main thread (e.g. a WebKit snapshot) — that deadlocks. See how `smoke_done` hands off to `snapshot::window_png`.
- **upload-artifact keeps the common parent path** of its inputs, so files land at different depths per OS; the release job finds relay binaries with `find`.
- **Vector-tile streets carry no `surface`**, only `kind`/`tracktype` — surface and width needed Overpass.
- **Fonts on canvas:** barrier banners are drawn on a canvas in Manrope; `Ride.tsx` waits for `document.fonts.load` before building the world, or the banner falls back to a system font.
- **The native renderer is fed partly from the page** (`native.ts`: textures, text labels, props baked from the three.js scene, rider poses) and partly from Rust meshes; anything added to the three.js scene that native should also show must be either baked or reimplemented in `native/mod.rs` (barriers, road surfaces and precipitation are reimplemented there).
- **Barriers and the road width:** anything placed beside the route must use `World::half_width(i)` (Rust) or `halfWidthAt()` (page), never a constant.
