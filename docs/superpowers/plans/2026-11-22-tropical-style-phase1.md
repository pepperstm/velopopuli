# Tropical Style — Phase 1 (rules) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make routes in the lowland tropics look like the real place (palms, rain trees, yellow road lines, planted medians with double-arm lamps, concrete median barriers, open-road sidewalks, residential walls) using rules only, with no change to routes outside the tropics.

**Architecture:** A new `world/tropics.rs` answers "is this tropical / in the Americas". A new `world/roadside.rs` classifies every route sample into a stretch type (avenue, periurban, rural, town) from data the world already has (route road marking and width, one-way carriageways from the vector tiles, building footprints) and places rule-based props. Props reuse the existing instanced barrier pipeline (`World::barriers()` → `ride_barriers` → `web/src/world/barriers.ts`); palms reuse the tree pipeline; road paint reuses the route ribbon looks. The plan is computed once per ride via `World::with_roadside`, called from `session.rs`.

**Tech Stack:** Rust (crates/core, src-tauri), TypeScript + three.js (web/src), cargo tests, the debug smoke harness.

**Spec:** `docs/superpowers/specs/2026-11-22-tropical-style-design.md` (Spanish). Phase 1 = spec goals 1–4 (rule part), 7 (rule part), 8, 9 (rule part), 10. Phase 1b (prep script, OSM power lines/lamps/cycleways) and Phase 2 (open data) get their own plans.

## Global Constraints

- Tropical = `|lat| < 23.5°` and altitude `< ~1200 m`, blended: weight `(1 − smoothstep(21, 25, |lat|)) · (1 − smoothstep(900, 1500, height))`.
- Yellow road lines only "in the Americas": longitude in `[-170, -30]`.
- Outside the tropics (and outside the Americas for paint) every existing output is byte-for-byte unchanged: tree kinds, barriers, road looks.
- Open road is a user setting (`WorldLook.open_road`, default `false`); crowd barriers stay the default.
- Do not touch the app's Overpass query (`world/surface.rs::way_query`) or `WAY_TAGS`.
- Rule props never stand on bridges or in tunnels (`field.kinds()[i] != OPEN`) and never on another road strip.
- Native renderer: may fall back (unknown looks → default texture/model); it must not crash.
- Commits: the user's rule is "commit only when asked". Each task ends with a commit step; run it only if the user has OK'd commits, otherwise leave the task's changes staged.
- Tests: `cargo test -p velopopuli-core` must stay green (132 core tests today) plus the new ones.

## Review Focus

1. **No map data at all** (offline, tiles failed) on a tropical route: no panic, every sample `Rural`, no props, palms still come from the climate mix. → test in Task 2.
2. **Very short or closed (loop) routes**: prop and median loops must not index past `route.n`, and a loop must not double-place at the seam. → test in Task 5.
3. **Tropical but high** (Quito, 2,800 m): no palms anywhere, including the median (median plants become `BUSH`). → tests in Tasks 1 and 5.
4. **Bridges and tunnels on an avenue**: no lamps, jersey barriers, sidewalks or median palms on those samples. → test in Task 5.
5. **Old settings files** without `openRoad` must load with `open_road = false`; European worlds keep crowd barriers and white paint. → tests in Tasks 3 and 4.

---

## File Map

| File | Change | Responsibility |
|---|---|---|
| `crates/core/src/world/tropics.rs` | create | `tropical_weight`, `in_americas` |
| `crates/core/src/world/trees.rs` | modify | `PALM`, `FAN_PALM`, `RAIN_TREE` models; tropical branch in `Climate::kind_at` |
| `crates/core/src/world/roadside.rs` | create | `Stretch`, `RoadsideStyle`, `RoadsidePlan`, `plan()`, rule props, median plants, rule walls |
| `crates/core/src/world/scenery.rs` | modify | keep `road_kinds`; one-way strip accessor; sidewalks; residential grid |
| `crates/core/src/world/buildings.rs` | modify | `count_near` |
| `crates/core/src/world/surface.rs` | modify | look constants `YELLOW`, `DOUBLE_YELLOW`, `ONEWAY_AMERICAS` |
| `crates/core/src/world/barriers.rs` | modify | looks `LAMP_DOUBLE`, `LAMP_SINGLE`, `JERSEY`; open road drops crowd barriers; append rule props |
| `crates/core/src/world/mvt.rs` | modify | parse `land` kind `residential` |
| `crates/core/src/world/mod.rs` | modify | modules, re-exports, `World.roadside`, `with_roadside`, looks, median trees in `chunk` |
| `crates/core/src/types.rs` | modify | `WorldLook.open_road` |
| `src-tauri/src/session.rs` | modify | call `with_roadside` |
| `src-tauri/src/mapdata.rs` | modify | bump `SCENERY_FORMAT` 4 → 5 (Task 6) |
| `web/src/world/surfaces.ts` | modify | textures for the three new looks |
| `web/src/types.ts`, `web/src/ui/Settings.tsx` | modify | `openRoad` setting + toggle |
| `crates/core/tests/tropics.rs` | create | all new tests |

### Shared test fixture (create in Task 2, extend later)

In `crates/core/tests/tropics.rs`:

```rust
/// A straight road heading north from (lat0, lon0), with flat elevation 5 m.
fn straight_at(lat0: f64, lon0: f64, length_m: f64) -> Arc<Route>;
/// `(x metres east, metres along)` → `[lat, lon]` for that route.
fn geo(route: &Route, pts: &[(f64, f64)]) -> Vec<[f64; 2]>;
/// 3 km north from (lat0, lon0). Scenery: the route's own carriageway (oneway Main, x = 0,
/// 0–3000 m); the opposite carriageway (oneway Main, x = +14, 0–2000 m); 10 × 10 m building
/// footprints at x = ±50 and ±80 every 30 m for 0–1000 m and 2500–2950 m.
/// Expected stretches: 0–1000 Avenue, 1000–2000 Periurban, 2000–2500 Rural, 2500–3000 Town.
fn corridor(lat0: f64, lon0: f64, style: RoadsideStyle) -> (Arc<Route>, World);
const SAMBORONDON: (f64, f64) = (-2.07, -79.87);
const ALPS: (f64, f64) = (45.0, 7.0);
```

`corridor` builds `World::with_map(route, None, 0.0, Some(&scenery), &footprints).with_roadside(style)`.

---

### Task 1: Tropical climate and palm / rain-tree models

**Files:**
- Create: `crates/core/src/world/tropics.rs`
- Modify: `crates/core/src/world/trees.rs` (consts at top, `tree_model`, `Climate::kind_at`), `crates/core/src/world/mod.rs` (mod + re-exports at lines 19–37)
- Test: `crates/core/tests/tropics.rs`

**Interfaces:**
- Produces: `world::tropical_weight(lat_abs: f64, height: f64) -> f64`, `world::in_americas(lon: f64) -> bool`, `world::{PALM, FAN_PALM, RAIN_TREE}: u8` (= 6, 7, 8), `TREE_KINDS == 9`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn tropical_weight_is_lowland_tropics_only() {
    assert_eq!(tropical_weight(2.0, 10.0), 1.0);
    assert_eq!(tropical_weight(39.8, 10.0), 0.0);   // Mallorca
    assert_eq!(tropical_weight(0.2, 2800.0), 0.0);  // Quito
    let edge = tropical_weight(23.0, 10.0);
    assert!(edge > 0.0 && edge < 1.0);
    assert!(in_americas(-79.87) && !in_americas(2.8) && !in_americas(-10.0));
}

#[test]
fn tropical_mix_has_palms_and_no_conifers() {
    let c = TreeClimate { lat: 2.0, tree_line: 3524.0 };
    let kinds: Vec<u8> = grid(40).map(|(r, p)| c.kind_at((0.0, 0.0), 10.0, false, r, p)).collect();
    let tropical = kinds.iter().filter(|&&k| k == PALM || k == FAN_PALM || k == RAIN_TREE).count();
    assert!(tropical * 2 > kinds.len());
    assert!(!kinds.iter().any(|&k| k == SPRUCE || k == PINE || k == BIRCH || k == COLUMN));
}

#[test]
fn no_tropical_kinds_outside_the_tropics_or_up_high() {
    for (lat, h) in [(39.8, 10.0), (45.0, 300.0), (0.2, 2800.0)] {
        let c = TreeClimate { lat, tree_line: 3600.0 - lat * 38.0 };
        for wood in [false, true] {
            assert!(grid(40).all(|(r, p)| ![PALM, FAN_PALM, RAIN_TREE].contains(&c.kind_at((5.0, 9.0), h, wood, r, p))));
        }
    }
}

#[test]
fn new_tree_models_have_their_shapes() {
    assert_eq!(TREE_KINDS, 9);
    let [w, _, top] = sprite_frame(PALM);       assert!(top > 8.0 && w < 4.0);
    let [_, _, top] = sprite_frame(FAN_PALM);   assert!(top > 2.5 && top < 6.0);
    let [w, _, top] = sprite_frame(RAIN_TREE);  assert!(w >= 5.0 && top <= 10.0);
}
```

(`grid(n)` yields the `n × n` points `((i + 0.5) / n, (j + 0.5) / n)`.)

- [ ] **Step 2: Run, expect FAIL** — `cargo test -p velopopuli-core --test tropics` → unresolved imports.

- [ ] **Step 3: Implement `tropical_weight` and `in_americas` in `tropics.rs`** with the Global Constraints values (`smoothstep` from `world::height`).

- [ ] **Step 4: Add the three models to `tree_model`** using the existing `frustum`/`clump` helpers and a new `frond(m, base, dir, length, droop, colour)` that adds a thin drooping leaf of 2–4 triangles. PALM: slightly leaning trunk of 4 stacked frustums to ~10 m, 8 fronds of ~4 m. FAN_PALM: 1.5 m trunk, 10 shorter fronds spread wide (top 3–5 m). RAIN_TREE: 3 m thick trunk, 3–4 flattened clumps (radii ~6 m across, ~1.8 m tall) at 5–7 m. Set `KINDS = 9`.

- [ ] **Step 5: Tropical branch in `Climate::kind_at`**: `t = tropical_weight(self.lat, height)`; if `roll < t` pick by `pick`: in a wood `BROADLEAF 0.45, RAIN_TREE 0.25, PALM 0.15, BUSH rest`; alone `RAIN_TREE 0.35, PALM 0.25, FAN_PALM 0.15, BUSH rest`. Otherwise run the existing logic with `roll = (roll − t) / (1 − t)` (when `t == 0` this is the old `roll`, so outside the tropics nothing changes).

- [ ] **Step 6: Run, expect PASS** — `cargo test -p velopopuli-core` (all, including the 132 existing).

- [ ] **Step 7: Commit** — `git add crates/core && git commit -m "feat(world): palms and rain trees in the lowland tropics"`

---

### Task 2: Stretch classification (`roadside::plan`)

**Files:**
- Create: `crates/core/src/world/roadside.rs`
- Modify: `crates/core/src/world/scenery.rs` (one-way strip accessor), `crates/core/src/world/buildings.rs` (`count_near`), `crates/core/src/world/mod.rs` (`World.roadside: Option<RoadsidePlan>`, `with_roadside`, `roadside()`)
- Test: `crates/core/tests/tropics.rs` (+ fixture above)

**Interfaces:**
- Consumes: `tropical_weight`, `in_americas` (Task 1).
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Stretch { Avenue, Periurban, Rural, Town }`
  - `#[derive(Clone, Copy, Debug, Default)] pub struct RoadsideStyle { pub open_road: bool }`
  - `pub struct RoadsidePlan { pub stretch: Vec<Stretch>, pub median: Vec<Option<f64>>, pub median_side: Vec<f64>, pub tropical: bool, pub americas: bool, pub style: RoadsideStyle }` — one entry per route sample; `median` = metres between the two carriageways' inner edges; `median_side` = `+1.0` if the opposite carriageway is on the rider's right (+x for a north-heading route), `-1.0` left, `0.0` none.
  - `pub fn plan(route: &Route, scene: &Scene, buildings: &Buildings, style: RoadsideStyle) -> RoadsidePlan`
  - `impl World { pub fn with_roadside(self, style: RoadsideStyle) -> Self; pub fn roadside(&self) -> Option<&RoadsidePlan> }`
  - `Scene`: `pub(super) fn oneway_strips(&self) -> impl Iterator<Item = ((f64, f64), (f64, f64), f64)>` (`a`, `b`, `half`; one-way, non-casing, non-cap strips only)
  - `Buildings`: `pub(super) fn count_near(&self, x: f64, z: f64, reach: f64) -> usize`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn corridor_stretches_are_classified() {
    let (route, world) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle::default());
    let p = world.roadside().unwrap();
    let at = |d: f64| ((d / route.step) as usize).min(route.n - 1);
    assert_eq!(p.stretch[at(500.0)], Stretch::Avenue);
    assert_eq!(p.stretch[at(1500.0)], Stretch::Periurban);
    assert_eq!(p.stretch[at(2250.0)], Stretch::Rural);
    assert_eq!(p.stretch[at(2750.0)], Stretch::Town);
    let m = p.median[at(500.0)].unwrap();
    // Centre lines 14 m apart, minus both carriageways' half widths (about 3–4 m each).
    assert!(m > 4.0 && m < 11.0, "median {m}");
    assert_eq!(p.median_side[at(500.0)], 1.0);
    assert!(p.median[at(2250.0)].is_none());
    assert!(p.tropical && p.americas);
}

#[test]
fn no_map_data_means_rural_everywhere() {
    let route = straight_at(SAMBORONDON.0, SAMBORONDON.1, 1500.0);
    let world = World::new(route.clone(), None, 0.0).with_roadside(RoadsideStyle::default());
    assert!(world.roadside().unwrap().stretch.iter().all(|&s| s == Stretch::Rural));
}
```

- [ ] **Step 2: Run, expect FAIL** (missing types).

- [ ] **Step 3: Implement `plan`.** Per sample `i`:
  - **divided**: some one-way strip lies nearly parallel (`|cos| > 0.94`) with its centre line at lateral distance `half_width(i) + 1.0 ..= half_width(i) + 40.0` from the route, found for ≥ 60 m of route. `median = lateral − half_width(i) − strip.half`, `median_side` = sign of the lateral offset.
  - **urban**: `buildings.count_near(x_i, z_i, 150.0) >= 6`.
  - **stretch**: divided ∧ urban → `Avenue`; divided ∧ ¬urban → `Periurban`; ¬divided ∧ urban → `Town`; else `Rural`.
  - Clean with `surface::tidy(&mut stretch, (200.0 / route.step) as usize)`.
  - `tropical = tropical_weight(route.lat0.abs(), median elevation of the route) > 0.5`; `americas = in_americas(route.lon0)`.
  - `World::new` / `with_map` leave `roadside = None`; only `with_roadside` sets it.

- [ ] **Step 4: Run, expect PASS** — `cargo test -p velopopuli-core`.

- [ ] **Step 5: Commit** — `git commit -m "feat(world): classify route stretches for roadside rules"`

---

### Task 3: Yellow road paint in the Americas

**Files:**
- Modify: `crates/core/src/world/surface.rs` (consts after `TRACK` at line 216), `crates/core/src/world/mod.rs` (`looks` in `World::road`, line ~718), `web/src/world/surfaces.ts` (`SURFACES` + `draw` cases), `src-tauri/src/native/mod.rs` (texture name match, line ~1450: map new looks to `"road"` explicitly)
- Test: `crates/core/tests/tropics.rs`

**Interfaces:**
- Consumes: `World::roadside()` (Task 2).
- Produces: `pub const YELLOW: u8 = 10; pub const DOUBLE_YELLOW: u8 = 11; pub const ONEWAY_AMERICAS: u8 = 12;` re-exported from `world`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn americas_get_yellow_paint() {
    let (route, world) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle::default());
    let looks = world.road().looks;
    let at = |d: f64| ((d / route.step) as usize).min(route.n - 1);
    assert_eq!(looks[at(500.0)], ONEWAY_AMERICAS);  // a carriageway of a divided road
    assert_eq!(looks[at(2250.0)], DOUBLE_YELLOW);   // rural two-lane
    assert_eq!(looks[at(2750.0)], YELLOW);          // town, two-way, marked
}

#[test]
fn europe_keeps_white_paint() {
    let (_, world) = corridor(ALPS.0, ALPS.1, RoadsideStyle::default());
    assert!(world.road().looks.iter().all(|&l| l < YELLOW));
}
```

- [ ] **Step 2: Run, expect FAIL.**

- [ ] **Step 3: In `World::road`**, after the existing match, when `roadside.americas` and the look is `Surface::Asphalt as u8` (marked): `median.is_some()` → `ONEWAY_AMERICAS`; `Stretch::Rural` → `DOUBLE_YELLOW`; otherwise → `YELLOW`. `UNMARKED` and every other look are untouched.

- [ ] **Step 4: Textures in `surfaces.ts`** (same 256 px layout as the default case): `'yellow'` — white edge lines, broken yellow centre (`#e3b324`, x 126, 4 px, half period); `'double-yellow'` — two solid yellow lines at x 121 and 131; `'oneway-americas'` — solid yellow line at the left edge (x 25), white right edge, broken white lane line at x 126.

- [ ] **Step 5: Run, expect PASS** — `cargo test -p velopopuli-core` and `npx tsc --noEmit`.

- [ ] **Step 6: Commit** — `git commit -m "feat(world): yellow road paint in the Americas"`

---

### Task 4: Open road setting (no crowd barriers, sidewalks in towns and avenues)

**Files:**
- Modify: `crates/core/src/types.rs` (`WorldLook` + its `Default`), `crates/core/src/world/barriers.rs` (`barriers()`), `crates/core/src/world/scenery.rs` (sidewalk strips), `crates/core/src/world/mod.rs` (`with_roadside` adds sidewalks when `open_road`), `src-tauri/src/session.rs` (chain `.with_roadside(RoadsideStyle { open_road: settings.look.open_road })` on the non-velodrome world), `web/src/types.ts` (`look.openRoad`, default `false`), `web/src/ui/Settings.tsx` ("Virtual world" section, line ~601: toggle labelled "Open road (no race barriers)")
- Test: `crates/core/tests/tropics.rs`

**Interfaces:**
- Consumes: `RoadsideStyle`, `RoadsidePlan.stretch/median_side` (Task 2).
- Produces: `WorldLook.open_road: bool`; `Scene::add_sidewalks(&mut self, route: &Route, plan: &RoadsidePlan, half_width: impl Fn(usize) -> f64)` (pub(super)).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn old_settings_load_with_barriers_on() {
    let look: WorldLook = serde_json::from_str(r#"{"timeOfDay":"day","weather":"clear","landmarks":true,"photos":false,"mapillaryToken":""}"#).unwrap();
    assert!(!look.open_road);
}

#[test]
fn open_road_drops_crowd_barriers_and_adds_sidewalks() {
    let (_, closed) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle { open_road: false });
    assert!(closed.barriers().iter().any(|b| (b[5] as usize) <= BARRIER_FENCE));
    let (route, open) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle { open_road: true });
    assert!(!open.barriers().iter().any(|b| (b[5] as usize) <= BARRIER_FENCE));
    // A sidewalk on the outer (left, −x) side of the avenue at 500 m, and none in the rural stretch.
    let half = open.half_width(((500.0 / route.step) as usize).min(route.n - 1));
    let has_road_vertex = |c: &GridMesh, x0: f64, x1: f64| c.roads.positions.chunks(3).any(|p| (p[0] as f64) > x0 && (p[0] as f64) < x1);
    assert!(has_road_vertex(&open.chunk(-1, chunk_of(&route, 500.0)), -(half + 2.8), -(half + 0.3)));
    assert!(!has_road_vertex(&open.chunk(-1, chunk_of(&route, 2250.0)), -(half + 2.8), -(half + 0.3)));
}
```

(`chunk_of(route, d)` = `(route.sample_at(d).z / CHUNK).floor() as i32`; the fixture route heads north, so the rider's right is `+x` and chunk `cx = -1` holds the left side.)

- [ ] **Step 2: Run, expect FAIL.**

- [ ] **Step 3: `WorldLook.open_road`** with `#[serde(default)]`, `false` in `Default`; mirror in `web/src/types.ts` and the Settings toggle.

- [ ] **Step 4: `barriers()`**: when `roadside.style.open_road`, skip the crowd-barrier sections and the bend fences (props from Task 5 are still appended).

- [ ] **Step 5: Sidewalks**: for `Avenue` (outer side only, i.e. `-median_side`) and `Town` (both sides) samples on `OPEN` ground, add footway-kind strips 2.5 m wide whose inner edge is `half_width(i) + 0.3`, through the existing `add_strip` machinery (so they are cut into the terrain like other roads, with a casing that reads as a kerb). Group consecutive samples into strips of ≤ 20 m.

- [ ] **Step 6: Run, expect PASS** — `cargo test -p velopopuli-core`, `npx tsc --noEmit`, `cargo build -p velopopuli`.

- [ ] **Step 7: Commit** — `git commit -m "feat(world): open-road setting with sidewalks"`

---

### Task 5: Median palms, street lamps and concrete median barriers

**Files:**
- Modify: `crates/core/src/world/barriers.rs` (new looks + models, append `roadside::props`), `crates/core/src/world/roadside.rs` (`props`, `median_plants`), `crates/core/src/world/mod.rs` (`chunk`: push median plants into `planted` before the final sort at line ~656)
- Test: `crates/core/tests/tropics.rs`

**Interfaces:**
- Consumes: `RoadsidePlan` (Task 2), `PALM`, `BUSH`, `tropical_weight` (Task 1).
- Produces:
  - `barriers.rs`: `pub const LAMP_DOUBLE: usize = FENCE + 1; pub const LAMP_SINGLE: usize = FENCE + 2; pub const JERSEY: usize = FENCE + 3; pub const LOOKS: usize = FENCE + 4;` re-exported as `BARRIER_LAMP_DOUBLE`, `BARRIER_LAMP_SINGLE`, `BARRIER_JERSEY`.
  - `roadside.rs`: `pub(super) fn props(world: &World, plan: &RoadsidePlan) -> Vec<[f32; 7]>` (same layout as `barriers()`); `pub(super) fn median_plants(world: &World, plan: &RoadsidePlan, cx: i32, cz: i32) -> Vec<(u8, [f32; TREE_FLOATS])>`.

Rules (tropical plans only; `OPEN` samples only):
- `Avenue`: `LAMP_DOUBLE` every 38 m at offset `half_width + median / 2` on `median_side` (needs `median ≥ 1.5`). Median plants every 12 m (first at 6 m) at the same offset: `PALM` when `tropical_weight(lat, ele) > 0.5`, else `BUSH`; skip a plant within 3 m along the road of a lamp; a `BUSH` halfway between palms when `median ≥ 3.0`. Outer-side palms every 24 m at `half_width + 5.0` on `-median_side`, only where the spot is clear of buildings and road strips.
- `Town`: `LAMP_SINGLE` every 38 m on the right (`+x` side of travel) at `half_width + 0.8` (`+ 2.8` with open road), arm pointing at the road.
- `Periurban`: `JERSEY` sections every 2.5 m at `half_width + median / 2` on `median_side` (when `median` is `None` or `< 1.0`, at `half_width + 0.5`). No lamps.
- `Rural`: nothing.
- Spacing is measured in route distance; on a closed loop stop one spacing before `route.total`.

Models (`barrier_model(look)`): `LAMP_DOUBLE` grey 10 m pole with two arms forming a Y and two flat heads; `LAMP_SINGLE` 9 m pole, one arm leaning over `-x`; `JERSEY` 2.5 m long trapezoid profile (0.6 m base, 0.15 m top, 0.8 m tall), concrete grey with yellow and black diagonal bands on the road face.

- [ ] **Step 1: Write the failing tests**

```rust
fn looks_between(b: &[[f32; 7]], look: usize, d0: f64, d1: f64) -> Vec<[f32; 7]> {
    b.iter().filter(|e| e[5] as usize == look && (e[4] as f64) >= d0 && (e[4] as f64) < d1).copied().collect()
}

#[test]
fn avenue_gets_double_lamps_and_median_palms() {
    let (route, world) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle::default());
    let lamps = looks_between(&world.barriers(), BARRIER_LAMP_DOUBLE, 100.0, 900.0);
    assert!(lamps.len() >= 19 && lamps.len() <= 23);
    let i = ((500.0 / route.step) as usize).min(route.n - 1);
    let p = world.roadside().unwrap();
    let off = world.half_width(i) + p.median[i].unwrap() / 2.0;
    assert!(lamps.iter().all(|l| ((l[0] as f64) - off).abs() < 0.6));
    let trees = world.chunk(0, chunk_of(&route, 500.0)).trees;
    assert!(trees.chunks(TREE_FLOATS).any(|t| t[6] as u8 == PALM && ((t[0] as f64) - off).abs() < 0.6));
}

#[test]
fn periurban_gets_jersey_and_rural_nothing() {
    let (_, world) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle::default());
    let b = world.barriers();
    assert!(looks_between(&b, BARRIER_JERSEY, 1100.0, 1900.0).len() > 250);
    assert!(looks_between(&b, BARRIER_LAMP_DOUBLE, 1100.0, 1900.0).is_empty());
    assert!(b.iter().filter(|e| (e[5] as usize) > BARRIER_FENCE).all(|e| !(2050.0..2450.0).contains(&(e[4] as f64))));
}

#[test]
fn europe_gets_no_rule_props_or_palms() {
    let (route, world) = corridor(ALPS.0, ALPS.1, RoadsideStyle::default());
    assert!(world.barriers().iter().all(|e| (e[5] as usize) <= BARRIER_FENCE));
    assert!(world.chunk(0, chunk_of(&route, 500.0)).trees.chunks(TREE_FLOATS).all(|t| t[6] as u8 != PALM));
}

#[test]
fn high_tropical_avenue_has_bushes_not_palms() {
    // Same corridor, but at 2,800 m (build `corridor` with a flat elevation parameter).
    let (route, world) = corridor_at_height(SAMBORONDON.0, SAMBORONDON.1, 2800.0, RoadsideStyle::default());
    assert!(world.chunk(0, chunk_of(&route, 500.0)).trees.chunks(TREE_FLOATS).all(|t| t[6] as u8 != PALM));
}

#[test]
fn no_props_on_a_tunnel_and_short_loops_are_safe() {
    // Corridor plus a mapped tunnel crossing on the route at 400–600 m (a `Crossing` with `tunnel: true`).
    let (_, world) = corridor_with_tunnel(SAMBORONDON.0, SAMBORONDON.1);
    assert!(looks_between(&world.barriers(), BARRIER_LAMP_DOUBLE, 400.0, 600.0).is_empty());
    // A 150 m closed loop in the tropics builds without panicking.
    let ring = tropical_ring(150.0);
    let w = World::new(ring.clone(), None, 0.0).with_roadside(RoadsideStyle { open_road: true });
    let _ = (w.barriers(), w.chunk(0, 0), w.road());
}
```

(`corridor_at_height`, `corridor_with_tunnel` and `tropical_ring` are small variants of the fixture; `corridor` becomes `corridor_at_height(.., 5.0, ..)`.)

- [ ] **Step 2: Run, expect FAIL.**

- [ ] **Step 3: Models** in `barrier_model` for the three looks; bump `LOOKS`. Check `web/src/world/barriers.ts` builds one instanced mesh per model it receives (no hard-coded count); fix it if it is hard-coded.

- [ ] **Step 4: `roadside::props`** with the rules above; `barriers()` appends them when `roadside` is `Some` and `tropical`, then sorts as today.

- [ ] **Step 5: `roadside::median_plants`** for the chunk `(cx, cz)`; `World::chunk` extends `planted` with them before `planted.sort_by_key`.

- [ ] **Step 6: Run, expect PASS** — `cargo test -p velopopuli-core`, `npx tsc --noEmit`.

- [ ] **Step 7: Commit** — `git commit -m "feat(world): median palms, street lamps and jersey barriers in the tropics"`

---

### Task 6: Residential walls by rule

**Files:**
- Modify: `crates/core/src/world/scenery.rs` (`AreaKind::Residential`; `Scene.residential` grid filled like `forest`), `crates/core/src/world/mvt.rs` (line ~389: `("land", "residential") => AreaKind::Residential`), `src-tauri/src/mapdata.rs` (`SCENERY_FORMAT` 4 → 5), `crates/core/src/world/roadside.rs` (`walls`), `crates/core/src/world/mod.rs` (`chunk`: draw rule walls with `fields::draw(.., Boundary::Wall, ..)`)
- Test: `crates/core/tests/tropics.rs`

**Interfaces:**
- Consumes: `RoadsidePlan` (Task 2), `scene.near_mapped_boundary` (existing).
- Produces: `pub(super) fn walls(world: &World, plan: &RoadsidePlan, cx: i32, cz: i32) -> Vec<Vec<(f64, f64)>>` — polylines in local metres.

Rule (tropical plans, `Avenue`/`Periurban`/`Town`, `OPEN` samples, never on `median_side`): every 2.5 m, step outward from `half_width + 3.0` to `+ 60.0` in 1 m steps; the first point inside `residential` and clear of road strips and buildings is a wall point. Consecutive points (≤ 4 m apart) form a run; drop runs shorter than 20 m and any piece within 25 m of a mapped boundary (`near_mapped_boundary`). Wall colour: cream (`[0.90, 0.86, 0.76]` sRGB) when tropical.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn residential_edge_along_avenue_gets_a_wall() {
    // Corridor plus a residential area on the outer (−x) side: x −20 … −200, 100 … 900 m along.
    let (route, world) = corridor_with_residential(SAMBORONDON.0, SAMBORONDON.1);
    let mesh = world.chunk(-1, chunk_of(&route, 500.0)).buildings;
    let wall = |m: &TriMesh| m.positions.chunks(3).any(|p| (p[0] as f64) < -19.0 && (p[0] as f64) > -24.0 && p[1] as f64 > 5.8);
    assert!(wall(&mesh));
    let (route_eu, world_eu) = corridor_with_residential(ALPS.0, ALPS.1);
    assert!(!wall(&world_eu.chunk(-1, chunk_of(&route_eu, 500.0)).buildings));
}
```

(The fixture ground is at 5 m, so any wall top is above 5.8 m; use the wall height `fields::draw` already gives `Boundary::Wall`.)

- [ ] **Step 2: Run, expect FAIL.**
- [ ] **Step 3: Parse residential land, store the grid, bump `SCENERY_FORMAT`.**
- [ ] **Step 4: `roadside::walls` and drawing in `chunk`.**
- [ ] **Step 5: Run, expect PASS** — `cargo test --workspace` (also runs the app's tests touched by `mapdata.rs`).
- [ ] **Step 6: Commit** — `git commit -m "feat(world): walls along residential edges in the tropics"`

---

### Task 7: End-to-end visual check on the real routes

**Files:** none changed (scratch data folder only).

- [ ] **Step 1: Build** — `npx tauri build --debug --no-bundle` → `Built application at: …/target/debug/velopopuli`.
- [ ] **Step 2: Test library** — create `/tmp/vp-trop/data/routes.json` and `route-samborondon.json` from `~/Downloads/plaza-lagos-samborondon.gpx` (format in `docs/developers/building-and-testing.md`, "A test route"), and `/tmp/vp-trop/settings.json` with `"look": {"openRoad": true}` merged into defaults.
- [ ] **Step 3: Captures** — for `VP_SMOKE_AT` in `3000 8000 18000 26500`: `VP_DATA_DIR=/tmp/vp-trop VP_SMOKE=/tmp/vp-trop/km$AT.png VP_SMOKE_ROUTE=samborondon VP_SMOKE_AT=$AT VP_SMOKE_WINDOW=1 VP_SMOKE_MS=45000 target/debug/velopopuli`. Expected: km 3 palms + lamps in the median, sidewalk, yellow left edge; km 8 jersey barrier; km 18 double yellow line, rain trees, no lamps; km 26.5 yellow centre line, single lamps.
- [ ] **Step 4: Europe unchanged** — same capture for the Sa Calobra test route (`VP_SMOKE_ROUTE=sacalobra VP_SMOKE_AT=4000`) with default settings; compare by eye with the earlier capture: crowd barriers, white paint, no palms.
- [ ] **Step 5: Full check** — `npm run check` (format, types, clippy, all tests) passes.
- [ ] **Step 6: Report** the captures to the user.
