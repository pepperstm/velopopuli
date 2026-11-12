//! Built-in velodromes: the route round the measuring line, and the track drawn round it.

use velopopuli_core::gpx::haversine;
use velopopuli_core::velodrome::{track_for, HERNE_HILL};

#[test]
fn herne_hill_is_a_flat_450_m_loop_from_the_line() {
    for clockwise in [false, true] {
        let route = HERNE_HILL.route(clockwise);
        assert!((route.total - 450.0).abs() < 0.05, "a lap is {} m", route.total);
        assert!(route.closed, "the route ends where it starts");
        assert!(route.ele.iter().all(|&e| e == 0.0), "the track is flat");
        assert!(route.climbs.is_empty());
        // The line is on the home straight, just north of the pavilion: within a few metres of
        // where it is on the ground (51.45125, -0.09082).
        assert!(haversine(route.lat[0], route.lon[0], 51.45125, -0.09082) < 3.0, "start at {}, {}", route.lat[0], route.lon[0]);
        assert_eq!(route.name, if clockwise { "Herne Hill Velodrome (clockwise)" } else { "Herne Hill Velodrome" });
    }
}

#[test]
fn anticlockwise_heads_north_from_the_line_and_clockwise_south() {
    let (anti, clock) = (HERNE_HILL.route(false), HERNE_HILL.route(true));
    assert!(anti.lat[10] > anti.lat[0], "anticlockwise rides north up the home straight");
    assert!(clock.lat[10] < clock.lat[0], "clockwise rides south");
    // Signed area of the outline in (east, north): positive is anticlockwise.
    let area = |r: &velopopuli_core::gpx::Route| -> f64 { (0..r.n - 1).map(|i| r.x[i] * -r.z[i + 1] - r.x[i + 1] * -r.z[i]).sum() };
    assert!(area(&anti) > 0.0 && area(&clock) < 0.0);
}

#[test]
fn track_ids_say_which_way_round() {
    assert!(track_for("herne-hill").is_some_and(|(t, cw)| t.id == "herne-hill" && !cw));
    assert!(track_for("herne-hill-clockwise").is_some_and(|(_, cw)| cw));
    assert!(track_for("demo").is_none());
}

mod track {
    use std::sync::Arc;
    use velopopuli_core::velodrome::HERNE_HILL;
    use velopopuli_core::world::{World, CHUNK, TRACK};

    fn world(clockwise: bool) -> World {
        World::new(Arc::new(HERNE_HILL.route(clockwise)), None, 0.0).with_venue(HERNE_HILL, clockwise)
    }

    #[test]
    fn the_track_is_banked_up_to_the_outside_steepest_in_the_bends() {
        for clockwise in [false, true] {
            let w = world(clockwise);
            let half = HERNE_HILL.width / 2.0;
            // Which side is the inside: the anticlockwise centre of the oval is on the left (+offset).
            let inside = if clockwise { -1.0 } else { 1.0 };
            let degrees: Vec<f64> = (0..w.route.n).map(|i| w.bank(i).abs().atan().to_degrees()).collect();
            let steepest = degrees.iter().copied().fold(0.0, f64::max);
            let flattest = degrees.iter().copied().fold(90.0, f64::min);
            assert!((steepest - HERNE_HILL.bend_banking).abs() < 0.5, "bends banked at {steepest:.1}°");
            assert!((flattest - HERNE_HILL.straight_banking).abs() < 0.5, "straights banked at {flattest:.1}°");
            for i in (0..w.route.n).step_by(25) {
                assert!(w.surface_lift(i, -inside * half) > w.surface_lift(i, inside * half) + 0.5, "outside higher at sample {i}");
                assert!(w.surface_lift(i, inside * half).abs() < 1e-9, "the inside edge stays on the ground");
            }
            // The line is on the home straight, so it is banked as a straight.
            assert!((degrees[0] - HERNE_HILL.straight_banking).abs() < 0.5);
        }
    }

    #[test]
    fn the_track_is_drawn_as_a_track_with_its_paint_inside_whichever_way_round() {
        let (anti, clock) = (world(false), world(true));
        let (a, c) = (anti.road(), clock.road());
        assert!(a.looks.iter().all(|&l| l == TRACK));
        // Texture across: the inside's markings are at the high end. Anticlockwise the inside is
        // at +offset (the fourth vertex across); clockwise it is mirrored.
        assert!(a.uvs[3 * 2] > 0.5 && c.uvs[3 * 2] < 0.5);
        assert!(anti.barriers().is_empty(), "a railing, not race barriers");
        assert!(anti.structures().lit.vertices() > 20_000, "the grounds are built on");
    }

    #[test]
    fn the_grounds_are_level_and_kept_clear() {
        let w = world(false);
        let route = &w.route;
        // The middle of the oval and the pavilion's spot, at the track's height.
        let (cx, cz) = route.to_local(HERNE_HILL.centre.0, HERNE_HILL.centre.1);
        assert!(w.field.probe(cx, cz, 1.0).ground.abs() < 1e-6);
        let chunk = w.chunk((cx / CHUNK).floor() as i32, (cz / CHUNK).floor() as i32);
        let trees_in_infield = chunk.trees.chunks(8).filter(|t| (t[0] as f64 - cx).hypot(t[2] as f64 - cz) < 40.0).count();
        assert_eq!(trees_in_infield, 0);
    }
}
