use velopopuli_core::gpx::{build_route, RawPoint};
use velopopuli_core::physics::PhysicsParams;
use velopopuli_core::planner::*;

/// A BRouter-style reply: points going east along a parallel, and a messages table whose
/// rows end at the given point indices with the given tags.
fn reply(n: usize, rows: &[(usize, &str)]) -> String {
    let point = |i: usize| -> (f64, f64) { (-3.0 + i as f64 * 0.001, 50.5) };
    let coords: Vec<String> = (0..n).map(|i| format!("[{:.6},{:.6},{}]", point(i).0, point(i).1, 10 + i)).collect();
    let mut messages = vec![r#"["Longitude","Latitude","Elevation","Distance","CostPerKm","ElevCost","TurnCost","NodeCost","InitialCost","WayTags","NodeTags","Time","Energy"]"#.to_string()];
    for (i, tags) in rows {
        let (lon, lat) = point(*i);
        messages.push(format!(r#"["{}","{}","10","100","1000","0","0","0","0","{tags}","","1","1"]"#, (lon * 1e6).round() as i64, (lat * 1e6).round() as i64));
    }
    format!(
        r#"{{"type":"FeatureCollection","features":[{{"type":"Feature","properties":{{"messages":[{}]}},"geometry":{{"type":"LineString","coordinates":[{}]}}}}]}}"#,
        messages.join(","),
        coords.join(",")
    )
}

#[test]
fn reads_a_routed_section_and_its_ways() {
    let leg = parse_brouter(&reply(6, &[(2, "highway=residential"), (4, "highway=track tracktype=grade2")])).unwrap();
    assert_eq!(leg.points.len(), 6);
    assert_eq!(leg.points[0].0, [50.5, -3.0]);
    assert_eq!(leg.points[3].1, Some(13.0));
    // The table stops at point 4; the last stretch is of unknown way.
    let ends: Vec<(usize, &str)> = leg.stretches.iter().map(|s| (s.to, s.tags.as_str())).collect();
    assert_eq!(ends, vec![(2, "highway=residential"), (4, "highway=track tracktype=grade2"), (5, "")]);
    assert!(parse_brouter("{}").is_err());
    assert!(parse_brouter("not json").is_err());
}

#[test]
fn sections_join_end_to_end_and_slice_back_apart() {
    let a = parse_brouter(&reply(4, &[(3, "highway=primary")])).unwrap();
    let mut b = a.clone();
    b.stretches[0].tags = "highway=cycleway".into();
    let joined = join(&[a.clone(), b.clone()]);
    // The point where they meet is kept once.
    assert_eq!(joined.points.len(), 7);
    assert_eq!(joined.stretches.iter().map(|s| s.to).collect::<Vec<_>>(), vec![3, 6]);
    assert_eq!(joined.slice(0, 3), a);
    assert_eq!(joined.slice(3, 6).stretches, b.stretches);
    assert_eq!(join(&[joined.slice(0, 2), joined.slice(2, 5), joined.slice(5, 6)]), joined);
}

#[test]
fn stretches_are_classified_by_surface_and_way() {
    use SurfaceClass::*;
    use WayClass::*;
    assert_eq!(classify("highway=residential maxspeed=50"), (Paved, MinorRoad));
    assert_eq!(classify("highway=primary surface=sett"), (Cobbles, MainRoad));
    assert_eq!(classify("highway=secondary_link"), (Paved, MainRoad));
    assert_eq!(classify("highway=track tracktype=grade2"), (Gravel, Track));
    assert_eq!(classify("highway=track"), (Unknown, Track));
    assert_eq!(classify("highway=track surface=asphalt"), (Paved, Track));
    assert_eq!(classify("highway=cycleway surface=asphalt"), (Paved, CyclePath));
    assert_eq!(classify("highway=path bicycle=designated"), (Unknown, CyclePath));
    assert_eq!(classify("highway=path surface=ground"), (Dirt, Path));
    assert_eq!(classify(""), (Unknown, Path));
}

#[test]
fn breakdown_adds_up_to_the_route() {
    let leg = parse_brouter(&reply(11, &[(3, "highway=residential"), (5, "highway=residential surface=asphalt"), (8, "highway=track tracktype=grade3"), (10, "highway=primary")])).unwrap();
    let b = breakdown(&leg, 1000);
    // The two residential stretches are one run.
    assert_eq!(b.runs.len(), 3);
    assert_eq!(b.runs[0].line.len(), 6);
    let total = leg.length();
    let sum = |v: &[f64]| v.iter().sum::<f64>();
    assert!((sum(&b.surfaces.iter().map(|s| s.metres).collect::<Vec<_>>()) - total).abs() < 0.01);
    assert!((sum(&b.ways.iter().map(|s| s.metres).collect::<Vec<_>>()) - total).abs() < 0.01);
    // Largest first: 7 of the 10 steps are paved.
    assert_eq!(b.surfaces[0].class, SurfaceClass::Paved);
    assert!((b.surfaces[0].metres / total - 0.7).abs() < 0.01);
    assert_eq!(b.ways.iter().map(|w| w.class).collect::<Vec<_>>(), vec![WayClass::MinorRoad, WayClass::Track, WayClass::MainRoad]);
    // Thinned to a point budget, runs still join up.
    let thin = breakdown(&leg, 3);
    assert_eq!(thin.runs[0].line.first(), Some(&leg.points[0].0));
    assert_eq!(thin.runs.last().unwrap().line.last(), Some(&leg.points[10].0));
}

#[test]
fn the_outline_keeps_the_points_where_sections_meet() {
    let leg = parse_brouter(&reply(101, &[(100, "highway=residential")])).unwrap();
    let (line, ends) = outline(&leg, &[0, 37, 100], 10);
    assert!(line.len() <= 15);
    assert_eq!(ends.len(), 3);
    assert_eq!(line[ends[0]], leg.points[0].0);
    assert_eq!(line[ends[1]], leg.points[37].0);
    assert_eq!(line[ends[2]], leg.points[100].0);
}

#[test]
fn loops_become_editable_waypoints() {
    // A square loop about 4.4 km round.
    let corners = [[50.5, -3.0], [50.5, -2.9845], [50.51, -2.9845], [50.51, -3.0], [50.5, -3.0]];
    let mut points = Vec::new();
    for w in corners.windows(2) {
        for k in 0..20 {
            let t = k as f64 / 20.0;
            points.push(([w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t], None));
        }
    }
    points.push((corners[4], None));
    let leg = Leg { stretches: vec![Stretch { to: points.len() - 1, tags: String::new() }], points };
    let wps = loop_waypoints(&leg, 1000.0);
    assert!((4..=13).contains(&wps.len()));
    assert_eq!(wps[0], (0, corners[0]));
    assert_eq!(*wps.last().unwrap(), (leg.points.len() - 1, corners[4]));
    assert!(wps.windows(2).all(|w| w[0].0 < w[1].0));
    // Cut at the waypoints and joined again, the loop is unchanged.
    let parts: Vec<Leg> = wps.windows(2).map(|w| leg.slice(w[0].0, w[1].0)).collect();
    assert_eq!(join(&parts), leg);
    assert!((round_trip_radius(64_000.0) - 10_000.0).abs() < 1.0);
}

#[test]
fn ride_time_follows_the_physics_and_the_surface() {
    let raw: Vec<RawPoint> = (0..=100).map(|i| RawPoint { lat: 50.5, lon: -3.0 + i as f64 * 0.0014, ele: Some(20.0) }).collect();
    let route = build_route("Flat", &raw).unwrap();
    let params = PhysicsParams { mass: 84.0, crr: 0.004, cda: 0.32, max_speed: 80.0 / 3.6 };
    let flat = ride_time(&route, 200.0, &params, |_| 1.0);
    // About 34 km/h at 200 W on the flat, plus a few seconds to get going.
    let expected = route.total / (33.9 / 3.6);
    assert!((flat - expected).abs() < 15.0, "{flat} vs {expected}");
    let gravel = ride_time(&route, 200.0, &params, |_| 2.5);
    assert!(gravel > flat * 1.07);
}

#[test]
fn unknown_styles_fall_back_to_the_default() {
    assert_eq!(style("gravel"), "gravel");
    assert_eq!(style("fastbike-verylowtraffic"), "fastbike-verylowtraffic");
    assert_eq!(style("car-fast"), "fastbike");
}
