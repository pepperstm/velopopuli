"""Putting the pack together and writing it where the app reads it."""

import json
import logging
import math
import os
from collections.abc import Callable
from pathlib import Path

from . import PACK_FORMAT

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


def dedupe(points: list[tuple[float, float]], within_m: float) -> list[tuple[float, float]]:
    """Points with any within `within_m` of an earlier one dropped."""
    kept: list[tuple[float, float]] = []
    for lat, lon in points:
        kx = 111_320.0 * math.cos(math.radians(lat))
        if all(math.hypot((lat - a) * 111_320.0, (lon - b) * kx) >= within_m for a, b in kept):
            kept.append((lat, lon))
    return kept


def assemble(sources: dict[str, Callable[[], dict]], trees_corridor_m: float) -> dict:
    """Runs each source and merges what it gives; a source that fails is listed in `failed`
    and the others still make the pack."""
    result: dict = {"format": PACK_FORMAT, "sources": [], "failed": [], "trees_corridor_m": trees_corridor_m}
    for field in LIST_FIELDS:
        result[field] = []
    for name, fetch in sources.items():
        try:
            part = fetch()
        except Exception as why:  # noqa: BLE001 - any failure of one source must not lose the others
            log.warning("%s failed: %s", name, why)
            result["failed"].append(f"{name}: {why}")
            continue
        for field, values in part.items():
            result.setdefault(field, []).extend(values)
        result["sources"].append(CREDITS.get(name, name))
        log.info("%s: %s", name, ", ".join(f"{len(v)} {k}" for k, v in part.items()))
    result["lamps"] = [list(p) for p in dedupe([tuple(p) for p in result["lamps"]], 5.0)]
    return result


def write(data_dir: Path, route_id: str, contents: dict) -> Path:
    """Writes the pack as `data/opendata-<id>.json`, all at once (a temporary file, then a rename)."""
    path = data_dir / "data" / f"opendata-{route_id}.json"
    temp = path.with_suffix(".json.tmp")
    temp.write_text(json.dumps(contents, separators=(",", ":")))
    os.replace(temp, path)
    return path
