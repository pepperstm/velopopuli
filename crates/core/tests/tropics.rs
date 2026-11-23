//! The lowland tropics: palms and rain trees, and (from the roadside rules) the look of a
//! tropical road. Everything here must leave routes outside the tropics exactly as they were.

use std::sync::Arc;

use velopopuli_core::gpx::{build_route, RawPoint, Route, M_PER_DEG};
use velopopuli_core::types::WorldLook;
use velopopuli_core::world::{
    in_americas, sprite_frame, tropical_weight, Area, AreaKind, Crossing, Footprint, GridMesh, OpenData, PowerLine, Road, RoadKind, RoadsideStyle, Scenery, Stretch, TreeClimate, TriMesh, World,
    BARRIER_FENCE, BARRIER_JERSEY, BARRIER_LAMP_DOUBLE, BARRIER_LAMP_SINGLE, BARRIER_POLE, BARRIER_TOWER, BARRIER_WATER_TOWER, BIRCH, CHUNK, COLUMN, DOUBLE_YELLOW, FAN_PALM, ONEWAY_AMERICAS, OPEN,
    PALM, PINE, RAIN_TREE, SPRUCE, TREE_FLOATS, TREE_KINDS, YELLOW,
};

const SAMBORONDON: (f64, f64) = (-2.07, -79.87);
const ALPS: (f64, f64) = (45.0, 7.0);

/// The `n × n` points `((i + 0.5) / n, (j + 0.5) / n)`: every pair of a tree's random numbers.
fn grid(n: usize) -> impl Iterator<Item = (f64, f64)> {
    (0..n).flat_map(move |i| (0..n).map(move |j| ((i as f64 + 0.5) / n as f64, (j as f64 + 0.5) / n as f64)))
}

/// A straight road heading north from `(lat0, lon0)`, level at `ele` metres.
fn straight_at_height(lat0: f64, lon0: f64, length_m: f64, ele: f64) -> Arc<Route> {
    let pts: Vec<RawPoint> = (0..=(length_m / 10.0) as usize).map(|i| RawPoint { lat: lat0 + i as f64 * 10.0 / M_PER_DEG, lon: lon0, ele: Some(ele) }).collect();
    Arc::new(build_route("straight", &pts).unwrap())
}

fn straight_at(lat0: f64, lon0: f64, length_m: f64) -> Arc<Route> {
    straight_at_height(lat0, lon0, length_m, 5.0)
}

/// `(x metres east of the road, metres along it)` → `[lat, lon]`.
fn geo(route: &Route, pts: &[(f64, f64)]) -> Vec<[f64; 2]> {
    pts.iter()
        .map(|&(x, along)| {
            let p = route.sample_at(along);
            let (lat, lon) = route.to_lat_lon(p.x + x, p.z);
            [lat, lon]
        })
        .collect()
}

/// A 10 × 10 m house centred `x` metres east of the road, `along` metres along it.
fn house(route: &Route, id: u64, x: f64, along: f64) -> Footprint {
    Footprint { id, height: 6.0, ring: geo(route, &[(x - 5.0, along - 5.0), (x + 5.0, along - 5.0), (x + 5.0, along + 5.0), (x - 5.0, along + 5.0), (x - 5.0, along - 5.0)]) }
}

/// The corridor's map: the route's own carriageway (0–3000 m), the opposite one 14 m east
/// (0–2000 m), and houses either side for 0–1000 m and 2500–2950 m.
fn corridor_map(route: &Route) -> (Scenery, Vec<Footprint>) {
    let scenery = Scenery {
        roads: vec![
            Road { kind: RoadKind::Main, oneway: true, line: geo(route, &[(0.0, 0.0), (0.0, 1500.0), (0.0, 3000.0)]) },
            Road { kind: RoadKind::Main, oneway: true, line: geo(route, &[(14.0, 2000.0), (14.0, 1000.0), (14.0, 0.0)]) },
        ],
        ..Default::default()
    };
    let mut houses = Vec::new();
    for along in (0..1000).step_by(30).chain((2500..2950).step_by(30)) {
        for x in [-80.0, -50.0, 50.0, 80.0] {
            houses.push(house(route, houses.len() as u64, x, along as f64));
        }
    }
    (scenery, houses)
}

/// 3 km north from `(lat0, lon0)` at `ele` metres: 0–1000 m an avenue (divided, built up),
/// 1000–2000 m periurban (divided, open), 2000–2500 m rural, 2500–3000 m a town.
fn corridor_at_height(lat0: f64, lon0: f64, ele: f64, style: RoadsideStyle) -> (Arc<Route>, World) {
    let route = straight_at_height(lat0, lon0, 3000.0, ele);
    let (scenery, houses) = corridor_map(&route);
    let world = World::with_map(route.clone(), None, 0.0, Some(&scenery), &houses).with_roadside(style);
    (route, world)
}

/// The corridor with the route in a mapped tunnel from 400 to 600 m.
fn corridor_with_tunnel(lat0: f64, lon0: f64) -> (Arc<Route>, World) {
    corridor_with_tunnel_styled(lat0, lon0, RoadsideStyle::default())
}

fn corridor_with_tunnel_styled(lat0: f64, lon0: f64, style: RoadsideStyle) -> (Arc<Route>, World) {
    let route = straight_at(lat0, lon0, 3000.0);
    let (mut scenery, houses) = corridor_map(&route);
    scenery.crossings.push(Crossing { tunnel: true, kind: RoadKind::Main, line: geo(&route, &[(0.0, 400.0), (0.0, 600.0)]) });
    let world = World::with_map(route.clone(), None, 0.0, Some(&scenery), &houses).with_roadside(style);
    (route, world)
}

/// The corridor with a residential area on the avenue's outer (west) side: from 20 to 200 m
/// west of the road, 100 to 900 m along.
fn corridor_with_residential(lat0: f64, lon0: f64) -> (Arc<Route>, World) {
    let route = straight_at(lat0, lon0, 3000.0);
    let (mut scenery, houses) = corridor_map(&route);
    scenery.areas.push(Area { kind: AreaKind::Residential, rings: vec![geo(&route, &[(-20.0, 100.0), (-200.0, 100.0), (-200.0, 900.0), (-20.0, 900.0)])] });
    let world = World::with_map(route.clone(), None, 0.0, Some(&scenery), &houses).with_roadside(RoadsideStyle::default());
    (route, world)
}

/// The corridor with two-way side streets crossing it at 513 m (where an avenue lamp would
/// stand) and 1513 m (across the periurban median barrier).
fn corridor_with_side_streets(lat0: f64, lon0: f64) -> (Arc<Route>, World) {
    let route = straight_at(lat0, lon0, 3000.0);
    let (mut scenery, houses) = corridor_map(&route);
    for along in [513.0, 1513.0] {
        scenery.roads.push(Road { kind: RoadKind::Minor, oneway: false, line: geo(&route, &[(-60.0, along), (60.0, along)]) });
    }
    let world = World::with_map(route.clone(), None, 0.0, Some(&scenery), &houses).with_roadside(RoadsideStyle::default());
    (route, world)
}

/// A closed loop `length_m` round, in the tropics.
fn tropical_ring(length_m: f64) -> Arc<Route> {
    let r = length_m / std::f64::consts::TAU;
    let steps = (length_m / 2.0) as usize;
    let pts: Vec<RawPoint> = (0..=steps)
        .map(|k| {
            let a = k as f64 / steps as f64 * std::f64::consts::TAU;
            RawPoint { lat: SAMBORONDON.0 + r * a.sin() / M_PER_DEG, lon: SAMBORONDON.1 + r * a.cos() / (M_PER_DEG * SAMBORONDON.0.to_radians().cos()), ele: Some(5.0) }
        })
        .collect();
    Arc::new(build_route("ring", &pts).unwrap())
}

fn corridor(lat0: f64, lon0: f64, style: RoadsideStyle) -> (Arc<Route>, World) {
    corridor_at_height(lat0, lon0, 5.0, style)
}

/// The chunk row (`cz`) holding the point `d` metres along the route.
fn chunk_of(route: &Route, d: f64) -> i32 {
    (route.sample_at(d).z / CHUNK).floor() as i32
}

/// The route sample nearest `d` metres along.
fn at(route: &Route, d: f64) -> usize {
    ((d / route.step).round() as usize).min(route.n - 1)
}

mod climate {
    use super::*;

    #[test]
    fn tropical_weight_is_lowland_tropics_only() {
        assert_eq!(tropical_weight(2.0, 10.0), 1.0);
        assert_eq!(tropical_weight(39.8, 10.0), 0.0); // Mallorca
        assert_eq!(tropical_weight(0.2, 2800.0), 0.0); // Quito
        let edge = tropical_weight(23.0, 10.0);
        assert!(edge > 0.0 && edge < 1.0, "edge of the tropics: {edge}");
        assert!(in_americas(-79.87) && !in_americas(2.8) && !in_americas(-10.0));
    }

    #[test]
    fn tropical_mix_has_palms_and_no_conifers() {
        let c = TreeClimate { lat: 2.0, tree_line: 3524.0 };
        let kinds: Vec<u8> = grid(40).map(|(r, p)| c.kind_at((0.0, 0.0), 10.0, false, r, p)).collect();
        let tropical = kinds.iter().filter(|&&k| k == PALM || k == FAN_PALM || k == RAIN_TREE).count();
        assert!(tropical * 2 > kinds.len(), "{tropical} of {} are tropical", kinds.len());
        assert!(!kinds.iter().any(|&k| k == SPRUCE || k == PINE || k == BIRCH || k == COLUMN));
    }

    #[test]
    fn no_tropical_kinds_outside_the_tropics_or_up_high() {
        for (lat, h) in [(39.8, 10.0), (45.0, 300.0), (0.2, 2800.0)] {
            let c = TreeClimate { lat, tree_line: 3600.0 - lat * 38.0 };
            for wood in [false, true] {
                assert!(grid(40).all(|(r, p)| ![PALM, FAN_PALM, RAIN_TREE].contains(&c.kind_at((5.0, 9.0), h, wood, r, p))), "lat {lat}, {h} m, wood {wood}");
            }
        }
    }

    #[test]
    fn new_tree_models_have_their_shapes() {
        assert_eq!(TREE_KINDS, 9);
        let [w, _, top] = sprite_frame(PALM);
        assert!(top > 8.0 && w < 4.0, "palm {w} wide, {top} tall");
        let [_, _, top] = sprite_frame(FAN_PALM);
        assert!(top > 2.5 && top < 6.0, "fan palm {top} tall");
        let [w, _, top] = sprite_frame(RAIN_TREE);
        assert!(w >= 5.0 && top <= 10.0, "rain tree {w} wide, {top} tall");
    }
}

mod stretches {
    use super::*;

    #[test]
    fn corridor_stretches_are_classified() {
        let (route, world) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle::default());
        let p = world.roadside().unwrap();
        assert_eq!(p.stretch[at(&route, 500.0)], Stretch::Avenue);
        assert_eq!(p.stretch[at(&route, 1500.0)], Stretch::Periurban);
        assert_eq!(p.stretch[at(&route, 2250.0)], Stretch::Rural);
        assert_eq!(p.stretch[at(&route, 2750.0)], Stretch::Town);
        let m = p.median[at(&route, 500.0)].unwrap();
        // Centre lines 14 m apart, minus both carriageways' half widths (about 3–4 m each).
        assert!(m > 4.0 && m < 11.0, "median {m}");
        assert_eq!(p.median_side[at(&route, 500.0)], 1.0);
        assert!(p.median[at(&route, 2250.0)].is_none());
        assert!(p.tropical && p.americas);
    }

    #[test]
    fn housing_land_counts_as_built_up() {
        // The open divided stretch (1000–2000 m) with housing mapped beside it but no houses:
        // the map's land use says it is built up, so it is an avenue, not periurban.
        let route = straight_at(SAMBORONDON.0, SAMBORONDON.1, 3000.0);
        let (mut scenery, houses) = corridor_map(&route);
        scenery.areas.push(Area { kind: AreaKind::Residential, rings: vec![geo(&route, &[(-30.0, 1100.0), (-300.0, 1100.0), (-300.0, 1900.0), (-30.0, 1900.0)])] });
        let world = World::with_map(route.clone(), None, 0.0, Some(&scenery), &houses).with_roadside(RoadsideStyle::default());
        assert_eq!(world.roadside().unwrap().stretch[at(&route, 1500.0)], Stretch::Avenue);
    }

    #[test]
    fn shops_count_as_built_up() {
        let route = straight_at(SAMBORONDON.0, SAMBORONDON.1, 3000.0);
        let (mut scenery, houses) = corridor_map(&route);
        scenery.areas.push(Area { kind: AreaKind::Commercial, rings: vec![geo(&route, &[(-30.0, 1100.0), (-300.0, 1100.0), (-300.0, 1900.0), (-30.0, 1900.0)])] });
        let world = World::with_map(route.clone(), None, 0.0, Some(&scenery), &houses).with_roadside(RoadsideStyle::default());
        assert_eq!(world.roadside().unwrap().stretch[at(&route, 1500.0)], Stretch::Avenue);
    }

    #[test]
    fn a_one_way_service_lane_is_not_the_other_carriageway() {
        // A one-way parking aisle 9 m west of the avenue, nearer than the real other carriageway
        // (14 m east): the median is still on the east side.
        let route = straight_at(SAMBORONDON.0, SAMBORONDON.1, 3000.0);
        let (mut scenery, houses) = corridor_map(&route);
        scenery.roads.push(Road { kind: RoadKind::Service, oneway: true, line: geo(&route, &[(-9.0, 100.0), (-9.0, 900.0)]) });
        let world = World::with_map(route.clone(), None, 0.0, Some(&scenery), &houses).with_roadside(RoadsideStyle::default());
        assert_eq!(world.roadside().unwrap().median_side[at(&route, 500.0)], 1.0);
    }

    #[test]
    fn no_map_data_means_rural_everywhere() {
        let route = straight_at(SAMBORONDON.0, SAMBORONDON.1, 1500.0);
        let world = World::new(route.clone(), None, 0.0).with_roadside(RoadsideStyle::default());
        assert!(world.roadside().unwrap().stretch.iter().all(|&s| s == Stretch::Rural));
    }

    #[test]
    fn worlds_without_roadside_rules_have_no_plan() {
        let (route, _) = corridor(ALPS.0, ALPS.1, RoadsideStyle::default());
        assert!(World::new(route, None, 0.0).roadside().is_none());
    }
}

mod paint {
    use super::*;

    #[test]
    fn americas_get_yellow_paint() {
        let (route, world) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle::default());
        let looks = world.road().looks;
        assert_eq!(looks[at(&route, 500.0)], ONEWAY_AMERICAS); // a carriageway of a divided road
        assert_eq!(looks[at(&route, 2250.0)], DOUBLE_YELLOW); // rural two-lane
        assert_eq!(looks[at(&route, 2750.0)], YELLOW); // town, two-way, marked
    }

    #[test]
    fn europe_keeps_white_paint() {
        let (_, world) = corridor(ALPS.0, ALPS.1, RoadsideStyle::default());
        assert!(world.road().looks.iter().all(|&l| l < YELLOW));
    }
}

mod open_road {
    use super::*;

    /// Whether the chunk's laid roads have a vertex between `x0` and `x1` (metres east) within
    /// 20 m (north–south) of `z`.
    fn road_vertex_between(c: &GridMesh, x0: f64, x1: f64, z: f64) -> bool {
        c.roads.positions.chunks(3).any(|p| (p[0] as f64) > x0 && (p[0] as f64) < x1 && ((p[2] as f64) - z).abs() < 20.0)
    }

    #[test]
    fn old_settings_load_with_barriers_on() {
        let look: WorldLook = serde_json::from_str(r#"{"timeOfDay":"day","weather":"clear","landmarks":true,"photos":false,"mapillaryToken":""}"#).unwrap();
        assert!(!look.open_road);
    }

    #[test]
    fn open_road_drops_crowd_barriers() {
        let (_, closed) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle { open_road: false });
        assert!(closed.barriers().iter().any(|b| (b[5] as usize) <= BARRIER_FENCE));
        let (_, open) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle { open_road: true });
        assert!(!open.barriers().iter().any(|b| (b[5] as usize) <= BARRIER_FENCE));
    }

    #[test]
    fn open_road_lays_sidewalks_in_built_up_stretches_only() {
        let (route, open) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle { open_road: true });
        let x = |d: f64| route.sample_at(d).x;
        let half = open.half_width(at(&route, 500.0));
        // The avenue's outer side is the left (west, −x): the other carriageway is on the right.
        let (x0, x1) = (x(500.0) - (half + 2.8), x(500.0) - (half + 0.3));
        let cx = (x(500.0) / CHUNK).floor() as i32 - 1;
        let avenue = (cx..=cx + 1).any(|c| road_vertex_between(&open.chunk(c, chunk_of(&route, 500.0)), x0, x1, route.sample_at(500.0).z));
        assert!(avenue, "a sidewalk beside the avenue");
        let (r0, r1) = (x(2250.0) - (half + 2.8), x(2250.0) - (half + 0.3));
        let rural = (cx..=cx + 1).any(|c| road_vertex_between(&open.chunk(c, chunk_of(&route, 2250.0)), r0, r1, route.sample_at(2250.0).z));
        assert!(!rural, "no sidewalk in the country");
        let (_, closed) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle { open_road: false });
        let none = (cx..=cx + 1).any(|c| road_vertex_between(&closed.chunk(c, chunk_of(&route, 500.0)), x0, x1, route.sample_at(500.0).z));
        assert!(!none, "no sidewalk with the race barriers up");
    }
}

mod roadside_props {
    use super::*;

    fn looks_between(b: &[[f32; 7]], look: usize, d0: f64, d1: f64) -> Vec<[f32; 7]> {
        b.iter().filter(|e| e[5] as usize == look && (e[4] as f64) >= d0 && (e[4] as f64) < d1).copied().collect()
    }

    /// The trees of the chunks either side of the route at `d` metres along.
    fn trees_near(route: &Route, world: &World, d: f64) -> Vec<f32> {
        let cx = (route.sample_at(d).x / CHUNK).floor() as i32;
        (cx - 1..=cx + 1).flat_map(|c| world.chunk(c, chunk_of(route, d)).trees).collect()
    }

    #[test]
    fn avenue_gets_double_lamps_and_median_palms() {
        let (route, world) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle::default());
        let lamps = looks_between(&world.barriers(), BARRIER_LAMP_DOUBLE, 100.0, 900.0);
        assert!(lamps.len() >= 19 && lamps.len() <= 23, "{} lamps", lamps.len());
        let i = at(&route, 500.0);
        let p = world.roadside().unwrap();
        let off = route.x[i] + world.half_width(i) + p.median[i].unwrap() / 2.0;
        assert!(lamps.iter().all(|l| ((l[0] as f64) - off).abs() < 0.6), "lamps at x {:?}, median at {off}", lamps.iter().map(|l| l[0]).collect::<Vec<_>>());
        let trees = trees_near(&route, &world, 500.0);
        assert!(trees.chunks(TREE_FLOATS).any(|t| t[6] as u8 == PALM && ((t[0] as f64) - off).abs() < 0.6), "a palm in the median");
    }

    #[test]
    fn periurban_gets_jersey_and_rural_nothing() {
        let (_, world) = corridor(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle::default());
        let b = world.barriers();
        assert!(looks_between(&b, BARRIER_JERSEY, 1100.0, 1900.0).len() > 250);
        assert!(looks_between(&b, BARRIER_LAMP_DOUBLE, 1100.0, 1900.0).is_empty());
        assert!(b.iter().filter(|e| (e[5] as usize) > BARRIER_FENCE).all(|e| !(2050.0..2350.0).contains(&(e[4] as f64))));
    }

    #[test]
    fn europe_gets_no_rule_props_or_palms() {
        let (route, world) = corridor(ALPS.0, ALPS.1, RoadsideStyle::default());
        assert!(world.barriers().iter().all(|e| (e[5] as usize) <= BARRIER_FENCE));
        assert!(trees_near(&route, &world, 500.0).chunks(TREE_FLOATS).all(|t| t[6] as u8 != PALM));
    }

    #[test]
    fn high_tropical_avenue_has_no_palms() {
        let (route, world) = corridor_at_height(SAMBORONDON.0, SAMBORONDON.1, 2800.0, RoadsideStyle::default());
        assert!(trees_near(&route, &world, 500.0).chunks(TREE_FLOATS).all(|t| t[6] as u8 != PALM));
    }

    #[test]
    fn no_lamps_in_a_tunnel() {
        let (_, world) = corridor_with_tunnel(SAMBORONDON.0, SAMBORONDON.1);
        let b = world.barriers();
        assert!(!looks_between(&b, BARRIER_LAMP_DOUBLE, 100.0, 380.0).is_empty(), "lamps before the tunnel");
        assert!(looks_between(&b, BARRIER_LAMP_DOUBLE, 400.0, 600.0).is_empty(), "none inside it");
    }

    #[test]
    fn nothing_stands_in_a_side_street() {
        let (route, world) = corridor_with_side_streets(SAMBORONDON.0, SAMBORONDON.1);
        let b = world.barriers();
        assert!(looks_between(&b, BARRIER_LAMP_DOUBLE, 508.0, 518.0).is_empty(), "no lamp in the side street");
        assert!(looks_between(&b, BARRIER_JERSEY, 1509.0, 1517.0).is_empty(), "the median barrier opens for the side street");
        assert!(!looks_between(&b, BARRIER_JERSEY, 1450.0, 1500.0).is_empty(), "but stands either side of it");
        let z = route.sample_at(513.0).z;
        let in_street = trees_near(&route, &world, 513.0).chunks(TREE_FLOATS).any(|t| ((t[2] as f64) - z).abs() < 4.0 && (t[0] as f64).abs() < 20.0);
        assert!(!in_street, "no palm or bush in the side street");
    }

    #[test]
    fn short_tropical_loops_are_safe() {
        let ring = tropical_ring(150.0);
        for open_road in [false, true] {
            let w = World::new(ring.clone(), None, 0.0).with_roadside(RoadsideStyle { open_road });
            let _ = (w.barriers(), w.chunk(0, 0), w.chunk(-1, -1), w.road());
        }
    }
}

mod walls {
    use super::*;

    /// Whether the mesh has a wall top (above 5.8 m, the ground being at 5 m) 13–26 m west of
    /// `x`: the estate's edge is 20 m out, give or take the 5 m lattice areas are kept on.
    fn wall(m: &TriMesh, x: f64) -> bool {
        m.positions.chunks(3).any(|p| (p[0] as f64) < x - 13.0 && (p[0] as f64) > x - 26.0 && p[1] as f64 > 5.8)
    }

    #[test]
    fn no_walls_where_housing_covers_the_street() {
        // A whole village mapped as housing, the town street and all (2400–3000 m): no estate
        // edge faces the road, so no walls line it.
        let route = straight_at(SAMBORONDON.0, SAMBORONDON.1, 3000.0);
        let (mut scenery, houses) = corridor_map(&route);
        scenery.areas.push(Area { kind: AreaKind::Residential, rings: vec![geo(&route, &[(-200.0, 2400.0), (200.0, 2400.0), (200.0, 3000.0), (-200.0, 3000.0)])] });
        let world = World::with_map(route.clone(), None, 0.0, Some(&scenery), &houses).with_roadside(RoadsideStyle::default());
        let x = route.sample_at(2750.0).x;
        let cx = (x / CHUNK).floor() as i32;
        let near_road = |m: &TriMesh| {
            m.positions.chunks(3).any(|p| ((p[0] as f64) - x).abs() < 40.0 && ((p[0] as f64) - x).abs() > 4.0 && p[1] as f64 > 5.8 && ((p[2] as f64) - route.sample_at(2750.0).z).abs() < 30.0)
        };
        // Houses stand 50 m out, so anything tall within 40 m of the road here is a wall.
        assert!(!(cx - 1..=cx + 1).any(|c| near_road(&world.chunk(c, chunk_of(&route, 2750.0)).buildings)));
    }

    #[test]
    fn residential_edge_along_avenue_gets_a_wall() {
        let (route, world) = corridor_with_residential(SAMBORONDON.0, SAMBORONDON.1);
        let x = route.sample_at(500.0).x;
        let cx = (x / CHUNK).floor() as i32 - 1;
        assert!((cx..=cx + 1).any(|c| wall(&world.chunk(c, chunk_of(&route, 500.0)).buildings, x)), "a wall along the estate");
        let (route_eu, world_eu) = corridor_with_residential(ALPS.0, ALPS.1);
        let x_eu = route_eu.sample_at(500.0).x;
        let cx_eu = (x_eu / CHUNK).floor() as i32 - 1;
        assert!(!(cx_eu..=cx_eu + 1).any(|c| wall(&world_eu.chunk(c, chunk_of(&route_eu, 500.0)).buildings, x_eu)), "none outside the tropics");
    }
}

mod sidewalk_edges {
    use super::*;

    #[test]
    fn sidewalks_stop_before_a_tunnel() {
        let (route, world) = corridor_with_tunnel_styled(SAMBORONDON.0, SAMBORONDON.1, RoadsideStyle { open_road: true });
        let kinds = world.field.kinds();
        let k = (0..route.n).find(|&i| kinds[i] != OPEN).expect("a tunnel");
        let k_end = (k..route.n).find(|&i| kinds[i] == OPEN).unwrap_or(route.n - 1);
        let (z_in, z_out) = (route.z[k], route.z[k_end - 1]);
        let x = route.x[k];
        let half = world.half_width(k);
        let cx = (x / CHUNK).floor() as i32;
        let inside = (cx - 1..=cx + 1).flat_map(|c| [chunk_of(&route, k as f64 * route.step), chunk_of(&route, (k_end - 1) as f64 * route.step)].map(|cz| world.chunk(c, cz))).any(|m| {
            m.roads.positions.chunks(3).any(|p| {
                let (px, pz) = (p[0] as f64 - x, p[2] as f64);
                (px.abs() - half) > 0.2 && (px.abs() - half) < 3.0 && pz <= z_in + 0.5 && pz >= z_out - 0.5
            })
        });
        assert!(!inside, "no sidewalk from the tunnel mouth on");
    }
}

mod median_on_the_left {
    use super::*;

    /// The corridor mirrored: the other carriageway 14 m WEST, as in right-hand traffic.
    fn mirrored(style: RoadsideStyle) -> (Arc<Route>, World) {
        let route = straight_at(SAMBORONDON.0, SAMBORONDON.1, 3000.0);
        let (mut scenery, houses) = corridor_map(&route);
        scenery.roads[1] = Road { kind: RoadKind::Main, oneway: true, line: geo(&route, &[(-14.0, 2000.0), (-14.0, 1000.0), (-14.0, 0.0)]) };
        let world = World::with_map(route.clone(), None, 0.0, Some(&scenery), &houses).with_roadside(style);
        (route, world)
    }

    #[test]
    fn lamps_go_in_the_median_on_the_left_and_the_sidewalk_on_the_right() {
        let (route, world) = mirrored(RoadsideStyle { open_road: true });
        let p = world.roadside().unwrap();
        let i = at(&route, 500.0);
        assert_eq!(p.median_side[i], -1.0);
        let off = world.half_width(i) + p.median[i].unwrap() / 2.0;
        let lamps: Vec<[f32; 7]> = world.barriers().into_iter().filter(|e| e[5] as usize == BARRIER_LAMP_DOUBLE && (100.0..900.0).contains(&(e[4] as f64))).collect();
        assert!(!lamps.is_empty() && lamps.iter().all(|l| ((l[0] as f64) - (route.x[i] - off)).abs() < 0.6));
        let x = route.x[i];
        let cx = (x / CHUNK).floor() as i32;
        let right = (cx..=cx + 1).any(|c| {
            world.chunk(c, chunk_of(&route, 500.0)).roads.positions.chunks(3).any(|q| {
                let dx = q[0] as f64 - x;
                dx > world.half_width(i) + 0.3 && dx < world.half_width(i) + 2.8 && ((q[2] as f64) - route.z[i]).abs() < 20.0
            })
        });
        assert!(right, "the sidewalk on the right");
    }
}

mod out_and_back {
    use super::*;

    #[test]
    fn coming_back_on_the_other_carriageway_is_a_divided_road() {
        // North 1500 m, round a 14 m U-turn, and back south on the other carriageway, with houses
        // either side: both legs are an avenue with the other leg as its other carriageway.
        let (lat0, lon0) = SAMBORONDON;
        let east = 14.0 / (M_PER_DEG * lat0.to_radians().cos());
        let mut pts: Vec<RawPoint> = (0..=150).map(|i| RawPoint { lat: lat0 + i as f64 * 10.0 / M_PER_DEG, lon: lon0, ele: Some(5.0) }).collect();
        pts.extend((0..=150).rev().map(|i| RawPoint { lat: lat0 + i as f64 * 10.0 / M_PER_DEG, lon: lon0 + east, ele: Some(5.0) }));
        let route = Arc::new(build_route("out and back", &pts).unwrap());
        let mut houses = Vec::new();
        for along in (0..1500).step_by(30) {
            for x in [-60.0, -40.0, 50.0, 70.0] {
                let p = route.sample_at(along as f64);
                let (lat, lon) = route.to_lat_lon(p.x + x, p.z);
                let d = 5.0 / M_PER_DEG;
                houses.push(Footprint { id: houses.len() as u64, height: 6.0, ring: vec![[lat - d, lon - d], [lat - d, lon + d], [lat + d, lon + d], [lat + d, lon - d], [lat - d, lon - d]] });
            }
        }
        let world = World::with_map(route.clone(), None, 0.0, Some(&Scenery::default()), &houses).with_roadside(RoadsideStyle::default());
        let p = world.roadside().unwrap();
        for d in [500.0, route.total - 500.0] {
            let i = at(&route, d);
            assert!(p.median[i].is_some(), "the other leg is the other carriageway at {d} m");
            assert_eq!(p.stretch[i], Stretch::Avenue, "an avenue at {d} m");
        }
    }
}

mod diagonal_walls {
    use super::*;

    #[test]
    fn an_estate_edge_at_an_angle_still_gets_its_wall() {
        // The estate's edge runs away from the road: 15 m out at 100 m, 60 m out at 250 m.
        let route = straight_at(SAMBORONDON.0, SAMBORONDON.1, 3000.0);
        let (mut scenery, houses) = corridor_map(&route);
        scenery.areas.push(Area { kind: AreaKind::Residential, rings: vec![geo(&route, &[(-15.0, 100.0), (-60.0, 250.0), (-300.0, 250.0), (-300.0, 100.0)])] });
        let world = World::with_map(route.clone(), None, 0.0, Some(&scenery), &houses).with_roadside(RoadsideStyle::default());
        let x = route.sample_at(175.0).x;
        let z = route.sample_at(175.0).z;
        let cx = (x / CHUNK).floor() as i32;
        let wall = (cx - 1..=cx).any(|c| {
            world.chunk(c, chunk_of(&route, 175.0)).buildings.positions.chunks(3).any(|q| {
                let dx = q[0] as f64 - x;
                dx < -10.0 && dx > -45.0 && q[1] as f64 > 5.8 && ((q[2] as f64) - z).abs() < 15.0
            })
        });
        assert!(wall, "a wall along the slanting edge");
    }
}

mod open_data {
    use super::*;

    /// A pack for `route` with one lamp, one house and one pond, as the script would write it.
    fn pack_json(route: &Route, format: u32) -> String {
        let [la, lo] = geo(route, &[(-12.0, 200.0)])[0];
        let house = geo(route, &[(-40.0, 300.0), (-30.0, 300.0), (-30.0, 310.0), (-40.0, 310.0), (-40.0, 300.0)]);
        let pond = geo(route, &[(60.0, 400.0), (120.0, 400.0), (120.0, 460.0), (60.0, 460.0)]);
        let ring = |r: &[[f64; 2]]| r.iter().map(|p| format!("[{},{}]", p[0], p[1])).collect::<Vec<_>>().join(",");
        format!(r#"{{"format":{format},"sources":["test"],"lamps":[[{la},{lo}]],"buildings":[{{"ring":[{}],"height":7.0}}],"water":[[[{}]]]}}"#, ring(&house), ring(&pond))
    }

    #[test]
    fn pack_parses_and_converts() {
        let route = straight_at(SAMBORONDON.0, SAMBORONDON.1, 3000.0);
        let od: OpenData = serde_json::from_str(&pack_json(&route, 1)).unwrap();
        assert!(od.usable());
        assert_eq!(od.footprints().len(), 1);
        let areas = od.areas();
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].kind, AreaKind::Water);
        let (scenery, houses) = corridor_map(&route);
        let world = World::with_map(route.clone(), None, 0.0, Some(&scenery), &houses).with_open_data(&od);
        assert_eq!(world.open().unwrap().lamps.len(), 1);
    }

    #[test]
    fn older_or_empty_packs_are_ignored() {
        let route = straight_at(SAMBORONDON.0, SAMBORONDON.1, 3000.0);
        let old: OpenData = serde_json::from_str(&pack_json(&route, 0)).unwrap();
        assert!(!old.usable());
        assert!(World::new(route.clone(), None, 0.0).with_open_data(&old).open().is_none());
        assert!(World::new(route, None, 0.0).with_open_data(&OpenData::default()).open().is_none());
    }

    #[test]
    fn packs_are_ignored_outside_the_tropics() {
        let route = straight_at(ALPS.0, ALPS.1, 3000.0);
        let od: OpenData = serde_json::from_str(&pack_json(&route, 1)).unwrap();
        assert!(World::new(route, None, 0.0).with_open_data(&od).open().is_none());
    }
}

mod open_lamps {
    use super::*;

    /// The corridor with an open-data pack.
    pub(super) fn corridor_with_pack(lat0: f64, lon0: f64, pack: impl Fn(&Route) -> OpenData) -> (Arc<Route>, World) {
        let route = straight_at(lat0, lon0, 3000.0);
        let (scenery, houses) = corridor_map(&route);
        let od = pack(&route);
        let world = World::with_map(route.clone(), None, 0.0, Some(&scenery), &houses).with_open_data(&od).with_roadside(RoadsideStyle::default());
        (route, world)
    }

    fn three_lamps(route: &Route) -> OpenData {
        OpenData { format: 1, lamps: geo(route, &[(-12.0, 200.0), (-12.0, 260.0), (-12.0, 320.0)]), ..Default::default() }
    }

    #[test]
    fn real_lamps_stand_where_mapped() {
        let (route, world) = corridor_with_pack(SAMBORONDON.0, SAMBORONDON.1, three_lamps);
        let singles: Vec<[f32; 7]> = world.barriers().into_iter().filter(|e| e[5] as usize == BARRIER_LAMP_SINGLE).collect();
        for d in [200.0, 260.0, 320.0] {
            let p = route.sample_at(d);
            assert!(singles.iter().any(|l| ((l[0] as f64) - (p.x - 12.0)).abs() < 0.5 && ((l[2] as f64) - p.z).abs() < 0.5), "a lamp at {d} m");
        }
    }

    #[test]
    fn rule_lamps_leave_room_for_real_ones() {
        let (_, world) = corridor_with_pack(SAMBORONDON.0, SAMBORONDON.1, three_lamps);
        assert!(world.barriers().iter().all(|e| !(e[5] as usize == BARRIER_LAMP_DOUBLE && (241.0..279.0).contains(&(e[4] as f64)))));
    }
}

mod power_lines {
    use super::open_lamps::corridor_with_pack;
    use super::*;

    fn a_line(route: &Route) -> OpenData {
        let pts = [(40.0, 100.0), (40.0, 400.0), (40.0, 700.0), (0.0, 1500.0)];
        OpenData {
            format: 1,
            power_lines: vec![PowerLine { voltage_kind: "line".into(), points: geo(route, &pts), supports: vec!["tower".into(); 4] }],
            poles: geo(route, &[(9.0, 2200.0)]),
            ..Default::default()
        }
    }

    #[test]
    fn towers_stand_on_their_mapped_spots() {
        let (route, world) = corridor_with_pack(SAMBORONDON.0, SAMBORONDON.1, a_line);
        let towers: Vec<[f32; 7]> = world.barriers().into_iter().filter(|e| e[5] as usize == BARRIER_TOWER).collect();
        for d in [100.0, 400.0, 700.0] {
            let p = route.sample_at(d);
            assert!(towers.iter().any(|t| ((t[0] as f64) - (p.x + 40.0)).abs() < 0.5 && ((t[2] as f64) - p.z).abs() < 0.5), "a tower at {d} m");
        }
        assert!(world.barriers().iter().any(|e| e[5] as usize == BARRIER_POLE), "the pole seen by Mapillary");
    }

    #[test]
    fn cables_hang_between_towers() {
        let (route, world) = corridor_with_pack(SAMBORONDON.0, SAMBORONDON.1, a_line);
        let (x, z0, z1) = (route.sample_at(250.0).x + 40.0, route.sample_at(400.0).z, route.sample_at(100.0).z);
        let cx = (x / CHUNK).floor() as i32;
        let high = (cx - 1..=cx + 1)
            .any(|c| world.chunk(c, chunk_of(&route, 250.0)).buildings.positions.chunks(3).any(|q| ((q[0] as f64) - x).abs() < 8.0 && q[1] as f64 > 20.0 && (q[2] as f64) > z0 && (q[2] as f64) < z1));
        assert!(high, "wires overhead between the towers");
    }

    #[test]
    fn no_tower_in_the_road() {
        let (route, world) = corridor_with_pack(SAMBORONDON.0, SAMBORONDON.1, a_line);
        let i = at(&route, 1500.0);
        let tower = world.barriers().into_iter().filter(|e| e[5] as usize == BARRIER_TOWER).min_by(|a, b| ((a[2] as f64) - route.z[i]).abs().total_cmp(&((b[2] as f64) - route.z[i]).abs())).unwrap();
        assert!(((tower[0] as f64) - route.x[i]).abs() >= world.half_width(i) + 1.0, "moved off the road: {}", tower[0]);
    }
}

mod cycle_lanes {
    use super::*;

    /// The corridor with a cycle lane mapped along the avenue's outer (west) side, 100–900 m.
    fn with_lane(open_road: bool) -> (Arc<Route>, World) {
        let route = straight_at(SAMBORONDON.0, SAMBORONDON.1, 3000.0);
        let (scenery, houses) = corridor_map(&route);
        let od = OpenData { format: 1, cycleways: vec![geo(&route, &[(-6.0, 100.0), (-6.0, 500.0), (-6.0, 900.0)])], ..Default::default() };
        let world = World::with_map(route.clone(), None, 0.0, Some(&scenery), &houses).with_open_data(&od).with_roadside(RoadsideStyle { open_road });
        (route, world)
    }

    /// The laid-road vertices beside the route at 500 m, as `(metres out from the road's edge, west side?, green?)`.
    fn beside_500(route: &Route, world: &World) -> Vec<(f64, bool, bool)> {
        let i = at(route, 500.0);
        let (x, z, half) = (route.x[i], route.z[i], world.half_width(i));
        let cx = (x / CHUNK).floor() as i32;
        (cx - 1..=cx + 1)
            .flat_map(|c| {
                let m = world.chunk(c, chunk_of(route, 500.0)).roads;
                m.positions
                    .chunks(3)
                    .zip(m.colors.chunks(3))
                    .filter(|(p, _)| ((p[2] as f64) - z).abs() < 15.0)
                    .map(|(p, c)| ((p[0] as f64 - x).abs() - half, (p[0] as f64) < x, c[1] > 0.15 && c[1] > 2.0 * c[0] && c[1] > 2.0 * c[2]))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn cycle_lane_runs_beside_the_road() {
        let (route, world) = with_lane(false);
        let v = beside_500(&route, &world);
        assert!(v.iter().any(|&(out, west, green)| green && west && out > 0.0 && out < 1.9), "a green lane on the west side");
        assert!(!v.iter().any(|&(_, west, green)| green && !west), "none on the east side");
    }

    #[test]
    fn sidewalk_moves_out_past_the_cycle_lane() {
        let (route, world) = with_lane(true);
        let v = beside_500(&route, &world);
        assert!(v.iter().any(|&(out, west, green)| !green && west && out > 3.2 && out < 4.7), "the sidewalk beyond the lane");
    }
}
