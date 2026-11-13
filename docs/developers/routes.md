# Routes

A route is a GPX file turned into evenly spaced samples, each with a position, height and gradient. Everything else (physics, the world, climbs, ghosts, segments) is worked out from those samples. The code is in [`crates/core/src/gpx.rs`](../../crates/core/src/gpx.rs).

## The `Route`

| Field | Meaning |
|---|---|
| `step`, `n`, `total` | Samples are `step` metres apart (5 m), `n` of them, `total = (n − 1) × step` metres |
| `lat`, `lon` | Each sample's position |
| `x`, `z` | Each sample in local metres: `x` east and `z` **south** of `lat0, lon0`, the route's centroid. This is the 3D ground plane; `y` is up. |
| `ele`, `grade` | Smoothed height (m) and gradient (a fraction: 0.05 is 5%) |
| `ascent`, `descent`, `min_ele`, `max_ele` | Totals |
| `climbs` | Detected climbs (below) |
| `has_elevation` | Whether the GPX had heights. If not, they come from the terrain at ride start. |
| `closed` | A loop: the end is within 15 m and 5 m of height of the start |

The projection is equirectangular about the centroid (`x = Δlon · cos(lat0) · M_PER_DEG`). It's accurate to well under a metre over any distance anyone rides indoors. `to_local` and `to_lat_lon` convert both ways.

## Building one

`build_route(name, points)` (with `step` = 5 m):

1. **Parse** (`parse_gpx`): track points, or route points if there are none, and the name. `parse_gpx_track_times` reads timestamps too, for ghosts.
2. **Drop duplicates** closer than 0.5 m, so every segment has a direction. A route needs at least two points and 100 m.
3. **Round the corners** (`round_corners`): resample the line every 5 m, then average each point with two neighbours either side, twice. Planner exports and sparse GPS tracks draw a hairpin as straight lines meeting at a point, which nobody rides. The ends stay where they are.
4. **Fill missing heights** by interpolating along the distance between known ones. The ends take the nearest known height.
5. **Resample** at exactly `step` and project to `x, z`.
6. **Finish the heights** (`finalize_elevation`):
   - smooth `ele` with two passes of a box filter of radius 30 m, which approximates a Gaussian. GPS and barometric heights are noisy at the metre scale, and the trainer would twitch. On a loop the filter wraps round, so start and finish end up at the same height.
   - `grade[i]` is the height change across ±10 m, clamped to ±25%.
   - total ascent and descent, lowest and highest point, then the climbs.

When heights come from the terrain instead (`apply_elevation`, called from `dem.rs`), they're smoothed over 100 m. Terrain data is tens of metres per cell, which on a road cut into a hillside reads as wild swings in gradient. Across bridges and tunnels the terrain is ignored and the road runs evenly from about 30 m outside one end to 30 m outside the other: the terrain goes over the hill a tunnel goes through and down into the valley a bridge crosses.

When the GPX has its own heights, the terrain is shifted to match the route instead (`align_dem_with`: the median difference along the route), so the road sits on the ground.

## Climbs

`detect_climbs` walks the gradient:

- A climb **starts** where the gradient reaches 2%.
- It **ends** at the highest point, once the road has dropped 10 m below it or run 300 m past it without going higher.
- It's **kept** if it gains at least 30 m over at least 400 m at an average of at least 3%.
- Its **category** comes from length × average gradient in percent:

| Score (m × %) | Category |
|---|---|
| ≥ 80,000 | HC |
| ≥ 64,000 | 1 |
| ≥ 32,000 | 2 |
| ≥ 16,000 | 3 |
| ≥ 8,000 | 4 |
| less | uncategorised |

Alpe d'Huez (about 13.8 km at 8.1%) scores about 112,000: HC. Each climb becomes a timed segment for the ride (see [The ride](the-ride.md#climbs-and-segments)).

## Sampling along the route

- **Rust** (`Route::sample_at`) interpolates linearly between samples. That's all the physics needs.
- **The page** ([`web/src/core/gpx.ts`](../../web/src/core/gpx.ts), `sampleAt`) uses a Catmull-Rom curve through the samples for position, and the curve's tangent for heading. Linear interpolation turned the rider in steps at each sample, which showed in tight bends. The native renderer does the same in `native/mod.rs` (`sample`, `curve`).
- **Lean** (`leanAt`): the lean that balances the turn, `atan(v² κ / g)`. The curvature κ is the change of heading over ±4 m, divided by 8 m. It's capped at 40°, and negative leans left. Slow hairpins hardly lean; fast sweepers lean a lot, as outdoors.
- **Lap rides wrap**: `wrapDist` and `wrapGap` in `gpx.ts` read distances round the loop. Anything comparing distances on a lap ride must use them (see [The ride](the-ride.md#laps)).

## Where routes come from

| Source | How | Code |
|---|---|---|
| **A GPX file** | **Import GPX…** or dragging files on. The route is built once to check it, then the GPX text is stored. | `routes_import_dialog`, `routes_import_text`, `import_gpx` in `commands.rs` |
| **Planned on the map** | Each section between waypoints goes to [BRouter](https://brouter.de) in its own style, and comes back with heights and the tags of every way (see [Planning on the map](#planning-on-the-map)) | `geo_plan` → `geo::plan`, [`planner.rs`](../../crates/core/src/planner.rs) |
| **A loop** | BRouter's round-trip mode, turned into waypoints | `geo_round_trip` → `geo::round_trip` |
| **Place search** | [Nominatim](https://nominatim.openstreetmap.org) | `geo_search` → `geo::search` |
| **Famous climbs** | Eight start and finish points (`geo::CLASSICS`), planned through BRouter like any route | `geo_classics` |
| **Signposted road routes, MTB routes** | OpenStreetMap route relations (`route=bicycle`, `route=mtb`) in the area on the map, through Overpass | `geo_waymarked_routes`, `geo_waymarked_route`, [`waymarked.rs`](../../crates/core/src/waymarked.rs) |
| **Your Strava routes** | The connector's `saved_routes` and `route_gpx` | `connector_routes`, `connector_route` |
| **A Dreeve ride** | The connector's `list_rides` and `ride_gpx` | `connector_ride_list`, `route_import_connector` |
| **Built in** | The Demo Loop (`demo_route`, a synthetic 16 km loop with one climb) and Herne Hill (`velodrome.rs`) | `load_route` in `session.rs` |
| **A group ride** | The host's GPX, fetched from the relay | `group_fetch_route` |

### Planning on the map

The Explore screen ([`Explore.tsx`](../../web/src/ui/Explore.tsx)) keeps a plan: the waypoints, and a style for each section between them (`styles[i]` is the section from waypoint `i`). Every change (adding, inserting, moving or removing a waypoint, reversing, changing a style) is one step on an undo list, and the plan is sent to `geo_plan` 300 ms after the last change.

**Styles** are BRouter profiles, listed in `planner::STYLES`:

| Shown as | Profile |
|---|---|
| Road bike | `fastbike` |
| Quiet roads | `fastbike-verylowtraffic` |
| Gravel | `gravel` |
| Touring | `trekking` |
| Safest | `safety` |
| Mountain bike | `mtb` |

An unknown name falls back to `fastbike`.

**Sections are planned one at a time** (`geo::plan`). Each section is one BRouter request, asked for as GeoJSON, four at once. The reply is read by `planner::parse_brouter` into a `Leg`: the points with their heights, and *stretches*, each running to a point index with the OSM tags of its way. Those come from BRouter's `messages` table, whose rows end at a point of the line. Each section is kept in memory by style and end points (`SECTIONS`, up to 400), so moving one waypoint only plans the two sections that touch it.

`planner::join` puts the sections end to end, keeping their shared point once. That gives the GPX and the route, built like any other. From the joined leg:

- **The breakdown** (`planner::breakdown`): each stretch is classified by `classify` into a surface (paved, cobbles, gravel, dirt, not mapped) and a kind of way (main road, minor road, cycle path, track, path). The surface comes from `surface=` through `Surface::from_osm`, from `tracktype=` on a track, and is taken as paved on an untagged road. Neighbouring stretches of the same kind become one *run*, with its own thinned line, so the map can colour the route by either. The shares are in metres, largest first.
- **The outline** (`planner::outline`): the thinned line for the map, keeping the points where sections meet. `ends` says where they are, so the page can tell which section a point on the line belongs to, and dragging the line inserts a waypoint into that section.
- **The time** (`planner::ride_time`): the route ridden in the app's own physics at three quarters of the rider's FTP, with their weight, bike, CdA and rolling resistance. With road surfaces on in Settings, each surface's rolling factor counts too (`rolling_lookup`), so gravel is slower. Waymarked and Strava routes get a time too, on tarmac.

**Loops** (`geo::round_trip`) use BRouter's round-trip mode (`engineMode=4`) with a radius and a heading. A loop comes out about 6.4 times the radius (`planner::round_trip_radius`); if one is more than 20% off the distance asked for, it's asked again once with the radius corrected. The loop is then cut into 4 to 12 waypoints about an eighth of its length apart (`planner::loop_waypoints`), and each piece is put in the section memory under its style and ends. So the page gets back ordinary waypoints, planning them gives exactly the loop, and the rider can edit it like any route. **Try another** turns the heading a little each time, or picks a new one at random.

### Waymarked routes

`area_query` asks Overpass for route relations of one kind in the bounds (route relations only: a superroute such as a EuroVelo route holds other relations, not ways). The area may be about 60 km across at most (0.6° by 0.9°). Anything bigger holds too many routes for a public server, and the app asks the rider to zoom in. At most 60 routes are listed.

When one is picked, `route_query` fetches its ways and `join` strings them into one line. Ways whose ends are within 20 m count as meeting; elsewhere the line jumps straight across, and the number of such gaps is shown. A road route can be a national one hundreds of kilometres long, so only the part inside the searched area is fetched (`clip`). If that leaves separate pieces more than 1.5 km apart, the longest piece is kept (`longest_stretch`) and the route is marked `clipped`. The result is written as a GPX (`to_gpx`) and saved like an imported route.

## The velodrome

Herne Hill is a route built in code ([`velodrome.rs`](../../crates/core/src/velodrome.rs)): an oval of two straights and two semicircles round the track's measuring line, at its real place. The route is the measuring line itself, so a lap is exactly the track's official 450 m. Its samples are 1 m apart (not 5 m) so the bends stay round. It's flat (`ele` 0), starts on the line on the home straight, and has two ids: `herne-hill` (anticlockwise, as raced) and `herne-hill-clockwise`. `track_for(id)` maps an id to the track and the direction. The banked track, its paint and the buildings round it are part of the world (see [The world](the-world.md#the-velodrome)).

## The library

- `routes` in the store is the list the Routes page shows: `RouteMeta { id, name, distance, ascent, climbs, addedAt, preview }`. `preview` is the outline drawn on the card, at most 160 points.
- `route-<id>` holds `{ name, gpx }`: the original GPX text, so a route can be rebuilt if the route code changes.
- Imported routes get a UUID. The demo and velodrome ids are fixed.

A route is rebuilt from its GPX each time a ride starts (`load_route`). That's quick, and it means improvements to route building reach every saved route.
