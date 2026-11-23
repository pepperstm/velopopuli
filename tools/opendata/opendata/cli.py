"""`opendata --route <id>`: prepares a route's open-data pack in VeloPopuli's data folder."""

import argparse
import json
import logging
import os
import sys
from functools import partial
from pathlib import Path

from . import buildings, landcover, mapillary, osm, pack, route, trees
from .corridor import Corridor

log = logging.getLogger("opendata")

APP_ID = "org.velopopuli.VeloPopuli"
CORRIDOR_M = 500.0
MAX_ROUTE_KM = 150.0


def default_data_dir() -> Path:
    """Where VeloPopuli keeps its data: $VP_DATA_DIR, or the system's app-data folder."""
    if os.environ.get("VP_DATA_DIR"):
        return Path(os.environ["VP_DATA_DIR"])
    if sys.platform == "darwin":
        return Path.home() / "Library" / "Application Support" / APP_ID
    if sys.platform == "win32":
        return Path(os.environ.get("APPDATA", Path.home())) / APP_ID
    return Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local" / "share")) / APP_ID


def saved_mapillary_token(data_dir: Path) -> str:
    try:
        settings = json.loads((data_dir / "data" / "settings.json").read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return ""
    return settings.get("look", {}).get("mapillaryToken", "")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--route", help="route id in the app's library (see --list)")
    parser.add_argument("--list", action="store_true", help="list the routes in the library")
    parser.add_argument("--data-dir", type=Path, default=None, help="VeloPopuli's data folder")
    parser.add_argument("--mapillary-token", default=None, help="Mapillary client token (default: the one in the app's settings)")
    parser.add_argument("--skip", action="append", default=[], choices=["osm", "buildings", "trees", "water", "mapillary"], help="leave a source out")
    args = parser.parse_args(argv)
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(message)s", datefmt="%H:%M:%S")

    data_dir = args.data_dir or default_data_dir()
    if args.list or not args.route:
        for r in route.library(data_dir):
            print(f"{r['id']}\t{r.get('distance', 0) / 1000:.1f} km\t{r['name']}")
        return 0 if args.list else 2

    points = route.load(data_dir, args.route)
    c = Corridor(points, CORRIDOR_M)
    if c.line.length > MAX_ROUTE_KM * 1000:
        log.error("route is %.0f km; the pack is for routes up to %.0f km", c.line.length / 1000, MAX_ROUTE_KM)
        return 1
    token = args.mapillary_token if args.mapillary_token is not None else saved_mapillary_token(data_dir)
    cache = Path.home() / ".cache" / "velopopuli-opendata"
    sources = {
        "OpenStreetMap": partial(osm.fetch, c),
        "Overture Maps": partial(buildings.fetch, c),
        "Meta/WRI canopy height": partial(trees.fetch, c, cache),
        "ESA WorldCover": partial(landcover.fetch, c),
        "Mapillary": partial(mapillary.fetch, c, token),
    }
    skip = {"osm": "OpenStreetMap", "buildings": "Overture Maps", "trees": "Meta/WRI canopy height", "water": "ESA WorldCover", "mapillary": "Mapillary"}
    for s in args.skip:
        sources.pop(skip[s], None)
    log.info("route %s: %.1f km, corridor %.0f m either side", args.route, c.line.length / 1000, CORRIDOR_M)
    contents = pack.assemble(sources, trees_corridor_m=CORRIDOR_M)
    path = pack.write(data_dir, args.route, contents)
    log.info("wrote %s (%d buildings, %d trees, %d lamps, %d power lines)", path, len(contents["buildings"]), len(contents["trees"]), len(contents["lamps"]), len(contents["power_lines"]))
    for failure in contents["failed"]:
        log.warning("left out: %s", failure)
    return 0


if __name__ == "__main__":
    sys.exit(main())
