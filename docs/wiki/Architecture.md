# Architecture

About two thirds of the code is Rust; everything that computes or talks to hardware is in it. The web layer (React + three.js) draws the screens and the 3D world from data the Rust side prepares.

```
crates/core/      Pure Rust library, no I/O — fully unit-tested
  gpx, physics    GPX → resampled route, smoothing, gradients, climb detection; speed from power
  ride, gamify    The ride simulation and recorder; XP, levels, badges, streaks
  workout         Zwift .zwo and .erg/.mrc workouts; built-in sessions
  fitness         Power curve, FTP estimate, zones, training load
  segments        Strava segments and other rides matched onto the route; ghosts; photos
  fit             FIT activity encoder
  waymarked       Waymarked mountain-bike routes: listing an area, joining a route's ways into one line
  devices/        Bluetooth GATT and ANT+ protocol encode/decode; the sensor hub
  world/          Terrain (elevation tiles, height field, chunk/horizon/road meshes),
                  OpenStreetMap buildings, scenery (forests, water, other roads, bridges and tunnels),
                  structures (decks, bores, overbridges, log bridges), the route road's surface and width,
                  trails (grades, berms, fords), barriers and landmarks
crates/relay/     The group-ride relay server (WebSocket, stateless)
src-tauri/        The app (Tauri 2)
  devices/        Bluetooth via the OS stack (btleplug); ANT+ USB stick via libusb; demo rider
  session         50 Hz simulation loop, trainer control, saving rides
  native/         The optional native renderer (wgpu → Metal) and its shaders
  mapdata         Downloading and caching map data (vector tiles, Overpass)
  diag            The diagnostics log and reports
  group, smart    Group-ride client; fan and light control
  strava, geo     Strava; place search and route planning
web/              React screens and the three.js world
```

## How a ride works

1. **Start:** the route is built (resampled every few metres, smoothed, climbs found). Map data for the route is loaded from the cache or downloaded: vector tiles for scenery, Overpass for building detail and the route's road tags (surface, width, lanes, trail grades). The `World` combines it: a height field pinned to the road, the scene (forests, water, other roads), structures, the route road and barriers.
2. **Loop (50 Hz, Rust):** sensor readings are merged (most direct source wins), the physics advances the rider, rivals and ghosts move, and once a second the trainer gets a command (gradient or ERG target) and the diagnostics log gets a line. State goes to the page 20 times a second.
3. **Drawing:** the page asks for meshes in compact binary form (terrain chunks streamed around the rider, the road ribbon, structures, barriers) and draws them with three.js — or the native renderer draws the same meshes with Metal beneath a transparent page that shows only the HUD.
4. **End:** the ride is saved as FIT, personal bests, XP and badges are updated, and it can be uploaded to Strava.

## Design choices

- **Speed is simulated from power,** never read from the trainer, so every trainer gives the same speed for the same effort.
- **Everything heavy is computed once per route** and cached; drawing only streams what's near the rider.
- **No central service.** Map data comes from public OpenStreetMap services; group rides use a relay anyone can run.
