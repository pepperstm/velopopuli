"""Fixes from review: partial packs, re-runs during outages, duplicates, water, the token, long routes."""

import math

from opendata import buildings, corridor, pack


def failing() -> dict:
    raise OSError("server down")


def test_trees_corridor_only_when_there_are_trees():
    assert pack.assemble({"Meta/WRI canopy height": failing}, trees_corridor_m=500.0)["trees_corridor_m"] == 0
    with_trees = pack.assemble({"Meta/WRI canopy height": lambda: {"trees": [[-2.09, -79.87, 12.0]]}}, trees_corridor_m=500.0)
    assert with_trees["trees_corridor_m"] == 500.0


def test_a_failed_source_keeps_what_the_last_pack_had():
    previous = {"format": 1, "power_lines": [{"voltage_kind": "line", "points": [[0, 0], [0, 1]], "supports": ["tower", "tower"]}], "buildings": [{"ring": [], "height": 6}]}
    result = pack.assemble({"OpenStreetMap": failing, "Overture Maps": lambda: {"buildings": [{"ring": [[1, 1]], "height": 9}]}}, trees_corridor_m=500.0, previous=previous)
    assert result["power_lines"] == previous["power_lines"]
    assert result["buildings"] == [{"ring": [[1, 1]], "height": 9}]


def test_nothing_is_worth_writing_when_every_source_failed():
    assert not pack.worth_writing(pack.assemble({"OpenStreetMap": failing, "Mapillary": failing}, trees_corridor_m=500.0))
    assert pack.worth_writing(pack.assemble({"OpenStreetMap": lambda: {"lamps": []}}, trees_corridor_m=500.0))


def test_a_rerun_where_every_source_fails_keeps_the_old_pack():
    previous = {"format": 1, "sources": ["ESA WorldCover 10 m 2021 v200 (CC BY 4.0)"], "water": [[[[0, 0], [0, 1], [1, 1], [0, 0]]]]}
    result = pack.assemble({"ESA WorldCover": failing}, trees_corridor_m=500.0, previous=previous)
    assert result["water"] == previous["water"] and not pack.worth_writing(result)


def test_poles_beside_supports_or_lamps_and_close_lamps_are_one():
    m = 1 / 111_320
    support = [-2.09, -79.87]
    result = pack.assemble(
        {
            "OpenStreetMap": lambda: {"power_lines": [{"voltage_kind": "minor_line", "points": [support, [-2.08, -79.87]], "supports": ["pole", "pole"]}], "lamps": [[-2.07, -79.87]]},
            "Mapillary": lambda: {"poles": [[-2.09 + 3 * m, -79.87], [-2.07 + 5 * m, -79.87], [-2.05, -79.87]], "lamps": [[-2.07 + 8 * m, -79.87]]},
        },
        trees_corridor_m=500.0,
    )
    assert result["poles"] == [[-2.05, -79.87]]
    assert len(result["lamps"]) == 1


def test_water_keeps_off_houses_and_roads():
    c = corridor.Corridor([(-2.10, -79.87), (-2.09, -79.87)], 500.0)
    k = 111_320 * math.cos(math.radians(-2.095))
    sq = lambda x0, y0, s: [[-2.095 + y0 / 111_320, -79.869 + x0 / k], [-2.095 + y0 / 111_320, -79.869 + (x0 + s) / k], [-2.095 + (y0 + s) / 111_320, -79.869 + (x0 + s) / k], [-2.095 + (y0 + s) / 111_320, -79.869 + x0 / k], [-2.095 + y0 / 111_320, -79.869 + x0 / k]]
    water = [[sq(0, 0, 200)]]
    house = {"ring": sq(50, 50, 10), "height": 6}
    road = [[-2.095 + 150 / 111_320, -79.869 - 10 / k], [-2.095 + 150 / 111_320, -79.869 + 210 / k]]
    cleared = pack.clear_water(water, [house], [road], c)
    inside = lambda lat, lon: any(c.area.contains(c.local_geometry(__import__("shapely").geometry.Point(lon, lat))) and pack._polygon(a, c).contains(c.local_geometry(__import__("shapely").geometry.Point(lon, lat))) for a in cleared)
    assert not inside(-2.095 + 55 / 111_320, -79.869 + 55 / k), "not in the house"
    assert not inside(-2.095 + 150 / 111_320, -79.869 + 100 / k), "not on the road"
    assert inside(-2.095 + 20 / 111_320, -79.869 + 150 / k), "still water elsewhere"


def test_the_mapillary_token_never_reaches_the_log_or_the_pack():
    def leaky() -> dict:
        raise OSError("404 for url: https://tiles.mapillary.com/x?access_token=MLY|SECRET123&y=2")

    result = pack.assemble({"Mapillary": leaky}, trees_corridor_m=500.0)
    assert "SECRET123" not in result["failed"][0] and "access_token=" in result["failed"][0]


def test_buildings_are_asked_for_piece_by_piece():
    sql, params = buildings.piece_filter([(-2.1, -79.9, -2.09, -79.88), (-2.09, -79.88, -2.08, -79.87)])
    assert sql.count("bbox.xmin") == 2 and len(params) == 8
