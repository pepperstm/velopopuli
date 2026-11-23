# Tropical Style — Phases 1b and 2 (open data) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put the real things along a tropical route where they really are — power lines, street lamps, cycle lanes, concrete median barriers, water towers (OpenStreetMap), the houses OSM lacks (Google Open Buildings / Microsoft via Overture Maps), the real trees (Meta/WRI canopy height), lamps and poles seen by Mapillary, and water and wetland (ESA WorldCover) — from a per-route open-data pack prepared by a script.

**Architecture:** A Python tool (`tools/opendata`, run with `uv`) reads a route from the app's data folder, fetches only the route's corridor from each open source and writes one JSON pack, `data/opendata-<route id>.json` (store key `opendata-<route id>`). The app loads it when a ride starts: buildings join the footprints, water and wetland join the scenery areas, and `World::with_open_data` keeps the rest in local metres for the renderers (props through the barrier pipeline, cables and cycle lanes in the chunk meshes, trees in the chunk tree lists). No pack: everything as in Phase 1.

**Tech Stack:** Python 3.11+, uv, duckdb (Overture GeoParquet over S3), rasterio (Cloud-Optimized GeoTIFF windows), shapely, numpy, requests; Rust (crates/core, src-tauri).

**Spec:** `docs/superpowers/specs/2026-11-22-tropical-style-design.md` (Spanish) — "Fase 1b" goals 4, 5, 6, 9 (mapped part), 10-adjacent (water towers), and "Fase 2". Phase 1 plan: `docs/superpowers/plans/2026-11-22-tropical-style-phase1.md`.

## Global Constraints

- Only open sources: OSM via Overpass (ODbL), Overture Maps buildings release `2026-11-18.0` (ODbL; includes Google Open Buildings and Microsoft ML Buildings), Meta/WRI canopy height v1 (CC BY 4.0), ESA WorldCover 10 m 2021 v200 (CC BY 4.0), Mapillary map features (needs the user's free token; skipped without one). Nothing from Google Maps / Street View.
- Corridor: ±500 m of the route for buildings and trees; ±300 m for power lines, lamps, jersey barriers; ±2 km for water towers.
- OSM buildings stay authoritative: the pack takes Overture buildings whose first source is not OpenStreetMap.
- The app's Overpass query (`world/surface.rs::way_query`) and `WAY_TAGS` stay untouched; the script makes its own Overpass requests with retries (3 servers, 60 s timeout, up to 3 rounds).
- Pack used only when the route's roadside plan is tropical (Phase 1 rule: everything new only in the lowland tropics). Outside the tropics a pack is ignored.
- Real objects win: rule lamps skip within 19 m (half the 38 m spacing) of a real lamp; rule jersey sections skip within 5 m of a mapped one.
- Attribution for every CC BY source in README "Data and credits" and in the pack's `sources` list.
- Python: `uv`, type hints on public functions, `logging`, files under ~400 lines, specific exceptions.
- Commits: the user OK'd committing for this work; Conventional Commits with the attribution trailer.

## Review Focus

1. **Pack missing, empty, or from an older format** — rides behave exactly as Phase 1; `format` other than 1 is ignored with a log line. → Task 1 test.
2. **Huge corridors** (a 100 km route) — the script must not try to download whole regions: per-source bbox limits and corridor filtering. → Task 7 test on corridor building.
3. **A source down** (Overpass, S3, Mapillary) — the script still writes a pack with the other sources and says which failed. → Task 7 test.
4. **Duplicates** — OSM lamp and Mapillary lamp at the same spot, Overture building over an OSM one, canopy tree on a road or in a house. → Tasks 6 and 7 tests.
5. **Power lines crossing the road** — cables span the road overhead, supports never stand on the road. → Task 3 test.

---

## File Map

| File | Change | Responsibility |
|---|---|---|
| `crates/core/src/world/opendata.rs` | create | `OpenData` (pack, serde), `OpenScene` (local metres), conversions |
| `crates/core/src/world/roadside.rs` | modify | real lamps, rule fill, mapped jersey, towers/poles/water towers as props |
| `crates/core/src/world/barriers.rs` | modify | looks `TOWER`, `POLE`, `WATER_TOWER` + models |
| `crates/core/src/world/scenery.rs` | modify | `RoadKind::Cycle`; cycle-lane strips; sidewalk offset past a cycle lane |
| `crates/core/src/world/mod.rs` | modify | `World.open`, `with_open_data`, cables and open trees in `chunk` |
| `src-tauri/src/session.rs` | modify | load the pack, merge buildings and areas, call `with_open_data` |
| `tools/opendata/` | create | the preparation script (package `opendata`, tests) |
| `README.md` | modify | how to run the script; credits |
| `crates/core/tests/opendata.rs` | create | Rust tests |

---

### Task 1: The pack and its loading

**Files:** create `crates/core/src/world/opendata.rs`; modify `world/mod.rs`, `src-tauri/src/session.rs`; test `crates/core/tests/opendata.rs`.

**Interfaces — Produces:**
```rust
#[derive(Debug, Clone, Default, Deserialize, Serialize)] #[serde(default)]
pub struct OpenData {
    pub format: u32,                       // 1
    pub sources: Vec<String>,
    pub power_lines: Vec<PowerLine>,       // { voltage_kind: String /* "line" | "minor_line" */, points: Vec<[f64; 2]>, supports: Vec<String> /* "tower" | "pole" | "" per point */ }
    pub lamps: Vec<[f64; 2]>,              // [lat, lon]
    pub cycleways: Vec<Vec<[f64; 2]>>,     // lines of cycle lanes or tracks along roads
    pub jersey: Vec<Vec<[f64; 2]>>,
    pub water_towers: Vec<[f64; 2]>,
    pub buildings: Vec<OpenBuilding>,      // { ring: Vec<[f64; 2]>, height: f32 }
    pub trees: Vec<[f64; 3]>,              // [lat, lon, height m]
    pub water: Vec<Vec<Vec<[f64; 2]>>>,    // areas (rings) of water and wetland
}
pub const OPEN_DATA_FORMAT: u32 = 1;
impl OpenData {
    pub fn usable(&self) -> bool;                       // format == 1
    pub fn footprints(&self) -> Vec<Footprint>;         // ids from 1 << 62 upward
    pub fn areas(&self) -> Vec<Area>;                   // AreaKind::Water
}
impl World { pub fn with_open_data(self, data: &OpenData) -> Self; pub fn open(&self) -> Option<&OpenScene>; }
```
`OpenScene` holds everything in local `(x, z)` (lamps, supports with kind, cable spans, cycle runs, jersey runs, water towers, trees bucketed by chunk). `with_open_data` is a no-op unless `tropical_weight(route.lat0.abs(), 0.0) > 0.5` or `!data.usable()`.

- [ ] Tests: `pack_parses_and_converts` (a JSON literal with one of each → 1 footprint, 1 water area, lamps in `open()`), `older_or_empty_packs_are_ignored` (`format: 0` → `open()` is None), `packs_are_ignored_outside_the_tropics`.
- [ ] Implement; session: `let open: Option<OpenData> = store.get(&format!("opendata-{route_id}")).filter(OpenData::usable);` — append `open.footprints()` to `map.footprints` and `open.areas()` to a cloned scenery's `areas` before `World::with_map`; chain `.with_open_data(&open)` before `.with_roadside(..)`.
- [ ] `cargo test -p velopopuli-core`, `cargo build -p velopopuli`; commit `feat(world): load a route's open-data pack`.

### Task 2: Real street lamps, rule lamps fill the gaps

**Files:** `world/roadside.rs`; test `tests/opendata.rs`.
- Real lamps within 30 m of the route become `LAMP_SINGLE` props at their position, arm towards the road (yaw from the nearest route sample). Rule lamps (Phase 1) skip a spot within 19 m (along the road) of any real lamp's nearest route distance.
- [ ] Tests: `real_lamps_stand_where_mapped` (corridor + pack with 3 lamps at 200, 260, 320 m, 12 m west → 3 single lamps at those x/z ±0.5 m), `rule_lamps_leave_room_for_real_ones` (no rule double lamp within 19 m of 260 m).
- [ ] Implement; tests; commit `feat(world): real street lamps from open data`.

### Task 3: Power lines

**Files:** `world/barriers.rs` (looks `TOWER = FENCE + 4`, `POLE = FENCE + 5`, `WATER_TOWER = FENCE + 6`, `LOOKS = FENCE + 7`; models: lattice-looking tower 30 m with three cross-arms, concrete pole 11 m with a cross-arm, water tower 22 m: four legs and a cylinder tank), `world/opendata.rs` (spans), `world/mod.rs` (`chunk`: cables).
- Supports: every pack point with a `tower`/`pole` support within 300 m of the route → prop at its place (not on a road strip: shift 2 m off when it would be). Cables: between consecutive supports, three conductors (tower: arms ±6 m at 27 m; pole: ±1 m at 10 m), each a chain of 8 segments sagging `0.03 × span` at mid-span, drawn as 0.06 m square tubes into the chunk building mesh of the segment's middle.
- [ ] Tests: `towers_stand_on_their_mapped_spots`, `cables_hang_between_towers` (chunk mesh has vertices above 20 m along the span), `no_tower_in_the_road` (a support mapped on the road centre is moved ≥ half width + 1 m off).
- [ ] Implement; tests; commit `feat(world): power lines from open data`.

### Task 4: Cycle lanes

**Files:** `world/scenery.rs` (`RoadKind::Cycle` — width 1.6, colour sRGB `[0.20, 0.55, 0.30]`, not banked, not marked; every exhaustive `match` on `RoadKind` gets an arm), `world/opendata.rs`, `world/mod.rs` (`with_open_data` lays them).
- A route sample has a cycle lane on a side when a pack cycleway runs within 12 m of that side, nearly parallel (cos ≥ 0.94), for ≥ 40 m. The lane is a `Cycle` strip from `half_width + 0.1` to `+ 1.7` on that side. Open-road sidewalks on that side start 1.7 m further out.
- [ ] Tests: `cycle_lane_runs_beside_the_road` (green strip vertices at half+0.1…1.7 on the mapped side only), `sidewalk_moves_out_past_the_cycle_lane`.
- [ ] Implement; tests; commit `feat(world): cycle lanes from open data`.

### Task 5: Mapped median barriers and water towers

**Files:** `world/roadside.rs`.
- Mapped jersey ways within 60 m of the route: `JERSEY` sections every 2.5 m along them (yaw along the way). Rule jersey sections skip within 5 m of a mapped one. Water towers within 2 km: `WATER_TOWER` prop.
- [ ] Tests: `mapped_jersey_replaces_the_rule`, `water_tower_stands_where_mapped`.
- [ ] Implement; tests; commit `feat(world): mapped median barriers and water towers`.

### Task 6: Real trees

**Files:** `world/mod.rs` (`chunk`), `world/opendata.rs`.
- In chunks whose centre is within 380 m of the route and that the pack's trees cover (the pack lists `trees_corridor_m`, default 500), the procedural "elsewhere" trees are not planted; instead each pack tree in the chunk that passes `plant()` (not on roads, water, houses) grows with its kind from `Climate::kind_at` (seeded by position) and height scale `clamp(height / model_height(kind), 0.6, 1.8)`. Mapped forests stay as they are.
- [ ] Tests: `canopy_trees_stand_where_measured`, `no_canopy_tree_on_the_road_or_in_a_house`, `chunks_outside_the_corridor_keep_their_generated_trees`.
- [ ] Implement; tests; commit `feat(world): real trees from canopy height`.

### Task 7: The preparation script

**Files:** create `tools/opendata/pyproject.toml`, `tools/opendata/opendata/{__init__,cli,route,corridor,osm,buildings,trees,landcover,mapillary,pack}.py`, `tools/opendata/tests/test_*.py`, `tools/opendata/preparar_datos.command`.
- `uv run opendata --route <id> [--data-dir DIR] [--mapillary-token T]`; default data dir: `$VP_DATA_DIR` or `~/Library/Application Support/org.velopopuli.VeloPopuli` (macOS), `%APPDATA%\org.velopopuli.VeloPopuli` (Windows), `~/.local/share/org.velopopuli.VeloPopuli` (Linux). Reads `data/route-<id>.json` (`gpx`), writes `data/opendata-<id>.json` atomically (temp file + rename).
- `route.py`: GPX → list of (lat, lon); `corridor.py`: buffered corridor polygon in a local metric projection, bbox, `contains`.
- `osm.py`: Overpass (servers overpass-api.de, overpass.openstreetmap.fr, overpass.private.coffee; UA `velopopuli-opendata/0.1`; 60 s; 3 rounds): power lines with support nodes, street lamps, cycle tracks/lanes (`highway=cycleway`, ways with `cycleway*=lane|track`), jersey barriers, water towers.
- `buildings.py`: duckdb over `s3://overturemaps-us-west-2/release/2026-11-18.0/theme=buildings/type=building/*` with the corridor bbox; drop `sources[1].dataset == 'OpenStreetMap'`; keep those intersecting the corridor; height = `height` or `num_floors × 3` or 6.
- `trees.py`: tiles from `forests/v1/alsgedi_global_v6_float/tiles.geojson`, windows of `chm/<tile>.tif` over the corridor bbox, mask to corridor, local maxima ≥ 3 m in a 5 m neighbourhood (one tree per crown), at most 200 000.
- `landcover.py`: WorldCover tile names `ESA_WorldCover_10m_2021_v200_{N|S}{lat:02d}{E|W}{lon:03d}_Map.tif` (3° tiles, lower-left corner), classes 80 (water) and 90 (wetland) → polygons ≥ 2 000 m², simplified 5 m, clear of 15 m around the route.
- `mapillary.py`: vector tiles `https://tiles.mapillary.com/maps/vtp/mly_map_feature_point/2/14/{x}/{y}?access_token=…`, values `object--street-light` → lamps, `object--support--utility-pole` → poles (kept only within 30 m of the route); skipped without a token.
- `pack.py`: assemble, dedupe lamps within 5 m, record `sources` (only the ones that answered) and `failed`, write.
- [ ] Tests (pytest, no network): corridor bbox and containment, GPX parsing, local maxima on a synthetic canopy array, lamp dedupe, WorldCover tile naming, pack JSON matches the Rust `OpenData` fields, a failing source leaves the others in the pack.
- [ ] `uv run pytest`; commit `feat(tools): open-data preparation script`.

### Task 8: End to end

- [ ] Run the script for the Plaza Lagos → Samborondón test library; report counts per source.
- [ ] Rebuild; captures at km 3, 8, 18, 26.5; compare with Phase 1 captures.
- [ ] README: "Open data for a route" section and credits; `npm run check`; commit.
