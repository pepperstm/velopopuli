"""Tests that need no network: routes, the corridor, trees, tile names, cycle lanes, buildings, the pack."""

import json

import numpy as np
import pytest

from opendata import buildings, corridor, landcover, osm, pack, route, trees

GPX = """<?xml version="1.0"?><gpx xmlns="http://www.topografix.com/GPX/1/1"><trk><name>t</name><trkseg>
<trkpt lat="-2.0986" lon="-79.8748"></trkpt><trkpt lat="-2.0900" lon="-79.8745"></trkpt><trkpt lat="-2.0800" lon="-79.8740"></trkpt>
</trkseg></trk></gpx>"""


def straight() -> corridor.Corridor:
    pts = [(-2.10 + k * 0.001, -79.87) for k in range(21)]  # about 2.2 km north
    return corridor.Corridor(pts, 500.0)


def test_gpx_points_are_read_in_order():
    pts = route.gpx_points(GPX)
    assert pts[0] == (-2.0986, -79.8748) and len(pts) == 3


def test_route_file_is_read_from_the_data_folder(tmp_path):
    (tmp_path / "data").mkdir()
    (tmp_path / "data" / "route-abc.json").write_text(json.dumps({"name": "x", "gpx": GPX}))
    assert len(route.load(tmp_path, "abc")) == 3


def test_corridor_reaches_its_width_and_no_further():
    c = straight()
    lat, lon = -2.09, -79.87
    east = 1 / (111_320 * np.cos(np.radians(lat)))
    assert c.contains(lat, lon + 300 * east)
    assert not c.contains(lat, lon + 800 * east)
    s, w, n, e = c.bbox()
    assert s < -2.10 and n > -2.08 and w < -79.874 and e > -79.866
    assert abs(c.distance_m(lat, lon + 100 * east) - 100) < 2


def test_corridor_pieces_cover_the_route():
    c = straight()
    pieces = c.pieces(1000.0)
    assert len(pieces) >= 2
    for s, w, n, e in pieces:
        assert s < n and w < e


def test_one_tree_per_crown():
    canopy = np.zeros((60, 60), dtype=np.uint8)
    yy, xx = np.mgrid[0:60, 0:60]
    for cy, cx, h in [(15, 15, 18), (40, 42, 9)]:
        canopy = np.maximum(canopy, (h * np.exp(-((yy - cy) ** 2 + (xx - cx) ** 2) / 20)).astype(np.uint8))
    canopy[55, 5] = 2  # too low to be a tree
    found = trees.crown_tops(canopy, min_height=3, neighbourhood=5)
    assert sorted((r, c, h) for r, c, h in found) == [(15, 15, 18), (40, 42, 9)]


def test_worldcover_tiles_are_named_by_their_corner():
    assert landcover.tile_name(-2.07, -79.87) == "S03W081"
    assert landcover.tile_name(1.5, 10.2) == "N00E009"
    assert landcover.tile_name(-0.5, -0.5) == "S03W003"


def test_cycle_lane_tags_become_lines_beside_the_road():
    # A road heading north, cycle lane on its right: the lane line lies east of the centre line.
    way = {"tags": {"highway": "primary", "cycleway:right": "lane", "lanes": "2"}, "geometry": [{"lat": -2.10, "lon": -79.87}, {"lat": -2.09, "lon": -79.87}]}
    lines = osm.cycle_lines(way)
    assert len(lines) == 1
    assert all(lon > -79.87 for _, lon in lines[0])
    both = osm.cycle_lines({**way, "tags": {"highway": "primary", "cycleway": "lane"}})
    assert len(both) == 2
    track = osm.cycle_lines({"tags": {"highway": "cycleway"}, "geometry": way["geometry"]})
    assert track == [[(-2.10, -79.87), (-2.09, -79.87)]]


def test_buildings_from_openstreetmap_are_left_to_the_map():
    rows = [("OpenStreetMap", None, None), ("Google Open Buildings", 7.5, None), ("Microsoft ML Buildings", None, 3)]
    kept = [buildings.height_of(h, f) for src, h, f in rows if buildings.wanted(src)]
    assert kept == [7.5, 9.0]


def test_lamps_seen_twice_are_one_lamp():
    lamps = [(-2.09, -79.87), (-2.09001, -79.87001), (-2.08, -79.87)]
    assert len(pack.dedupe(lamps, 5.0)) == 2


def test_a_failing_source_leaves_the_others_in_the_pack():
    def good() -> dict:
        return {"lamps": [[-2.09, -79.87]]}

    def bad() -> dict:
        raise OSError("server down")

    result = pack.assemble({"OpenStreetMap": good, "Mapillary": bad}, trees_corridor_m=500.0)
    assert result["lamps"] == [[-2.09, -79.87]]
    assert result["failed"] == ["Mapillary: server down"]
    assert result["format"] == 1 and any("OpenStreetMap" in s for s in result["sources"])


def test_pack_fields_are_the_apps():
    result = pack.assemble({}, trees_corridor_m=500.0)
    app_fields = {"format", "sources", "power_lines", "lamps", "poles", "cycleways", "jersey", "water_towers", "buildings", "trees", "trees_corridor_m", "water"}
    assert app_fields <= set(result)


def test_pack_is_written_whole(tmp_path):
    (tmp_path / "data").mkdir()
    path = pack.write(tmp_path, "abc", pack.assemble({}, trees_corridor_m=500.0))
    assert path.name == "opendata-abc.json" and json.loads(path.read_text())["format"] == 1
    assert not list((tmp_path / "data").glob("*.tmp"))
