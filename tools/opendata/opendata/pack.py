"""Putting the pack together and writing it where the app reads it."""

import json
import logging
import math
import os
import re
from collections.abc import Callable
from pathlib import Path

from shapely.geometry import LineString, Polygon
from shapely.ops import unary_union

from . import PACK_FORMAT
from .corridor import Corridor

log = logging.getLogger(__name__)

# What each source contributes to the credits list in the pack (and README).
CREDITS = {
    "OpenStreetMap": "OpenStreetMap contributors (ODbL), via the Overpass API",
    "Overture Maps": "Overture Maps Foundation buildings (ODbL), including Google Open Buildings (CC BY 4.0) and Microsoft ML Buildings (ODbL)",
    "Meta/WRI canopy height": "Meta and World Resources Institute, High Resolution Canopy Height Maps 2024 (CC BY 4.0), from Maxar imagery",
    "ESA WorldCover": "ESA WorldCover 10 m 2021 v200 (CC BY 4.0)",
    "Mapillary": "Mapillary map features (CC BY-SA)",
}

LIST_FIELDS = ("power_lines", "lamps", "poles", "cycleways", "jersey", "water_towers", "buildings", "trees", "water")
# What each source fills in, so a source that is down this time keeps what the last pack had.
FIELDS_BY_SOURCE = {
    "OpenStreetMap": ("power_lines", "lamps", "cycleways", "jersey", "water_towers"),
    "Overture Maps": ("buildings",),
    "Meta/WRI canopy height": ("trees",),
    "ESA WorldCover": ("water",),
    "Mapillary": ("lamps", "poles"),
}
# The same lamp seen twice (OpenStreetMap and Mapillary) within this; a pole this close to a
# power-line support or a lamp is that support or lamp.
LAMP_SAME_M = 10.0
POLE_SAME_M = 8.0
# Water keeps this far from houses and other roads, and smaller pieces are dropped.
WATER_CLEAR_OF_HOUSES_M = 3.0
WATER_CLEAR_OF_ROADS_M = 6.0
MIN_WATER_M2 = 2000.0


def redact(message: str) -> str:
    """A message with any access token blanked out."""
    return re.sub(r"(access_token=)[^&\s]+", r"\1…", message)


def _near(p: list[float] | tuple[float, float], others: list, within_m: float) -> bool:
    kx = 111_320.0 * math.cos(math.radians(p[0]))
    return any(math.hypot((p[0] - o[0]) * 111_320.0, (p[1] - o[1]) * kx) < within_m for o in others)


def dedupe(points: list[tuple[float, float]], within_m: float) -> list[tuple[float, float]]:
    """Points with any within `within_m` of an earlier one dropped."""
    kept: list[tuple[float, float]] = []
    for lat, lon in points:
        kx = 111_320.0 * math.cos(math.radians(lat))
        if all(math.hypot((lat - a) * 111_320.0, (lon - b) * kx) >= within_m for a, b in kept):
            kept.append((lat, lon))
    return kept


def assemble(sources: dict[str, Callable[[], dict]], trees_corridor_m: float, previous: dict | None = None) -> dict:
    """Runs each source and merges what it gives. A source that fails is listed in `failed`, the
    others still make the pack, and what the failed one gave last time (`previous`, the pack
    already on disk) is kept."""
    result: dict = {"format": PACK_FORMAT, "sources": [], "failed": [], "answered": 0, "trees_corridor_m": 0.0}
    for field in LIST_FIELDS:
        result[field] = []
    failed_names = []
    for name, fetch in sources.items():
        try:
            part = fetch()
        except Exception as why:  # noqa: BLE001 - any failure of one source must not lose the others
            message = redact(str(why))
            log.warning("%s failed: %s", name, message)
            result["failed"].append(f"{name}: {message}")
            failed_names.append(name)
            continue
        for field, values in part.items():
            result.setdefault(field, []).extend(values)
        result["sources"].append(CREDITS.get(name, name))
        result["answered"] += 1
        log.info("%s: %s", name, ", ".join(f"{len(v)} {k}" for k, v in part.items() if not k.startswith("_")))
    if previous and previous.get("format") == PACK_FORMAT:
        for name in failed_names:
            for field in FIELDS_BY_SOURCE.get(name, ()):
                old = previous.get(field, [])
                if field == "lamps":
                    result["lamps"].extend(old)
                elif not result[field]:
                    result[field] = old
            credit = CREDITS.get(name, name)
            if credit in previous.get("sources", []) and credit not in result["sources"] and any(previous.get(f) for f in FIELDS_BY_SOURCE.get(name, ())):
                result["sources"].append(credit)
    result["lamps"] = [list(p) for p in dedupe([tuple(p) for p in result["lamps"]], LAMP_SAME_M)]
    supports = [pt for line in result["power_lines"] for pt, kind in zip(line["points"], line["supports"]) if kind]
    poles = [p for p in result["poles"] if not _near(p, supports, POLE_SAME_M) and not _near(p, result["lamps"], POLE_SAME_M)]
    result["poles"] = [list(p) for p in dedupe([tuple(p) for p in poles], POLE_SAME_M)]
    if result["trees"]:
        result["trees_corridor_m"] = trees_corridor_m
    return result


def worth_writing(result: dict) -> bool:
    """Whether a run gave anything new: when every source failed, the pack on disk stays as it is."""
    return result.get("answered", 0) > 0


def _polygon(area: list, c: Corridor) -> Polygon:
    """A water area ([lat, lon] rings: outline, then holes) in the corridor's local metres."""
    rings = [[c.to_local(lat, lon) for lat, lon in ring] for ring in area]
    return Polygon(rings[0], rings[1:]).buffer(0)


def clear_water(water: list, houses: list, roads: list, c: Corridor) -> list:
    """Water areas with houses and roads (as [lat, lon] lines) cut out of them, and the pieces
    left too small to be ponds dropped."""
    obstacles = [Polygon([c.to_local(lat, lon) for lat, lon in h["ring"]]).buffer(WATER_CLEAR_OF_HOUSES_M) for h in houses if len(h.get("ring", [])) >= 4]
    obstacles += [LineString([c.to_local(lat, lon) for lat, lon in r]).buffer(WATER_CLEAR_OF_ROADS_M) for r in roads if len(r) >= 2]
    blocked = unary_union(obstacles) if obstacles else None
    out = []
    for area in water:
        shape = _polygon(area, c)
        if blocked is not None and shape.intersects(blocked):
            shape = shape.difference(blocked)
        for part in getattr(shape, "geoms", [shape]):
            if part.geom_type == "Polygon" and part.area >= MIN_WATER_M2:
                out.append([[list(c.to_latlon(x, y)) for x, y in ring.coords] for ring in [part.exterior, *part.interiors]])
    return out


def write(data_dir: Path, route_id: str, contents: dict) -> Path:
    """Writes the pack as `data/opendata-<id>.json`, all at once (a temporary file, then a rename)."""
    path = data_dir / "data" / f"opendata-{route_id}.json"
    temp = path.with_suffix(".json.tmp")
    temp.write_text(json.dumps(contents, separators=(",", ":")))
    os.replace(temp, path)
    return path
