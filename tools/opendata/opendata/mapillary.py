"""Street lamps and utility poles detected in Mapillary's street-level photos (needs a free
client token: https://www.mapillary.com/dashboard/developers)."""

import logging
import math

import mapbox_vector_tile
import requests

from . import USER_AGENT
from .corridor import Corridor

log = logging.getLogger(__name__)

TILES = "https://tiles.mapillary.com/maps/vtp/mly_map_feature_point/2/14/{x}/{y}?access_token={token}"
ZOOM = 14
REACH_M = 30.0
MAX_TILES = 400


def _tile(lat: float, lon: float) -> tuple[int, int]:
    n = 2**ZOOM
    x = int((lon + 180) / 360 * n)
    y = int((1 - math.asinh(math.tan(math.radians(lat))) / math.pi) / 2 * n)
    return x, y


def _lonlat(tx: int, ty: int, px: float, py: float, extent: int) -> tuple[float, float]:
    n = 2**ZOOM
    lon = (tx + px / extent) / n * 360 - 180
    lat = math.degrees(math.atan(math.sinh(math.pi * (1 - 2 * (ty + 1 - py / extent) / n))))
    return lon, lat


def fetch(c: Corridor, token: str) -> dict:
    if not token:
        raise OSError("no Mapillary token (add one in VeloPopuli's settings or pass --mapillary-token)")
    s, w, n, e = c.bbox()
    (x0, y1), (x1, y0) = _tile(s, w), _tile(n, e)
    tiles = [(x, y) for x in range(x0, x1 + 1) for y in range(y0, y1 + 1)]
    if len(tiles) > MAX_TILES:
        raise OSError(f"the route covers {len(tiles)} Mapillary tiles, more than {MAX_TILES}")
    lamps, poles = [], []
    for tx, ty in tiles:
        reply = requests.get(TILES.format(x=tx, y=ty, token=token), headers={"User-Agent": USER_AGENT}, timeout=30)
        if reply.status_code == 404:
            continue
        reply.raise_for_status()
        for layer in mapbox_vector_tile.decode(reply.content).values():
            extent = layer.get("extent", 4096)
            for f in layer.get("features", []):
                value = f.get("properties", {}).get("value", "")
                if value not in ("object--street-light", "object--support--utility-pole") or f["geometry"]["type"] != "Point":
                    continue
                lon, lat = _lonlat(tx, ty, *f["geometry"]["coordinates"], extent)
                if c.distance_m(lat, lon) <= REACH_M:
                    (lamps if value == "object--street-light" else poles).append([lat, lon])
    log.info("Mapillary: %d lamps, %d poles", len(lamps), len(poles))
    return {"lamps": lamps, "poles": poles}
