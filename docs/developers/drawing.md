# Drawing

The page draws the ride with [three.js](https://threejs.org) (WebGL). On macOS there's also an optional **native renderer** that draws with the GPU directly. Either way, all the geometry comes from Rust (see [The world](the-world.md)); the drawing code uploads it, lights it and moves the camera.

## The screens

The page is a React app ([`web/src`](../../web/src)), built with Vite. [`App.tsx`](../../web/src/App.tsx) switches between screens:

| Screen | File | What it does |
|---|---|---|
| Home | [`ui/Home.tsx`](../../web/src/ui/Home.tsx) | The route library, import, the ride button |
| Find a route | [`ui/Explore.tsx`](../../web/src/ui/Explore.tsx) | The map (Leaflet with OpenStreetMap tiles), place search, planning, famous climbs, waymarked routes, connector routes |
| Workouts | [`ui/Workouts.tsx`](../../web/src/ui/Workouts.tsx) | Built-in and imported workouts |
| Devices | [`ui/Devices.tsx`](../../web/src/ui/Devices.tsx) | Searching, connecting, live readings, remembered sensors |
| Progress | [`ui/History.tsx`](../../web/src/ui/History.tsx), [`ui/Fitness.tsx`](../../web/src/ui/Fitness.tsx) | Rides, XP and badges, fitness, power curve, training load |
| Settings | [`ui/Settings.tsx`](../../web/src/ui/Settings.tsx) | Every setting, connectors, fan and lights, diagnostics |
| Ride setup | [`ui/RideSetup.tsx`](../../web/src/ui/RideSetup.tsx) | Laps, workout, ghosts, segments, group room |
| Ride | [`ui/Ride.tsx`](../../web/src/ui/Ride.tsx) | The 3D view and the HUD: numbers, profile ([`ProfileChart.tsx`](../../web/src/ui/ProfileChart.tsx)), mini-map ([`MiniMap.tsx`](../../web/src/ui/MiniMap.tsx)), rivals, workout, street photos ([`StreetPhoto.tsx`](../../web/src/ui/StreetPhoto.tsx)) |
| Summary | [`ui/Summary.tsx`](../../web/src/ui/Summary.tsx) | After a ride: totals, bests, XP, badges, uploads |

Settings are one JSON document (`settings` in the store). The page loads it at start (`api.kv.get('settings')` in `App.tsx`), fills in anything missing from `DEFAULT_SETTINGS` (`mergeSettings` in [`state.ts`](../../web/src/state.ts)), and saves it whole on every change. During a ride, changes also reach the ride loop through `ride_update_settings`. Its shape is `Settings` in [`types.ts`](../../web/src/types.ts) and in `crates/core/src/types.rs`. A new setting needs a default on both sides (`DEFAULT_SETTINGS`, and `#[serde(default)]` in Rust), so older settings files still load.

## The 3D scene

[`world/scene.ts`](../../web/src/world/scene.ts), class `World`, owns the three.js renderer, scene, camera and lights. When a ride starts it:

1. Builds the **road** from `ride_road_mesh` (positions, texture coordinates, looks) with the road material ([`world/surfaces.ts`](../../web/src/world/surfaces.ts)).
2. Adds the **structures** (bridges, tunnels, overbridges) from `ride_structures`.
3. Adds the **landmarks:** text signs drawn on a canvas, with an icon for points of interest.
4. Creates the **rider** and the **rivals** ([`world/rider.ts`](../../web/src/world/rider.ts)), each with a name label.
5. Starts [`world/terrain.ts`](../../web/src/world/terrain.ts) streaming the ground around the rider, and [`world/barriers.ts`](../../web/src/world/barriers.ts) the barriers near them.

Every frame ([`Ride.tsx`](../../web/src/ui/Ride.tsx), the animation loop):

- **Glide:** the latest `ride:state` arrives 20 times a second. Between updates the rider and the rivals are moved on at their last speed, for up to 0.25 s, so motion is smooth at any frame rate. On a lap ride this wraps round the loop.
- **Place** the rider at the distance on the curve through the route samples (`sampleAt` in [`core/gpx.ts`](../../web/src/core/gpx.ts)), with the heading from the curve's tangent, the lean for the bend (`leanAt`), and the bank of a berm or velodrome.
- **Pose** the rider: pedals and legs from the cadence, wheels from the speed.
- **The camera** follows (`CAMERAS`), field of view 58°:

| Camera (`C` cycles) | Behind | Up | Beside | Looks ahead |
|---|---|---|---|---|
| 0, chase | 7.5 m | 3 m | | 9 m |
| 1, high chase | 24 m | 13 m | | 6 m |
| 2, alongside | | 2 m | 9 m | |
| 3, overhead | 38 m | 62 m | | 22 m |

### The ground

[`world/terrain.ts`](../../web/src/world/terrain.ts), class `Terrain`:

- **Chunks** are asked for nearest first, at most six at a time, as the rider moves. Each answer from `terrain_chunk` becomes meshes for the ground, buildings, water and other roads, plus a list of trees. Chunks out of range are freed.
- **The far sheet** (`terrain_far`) is fetched once, and again when the rider has gone far enough.
- **Buildings** share one window texture (`windowTexture`), drawn on a canvas. The other roads share one texture of road, path and railway markings (`waysTexture`).

### Trees

[`world/trees.ts`](../../web/src/world/trees.ts): each kind is one model (`tree_models`), drawn with instancing, so a thousand spruces are one draw call. From 200 m out (with 75 m of overlap either way) trees are **sprites**: two triangles showing a picture of the same model, turned to face the camera. The pictures are rendered from the models once, at start, into one atlas that the native renderer shares.

### The rider

[`world/rider.ts`](../../web/src/world/rider.ts) builds a cyclist from real proportions: a 56 cm road frame with drop bars and 700c wheels, and a 1.78 m rider on the hoods. The cranks turn with the cadence and the legs follow the pedals. Rivals use the same model; ghosts are translucent.

### Light and weather

[`world/weather.ts`](../../web/src/world/weather.ts):

- **Time of day:** dawn, day, evening, dusk or night, each with its own sky, sun or moon, haze and ambient light. *Real time of day* takes it from the route's local clock: dawn from 5:30, day from 8:00, evening from 17:00, dusk from 19:00, night from 21:00. At dusk and night the rider has a lamp.
- **Weather:** clear, cloudy, fog, rain or snow. *Live* asks [Open-Meteo](https://open-meteo.com) for the weather at the route now (`weather_now`) and maps its WMO code. Rain and snow are particles round the camera (`Precipitation`).
- **Fog** starts at 300 m and reaches full at 8.5 km on a clear day, closer in bad weather. It hides the edge of the far sheet.
- **In a tunnel** (`covered` in `RideInit`) the light is dimmed.

Colours are authored in sRGB and converted to linear for lighting, in Rust and in the page alike.

## The native renderer (macOS, optional)

**Native renderer (experimental, macOS)** in Settings (`nativeRenderer`) draws the world with [wgpu](https://wgpu.rs) on Metal instead of three.js. It's for lower overhead and better frame pacing. It's been compared with three.js only in screenshots so far, and HUD-over-native hasn't been checked in a real window (see HANDOFF.md).

- **Where it draws** ([`native/host.rs`](../../src-tauri/src/native/host.rs)): a view of its own placed beneath the web page. The page's background goes transparent, so the HUD shows on top.
- **What it draws** ([`native/mod.rs`](../../src-tauri/src/native/mod.rs), [`native/shader.wgsl`](../../src-tauri/src/native/shader.wgsl)): it reads the `World` and the `RideEngine` directly, so it needs nothing per frame from the page. Terrain, roads, buildings, water, trees, bridges and tunnels come from the same Rust code that serves the page. Barriers, road surfaces, precipitation and tree placement are reimplemented in `native/mod.rs`.
- **What the page hands over once** ([`world/native.ts`](../../web/src/world/native.ts), `packNative`): the textures (road surfaces, windows, ways, the tree-sprite atlas), the text signs, the small props along the route baked from the three.js scene, and the rider in six pedal positions. All of it is packed into one binary body for `native_start`.
- **Controls:** `native_camera` (the `C` key), `native_look` (time and weather changes), `native_capture` (smoke pictures, `<name>.native.png`), `native_stop`.

**If you add something to the three.js scene that the native renderer should show too, either bake it in `native.ts` or reimplement it in `native/mod.rs`.** Otherwise it will be missing there. `native_available` is `false` on other systems, which keep three.js whatever the setting says.
