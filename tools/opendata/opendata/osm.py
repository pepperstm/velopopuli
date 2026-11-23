"""OpenStreetMap through the Overpass API: power lines, street lamps, cycle lanes, median
barriers and water towers along the route."""

import logging
import time

import requests
from shapely.geometry import LineString

from . import USER_AGENT
from .corridor import Corridor

log = logging.getLogger(__name__)

SERVERS = ("https://overpass-api.de/api/interpreter", "https://overpass.openstreetmap.fr/api/interpreter", "https://overpass.private.coffee/api/interpreter")
TIMEOUT_S = 60
ROUNDS = 3
LANE_VALUES = ("lane", "track", "shared_lane")


def overpass(query: str) -> dict:
    """The answer to an Overpass query, trying each server in turn, up to `ROUNDS` times."""
    last: Exception | None = None
    for round_ in range(ROUNDS):
        for server in SERVERS:
            try:
                reply = requests.post(server, data={"data": query}, headers={"User-Agent": USER_AGENT}, timeout=TIMEOUT_S + 15)
                reply.raise_for_status()
                body = reply.json()
                # A server that ran out of time or memory still answers 200, with a remark and
                # whatever it had found so far: that is a failure, not an empty answer.
                remark = str(body.get("remark", ""))
                if "error" in remark.lower() or "timed out" in remark.lower():
                    raise ValueError(f"incomplete answer: {remark[:120]}")
                return body
            except (requests.RequestException, ValueError) as why:
                last = why
                log.info("Overpass %s: %s", server, why)
        time.sleep(5 * (round_ + 1))
    raise OSError(f"no Overpass server answered ({last})")


def _around(c: Corridor) -> str:
    return ",".join(f"{lat:.5f},{lon:.5f}" for lat, lon in c.sample_line(150.0))


def road_width(tags: dict) -> float:
    """A road's carriageway width from its tags, as the app estimates it."""
    try:
        return float(str(tags.get("width", "")).split()[0])
    except (ValueError, IndexError):
        pass
    try:
        return float(tags["lanes"]) * 3.25 + 0.5
    except (KeyError, ValueError):
        return 7.0


def cycle_lines(way: dict) -> list[list[tuple[float, float]]]:
    """The cycle lanes a way stands for, as (lat, lon) lines: a cycle track itself, or a road's
    painted lanes, drawn just beyond the road's edge on their side."""
    geometry = [(g["lat"], g["lon"]) for g in way.get("geometry", [])]
    tags = way.get("tags", {})
    if len(geometry) < 2:
        return []
    if tags.get("highway") == "cycleway":
        return [geometry]
    sides = []
    if tags.get("cycleway") in LANE_VALUES or tags.get("cycleway:both") in LANE_VALUES:
        sides = ["right", "left"]
    for side in ("right", "left"):
        if tags.get(f"cycleway:{side}") in LANE_VALUES and side not in sides:
            sides.append(side)
    if not sides:
        return []
    c = Corridor(geometry, 1.0)
    line = LineString([c.to_local(lat, lon) for lat, lon in geometry])
    out_m = road_width(tags) / 2 + 0.9
    lines = []
    for side in sides:
        # shapely offsets to the left for a positive distance.
        shifted = line.offset_curve(-out_m if side == "right" else out_m)
        lines.append([c.to_latlon(x, y) for x, y in shifted.coords])
    return lines


def fetch(c: Corridor) -> dict:
    """Everything this source gives the pack."""
    line = _around(c)
    out: dict = {"power_lines": [], "lamps": [], "cycleways": [], "jersey": [], "water_towers": []}

    power = overpass(f'[out:json][timeout:{TIMEOUT_S}];way["power"~"^(line|minor_line)$"](around:300,{line})->.p;(.p;>;);out body;')
    nodes = {e["id"]: e for e in power["elements"] if e["type"] == "node"}
    for way in (e for e in power["elements"] if e["type"] == "way"):
        pts = [nodes[n] for n in way.get("nodes", []) if n in nodes]
        if len(pts) >= 2:
            supports = [p.get("tags", {}).get("power", "") if p.get("tags", {}).get("power") in ("tower", "pole") else "" for p in pts]
            out["power_lines"].append({"voltage_kind": way["tags"].get("power", "line"), "points": [[p["lat"], p["lon"]] for p in pts], "supports": supports})

    lamps = overpass(f'[out:json][timeout:{TIMEOUT_S}];node["highway"="street_lamp"](around:30,{line});out;')
    out["lamps"] = [[e["lat"], e["lon"]] for e in lamps["elements"]]

    lanes = f'way["highway"="cycleway"](around:25,{line});' + "".join(f'way["{k}"~"^(lane|track|shared_lane)$"](around:20,{line});' for k in ("cycleway", "cycleway:right", "cycleway:left", "cycleway:both"))
    for way in overpass(f"[out:json][timeout:{TIMEOUT_S}];({lanes});out geom;")["elements"]:
        out["cycleways"].extend([[lat, lon] for lat, lon in ln] for ln in cycle_lines(way))

    jersey = overpass(f'[out:json][timeout:{TIMEOUT_S}];way["barrier"="jersey_barrier"](around:60,{line});out geom;')
    out["jersey"] = [[[g["lat"], g["lon"]] for g in e["geometry"]] for e in jersey["elements"] if len(e.get("geometry", [])) >= 2]

    # The other roads in the corridor, so water from the land-cover map keeps off them.
    try:
        roads = overpass(f'[out:json][timeout:{TIMEOUT_S}];way["highway"~"^(motorway|trunk|primary|secondary|tertiary|unclassified|residential|service|track|living_street)"](around:500,{line});out geom;')
        out["_roads"] = [[(g["lat"], g["lon"]) for g in e["geometry"]] for e in roads["elements"] if len(e.get("geometry", [])) >= 2]
    except OSError as why:
        log.info("roads for clearing water left out: %s", why)

    towers = overpass(f'[out:json][timeout:{TIMEOUT_S}];(node["man_made"="water_tower"](around:2000,{line});way["man_made"="water_tower"](around:2000,{line}););out center;')
    out["water_towers"] = [[e.get("lat", e.get("center", {}).get("lat")), e.get("lon", e.get("center", {}).get("lon"))] for e in towers["elements"]]
    out["water_towers"] = [p for p in out["water_towers"] if None not in p]
    return out
