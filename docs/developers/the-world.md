# The world

The 3D landscape is built in Rust ([`crates/core/src/world`](../../crates/core/src/world)), from the route, elevation data and OpenStreetMap. The page only uploads the buffers it's given to the GPU (see [Drawing](drawing.md)). All the geometry is computed off the render thread: the road ribbon, terrain chunks, the distant sheet, buildings, bridges, tunnels, barriers and trees.

## The inputs

| Input | Where it comes from | Code |
|---|---|---|
| **The route** | [Routes](routes.md) | `gpx.rs` |
| **Elevation** | "Terrarium" tiles (Tilezen data on AWS Open Data, no key): heights packed into RGB PNGs. Zoom 12, dropping a level until the route and a 6.5 km margin fit in 64 tiles; all fetched in parallel, 15 s budget. Not cached. | `dem.rs` (`Dem::plan`, decoding, sampling), `src-tauri/src/terrain.rs` (fetching) |
| **Buildings, roads, forests, water, bridges, tunnels** | OpenStreetMap vector tiles (Shortbread schema, zoom 14) from `vector.openstreetmap.org`: at most 500 tiles and 80,000 buildings, 40 s budget | `mvt.rs`, `src-tauri/src/mapdata.rs` |
| **Detail the tiles lack** | The Overpass API: building heights and levels, the route's own road tags (surface, width, lanes, trail grades), field boundaries, fords, places and points of interest. Three public servers are asked in parallel: overpass-api.de, overpass.openstreetmap.fr and overpass.private.coffee. The first good answer within 12 s wins, 45 s overall. | `mapdata.rs`, `scenery.rs`, `buildings.rs` |

Map data never fails a ride. Offline, it falls back to what's cached, then to nothing: procedural hills, no buildings, a tarmac road.

### Caching

Map data is cached per route in the store (see [Data and diagnostics](data-and-diagnostics.md)):

- `buildings-<route id>`: footprints, refreshed after 180 days. If only outlines came back (no Overpass detail), they're refreshed after 14 days.
- `scenery-<route id>`: everything else, refreshed after 180 days.

Each cache carries a format number: `FORMAT` (buildings, now 2) and `SCENERY_FORMAT` (now 4) in `mapdata.rs`. **Bump the number whenever the parsed shape or its meaning changes**, and old caches are dropped and fetched again. The route's own road tags have their own number, `WAY_TAGS` (now 4, in `world/surface.rs`). When it's out of date, only those tags are fetched again, at most once a day, without throwing away the rest.

## The coordinate frame

Everything is in the route's local metres: `x` east, `z` south, `y` up, with the origin at the route's centroid (see [Routes](routes.md#the-route)). The renderer uses the same frame, so no conversion happens anywhere.

## The height field

[`height.rs`](../../crates/core/src/world/height.rs) answers "how high is the ground here?" for any `(x, z)`, and everything else stands on it.

- **On and near the road** the ground is pinned to the route's own height, so the road never floats or sinks. The ground is flat at road level within 16 m of the centre line, a terrain cell beyond the widest road, so no cell can tilt up over its edge.
- **Beside it** the ground follows the shape real roads are built to: a cutting (slope 1.2, about 50°, steep enough to show as rock) where the terrain is above the road, an embankment (slope 0.67, about 34°, grassed) where it's below. Below the road the ground may fall away from 8 m out. Faces are at most 7 m high: most of a bigger difference is the terrain data being coarse, so it's blended away instead, and from 90 m out the ground is the terrain whatever.
- **Further out** it's the elevation data, shifted to match the route (`dem_offset`). Without data, it's procedural hills (fractal noise).
- **Under bridges** the embankment isn't built, so the valley shows. **Over tunnels** the hill stays, with at least 7 m of cover and 5.4 m of headroom.
- **Water** is cut 1.6 m deep, its surface 0.5 m below the banks.
- **A velodrome's grounds** are laid level.

## Chunks

The ground is drawn as square chunks of 240 m, each a grid of 24 by 24 cells (10 m cells), streamed in around the rider (`World::chunk`). Six chunks out in every direction are kept. Beyond that a coarse **far sheet** of 96 by 96 cells of 150 m reaches the horizon (`World::far`). `WorldLayout` sends these numbers to the page, so they're set in one place.

A chunk (`GridMesh`) holds:

- **Ground:** positions, normals, and vertex colours from `shade`: meadow tinted by noise, a worn verge beside the road, rock on steep faces, scree and then snow above the tree line, and forest floor, beach or shallows from the map's land cover.
- **Trees:** position, kind, size and turn of each tree (see [Trees](#trees)).
- **Buildings:** positions, normals, colours and texture coordinates for walls and roofs.
- **Water:** lakes, rivers and streams, with the shallows lighter.
- **Roads:** the map's other roads, paths and railways (see [Other roads](#other-roads)).

A chunk takes tens of milliseconds to build. The smoke harness prints how long.

## The route's own road

The road ridden is a separate ribbon (`World::road` → `RoadMesh`) along the route samples, `ROAD_WIDTH` 6.5 m wide by default, in sections of 6 m. Each vertex has a **look** that the page's road material uses to pick a texture:

| Look | Surface |
|---|---|
| 0–4 | Tarmac, concrete, cobbles, gravel, dirt |
| 5 | Tarmac with no centre line (lanes) |
| 6 | Timber (a log bridge) |
| 7 | Singletrack |
| 8 | A ford's stones |
| 9 | A velodrome's track, painted, its inside edge on the right |

### Matching the route to the map

[`scenery.rs`](../../crates/core/src/world/scenery.rs) works out which mapped road the route is on, sample by sample. A mapped way matches if it's within 6 m and runs the same way (direction cosine 0.94 or better) for at least 40 m. A match decides, for each sample:

- **Surface** ([`surface.rs`](../../crates/core/src/world/surface.rs), `Surface::from_osm`): `surface=asphalt` and the like → tarmac; `concrete*` → concrete; `sett`, `cobblestone`, `paving_stones` and the like → cobbles; `gravel`, `fine_gravel`, `compacted` → gravel; `dirt`, `earth`, `ground`, `grass`, `mud` → dirt. With no tag, a track or path defaults to dirt or gravel and a road to tarmac. The route's own road tags come from Overpass: a 15 m strip round the route, sampled every 25 m, in stretches of 20 km.
- **Width:** a `width` tag if there is one; otherwise `lanes × 3.25 + 0.5`; otherwise a figure for the road's class (`highway_width`: primary 7 m, residential 5 m, track 3 m, path 2 m, and so on). It's clamped to 2.4–11 m, and changes only after a run of at least 30 m.
- **Centre line:** only on main roads (motorway, trunk, primary, secondary and tertiary, and their slip roads, `highway_marked`). Anything smaller is tarmac with no line (look 5).
- **Trail grade:** a path or track counts as a trail (`is_trail`). Its grade comes from `mtb:scale` (or `mtb:scale:imba`), else is estimated from the hiking scale `sac_scale` (0–4). Steps count as S2. Trails narrower than 2.5 m are **singletrack**, 1.5 m wide unless tagged.
- **Off the map:** a route more than 100 m from any mapped way (a field, a beach) becomes a 3 m dirt track.
- **Roundabouts:** a closed ring of road under 450 m round is treated as a one-way roundabout, whatever the map says about its direction, so the route is matched to it the right way round.

The `[ride] surfaces … trails … berms … fords … widths …` line in the diagnostics log says what was found.

### Berms

On graded trails, tight turns are banked ([`berms.rs`](../../crates/core/src/world/berms.rs)). The radii are those of the route as ridden, after its corners are rounded (a switchback of a few metres becomes a bend of about 12–15 m). Bends tighter than 30 m start to bank, reaching the full angle at 12 m or tighter, up to 35°. The outside edge rises at most 1 m; the inside edge stays at the trail's height, so the ground beside it is undisturbed. Banks under 2° are left flat. The bank is stored per sample as a cross-slope, and riders stand and lean on it.

### Fords

Where a trail crosses a mapped stream with no bridge, it goes through the water ([`fords.rs`](../../crates/core/src/world/fords.rs)). The trail dips 0.3 m at the crossing, easing back over a sample either side. Its bed is stones (look 8), and the stream runs across it a little below the trail's surface either side, so the water is deepest in the middle. Streams within 15 m are brought down to the ford's level. Riding through is heavier and slower (see [Physics](physics-and-trainer-control.md#the-road-surface)).

### Bridges and tunnels

The map's `bridge=*` and `tunnel=*` ways are matched to the route like roads, but more loosely: within 9 m, direction cosine 0.85. A "tunnel" under 50 m is an underpass, so it's drawn as the road going under a bridge. [`structures.rs`](../../crates/core/src/world/structures.rs) builds:

- **Bridges:** parapets and piers. On a trail, a log bridge with a timber deck.
- **Tunnels:** the bore, the portals, and the land over them. The page dims the light inside (`covered` in `RideInit`).
- **Overbridges:** other roads and railways crossing above the route.

Lit and unlit parts come back separately (`ride_structures`).

## Other roads

[`roads.rs`](../../crates/core/src/world/roads.rs) lays the map's other roads, paths and railways within 700 m of the route (paths within 250 m) on the terrain. Each piece is cut along the terrain mesh's own triangles, so it lies exactly on the ground however that folds. They're drawn as part of each chunk.

## Buildings

[`buildings.rs`](../../crates/core/src/world/buildings.rs) extrudes OpenStreetMap footprints within 350 m of the route into simple blocks standing on the terrain. Height comes from `height`, or `building:levels` × 3 m + 1 m, else a typical height for the building's kind (3 m for a shed, 6.5 m for a house, and so on), between 2.5 and 150 m. Underground buildings and canopies (`building=roof`) are left out. Nothing is placed within 5.5 m of the road, so a badly mapped footprint can't block it. A small footprint that is close to a rectangle gets a pitched roof with the ridge along its longer side; the rest are flat. Walls use a window texture, darker at the foot than at the eaves.

## Trees

[`trees.rs`](../../crates/core/src/world/trees.rs): six kinds (spruce, pine, broadleaf, birch, a narrow column like a cypress or poplar, and a bush), each one shared low-poly model drawn many times over.

- **Where:** mapped woods are filled on a 5 m lattice at density 0.75. Elsewhere about 180 trees are scattered per chunk. Hedgerows have trees growing out of them too. None grow on the road, in water or on a velodrome's grounds.
- **Which:** the mix follows altitude and latitude. Conifers take over towards the tree line and in the north (from 52° to 62°); broadleaves in the middle latitudes; columns south of about 44°. The **tree line** is `3600 − 38 × |latitude|` metres, at least 600 m.
- **Far away:** from 200 m, with the switch spread over 30 m so it isn't seen as one line the page draws trees as camera-facing sprites (`sprite_frame`).

## Fields

[`fields.rs`](../../crates/core/src/world/fields.rs): field boundaries mapped near the route (`barrier=hedge|wall|fence`, queried within 120 m) stand where the map has them. Elsewhere, within 1 km of the route, a pattern of fields is made up in the style of the region (`Region::at`):

| Region | Boundaries |
|---|---|
| Britain and Ireland | Hedgerows, with a tree growing out of one point in twenty; dry-stone walls in the uplands |
| Normandy, Brittany and the Loire | Bocage: small fields, all hedged |
| The Mediterranean | Terraces and stony fields: mostly dry-stone walls, some hedges |
| Scandinavia | Timber fences |
| The rest of Europe | Big open fields, the odd hedge or fence |
| Everywhere | Timber fences in the mountains |

In hedge country, lanes up to 6 m wide and below 260 m run between **hedge banks** with a gateway now and then. Higher up the lanes cross open moor.

## Barriers

[`barriers.rs`](../../crates/core/src/world/barriers.rs): crowd barriers along both sides of the route, as if the road were closed for a race. They set the road being ridden apart from the map's road network. Sections are 2.5 m long, set back 0.55 m from the road's edge. On bends tighter than 30 m radius (hairpins) they become netting fences. There are none on bridges (they have parapets), in tunnels, along trails or at a velodrome. `ride_barriers` returns `[x, y, z, yaw, distance, look, pitch]` per section, and the page draws only those near the rider.

## Landmarks

[`landmarks.rs`](../../crates/core/src/world/landmarks.rs): signs along the route. Place names where the road enters a town or village, names and heights at the summits of the climbs, and points of interest by the roadside (cafés and the like), each with its distance along the route and the side of the road it stands on. Places come from the map tiles' labels, and must be close to the route to get a sign.

## The velodrome

[`venue.rs`](../../crates/core/src/world/venue.rs) builds a velodrome round a built-in track's route (`World::with_venue`):

- **The track:** as wide as the real one, with tarmac everywhere, no trails or fords. Banked 26° on the bends and 7° on the straights (Herne Hill's 1992 rebuild; published figures range from 18° to 30°), with the banking eased in through the transitions.
- **Markings:** the lines painted across the track: the finish line (white with a black line down the middle), the 200 m line, and the red pursuit lines half-way along each straight.
- **Round it:** the railing, floodlights, the grounds laid level, the stands, the pavilion and the other buildings, placed where they stand at the real track.

Nothing grows on the grounds, and no fields are made up there.

## Adding to the world

- Keep it in `crates/core`, and add a test that builds a small world and checks one fact (see `crates/core/tests/core.rs`).
- If the new thing comes from map data, bump `SCENERY_FORMAT` (or `FORMAT`, or `WAY_TAGS`) so saved routes fetch it.
- Anything per route sample (like `widths`, `surfaces`, `banks`) lives on `Scene`, and goes to the page in `ride_route_data`. If you add one, add it there and read it in `web/src/world/scene.ts`.
- Check it with the smoke harness on a route that has it (see [Good test routes](building-and-testing.md#good-test-routes)).
