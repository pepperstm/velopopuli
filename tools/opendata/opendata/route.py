"""A route from the app's library: its GPX track as (lat, lon) points."""

import json
import xml.etree.ElementTree as ET
from pathlib import Path


def gpx_points(gpx: str) -> list[tuple[float, float]]:
    """Track points (or route points if there are none), in order, as (lat, lon)."""
    root = ET.fromstring(gpx)
    points = [el for el in root.iter() if el.tag.rsplit("}", 1)[-1] == "trkpt"]
    if not points:
        points = [el for el in root.iter() if el.tag.rsplit("}", 1)[-1] == "rtept"]
    return [(float(el.attrib["lat"]), float(el.attrib["lon"])) for el in points]


def load(data_dir: Path, route_id: str) -> list[tuple[float, float]]:
    """The points of route `route_id` in the app's data folder (`data/route-<id>.json`)."""
    stored = json.loads((data_dir / "data" / f"route-{route_id}.json").read_text())
    return gpx_points(stored["gpx"])


def library(data_dir: Path) -> list[dict]:
    """The routes in the app's library (`data/routes.json`), as stored."""
    path = data_dir / "data" / "routes.json"
    return json.loads(path.read_text()) if path.exists() else []
