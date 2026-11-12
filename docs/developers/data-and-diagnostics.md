# Data and diagnostics

## The data folder

Everything the app keeps is in one folder:

| System | Folder |
|---|---|
| macOS | `~/Library/Application Support/org.velopopuli.VeloPopuli` |
| Windows | `%APPDATA%\org.velopopuli.VeloPopuli` |
| Linux | `~/.local/share/org.velopopuli.VeloPopuli` |

`VP_DATA_DIR` points it somewhere else, for tests, a second profile or a portable install.

```
data/            small JSON documents, one per store key
activities/      recorded rides as FIT files
logs/            diagnostics logs, the last 12 runs
devices.json     remembered sensors
```

## The store

[`store.rs`](../../src-tauri/src/store.rs): `Store::get`, `set` and `del` read and write `data/<key>.json`. Keys are letters, digits, `_` and `-`, at most 80 characters, so a key can never reach outside the folder. The page can use the store directly (`kv_get`, `kv_set`, `kv_del`), but mostly goes through specific commands.

| Key | What's in it | Written by |
|---|---|---|
| `settings` | Every setting: rider, trainer, physics, gears, look, group, smart home, connectors' options | Settings screen |
| `profile` | XP, level, badges, goal | End of a ride |
| `routes` | The library list (`RouteMeta`s) | Import, delete |
| `route-<id>` | `{ name, gpx }` | Import |
| `rides` | The history (`RideSummary`s, newest first, at most 1,000) | End of a ride, uploads |
| `bests` | Each route's best time, for the library | End of a ride |
| `pb-<route id>` | The route's best: time, trace (its ghost), climb and segment bests | End of a ride |
| `ghosts-<route id>` | Imported ghosts | Ghost import |
| `segments-<route id>` | Segments found by a connector | Segment search |
| `workouts`, `workout-<id>` | Imported workouts | Workout import |
| `scenery-<route id>`, `buildings-<route id>` | Map data cache | Ride start |
| `photos-<route id>` | Street-photo picks, 30 days | Ride start |

**Changing a stored shape:** give new fields `#[serde(default)]` so older files still load, and never rename a field without reading the old name too. People go back a version sometimes (see `strava_activity_id` in [Connectors](connectors-and-services.md#uploads)).

**Caches have versions.** `FORMAT` (buildings, 2) and `SCENERY_FORMAT` (4) in `mapdata.rs`, and `WAY_TAGS` (4) in `world/surface.rs`. Bump the right one when what's cached, or what it means, changes. See [The world](the-world.md#caching).

**Activity files** are only uploaded or shown in the file manager if they're in `activities/` (`Store::owns_activity`), whatever path the page sends. With an export folder set, each FIT file is copied there too.

## Secrets

[`secrets.rs`](../../src-tauri/src/secrets.rs): connector credentials and the Home Assistant token.

- **macOS and Windows:** the system keychain, under the service name **"Velo Populi"** (with a space, kept on purpose so saved logins survived the app's rename), one account per connector id.
- **Linux:** no keyring service is guaranteed, so a file per secret in the data folder, readable only by the user (mode 0600).

Secrets are never put in `settings`, which goes into diagnostics reports.

## Diagnostics

[`diag.rs`](../../src-tauri/src/diag.rs) writes a log for every run: `logs/diagnostics-<time>.log`. The last 12 are kept.

- `diag::event(category, message)`: always written. Connections, drops, errors, ride start and end, and a `[ride]` line every second of a ride: time, power, cadence, speed, distance, gradient, surface and trail, the command sent to the trainer, the gear, and what each device last said.
- `diag::raw(category, message)`: only with **Detailed diagnostics** on (the default). Every Bluetooth notification and write, and every ANT+ page, with its bytes in hex. It stops once a log reaches 60 MB; events carry on.

Each line is `<time> [<category>] <message>`. Categories include `ble`, `ble:rx`, `ble:tx`, `ant`, `ant:rx`, `ant:tx`, `dev`, `ride`, `map`, `smart`, `store`, `ui` and `panic`.

At ride start two `[ride]` lines record the set-up (route length and climbing, trainer mode, workout, group, devices) and what the map matching found (`surfaces … trails … berms … fords … widths …`, with the widths every 250 m). That's the first place to look when a route looks wrong.

### Reports

**Save diagnostics report…** in Settings (also **Trouble connecting? Save a diagnostics report…** on the Devices page) runs `diag_report`, which writes one text file:

1. **Environment:** app version, system and its version.
2. **Settings and devices**, redacted: anything named like a token, secret, password, client id or URL becomes `(set)`, and names become `(removed)`.
3. **An automatic summary** of the logs: for each device, how often it connected and dropped, how many messages it sent, at what rate, the longest silence and when, the message types, and its errors; for each ride, how long, how many seconds had no power; then any other errors.
4. **The logs themselves**, newest first, from the last two days, within 12 MB. A log that's too long keeps its start (connection set-up) and its end (the most recent).

**Report a problem** (`diag_issue`) saves a report and opens a GitHub issue with the summary filled in (see [Connectors and other services](connectors-and-services.md#problem-reports-to-github)).

The summary is tested in `diag.rs` against hand-written logs: `cargo test -p velopopuli diag`.

## Adding a setting

1. Add the field to `Settings` in `crates/core/src/types.rs`, with a default, so older settings files load.
2. Mirror it in `web/src/types.ts`, with the same default in `DEFAULT_SETTINGS`. If it's in a nested group, `mergeSettings` in `web/src/state.ts` already fills it in.
3. Add the control to `web/src/ui/Settings.tsx`, with a hint that says what it does in plain words.
4. If it holds anything secret, it doesn't belong in settings: use `secrets.rs`.
5. Describe it in the user's manual (`docs/wiki`).
