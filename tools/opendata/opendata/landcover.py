"""Water and wetland from ESA WorldCover (10 m, 2021), for the flooded fields and ponds the map
doesn't draw."""

import logging
import math

import numpy as np
import rasterio
from rasterio.features import shapes
from rasterio.windows import from_bounds
from shapely.geometry import shape
from shapely.ops import unary_union

from .corridor import Corridor

log = logging.getLogger(__name__)

BASE = "https://esa-worldcover.s3.eu-central-1.amazonaws.com/v200/2021/map"
WATER, WETLAND = 80, 90
MIN_AREA_M2 = 2000.0
SIMPLIFY_M = 5.0
CLEAR_OF_ROUTE_M = 15.0


def tile_name(lat: float, lon: float) -> str:
    """The 3° × 3° tile holding a point, named by its south-west corner (e.g. S03W081)."""
    south, west = math.floor(lat / 3) * 3, math.floor(lon / 3) * 3
    return f"{'N' if south >= 0 else 'S'}{abs(south):02d}{'E' if west >= 0 else 'W'}{abs(west):03d}"


PIECE_M = 3000.0


def fetch(c: Corridor) -> dict:
    """Water and wetland in the corridor, read a piece of route at a time."""
    keep = c.area.difference(c.line.buffer(CLEAR_OF_ROUTE_M))
    found = []
    for s, w, n, e in c.pieces(PIECE_M):
        piece = c.local_box(s, w, n, e).intersection(keep)
        for name in sorted({tile_name(lat, lon) for lat in (s, n) for lon in (w, e)}):
            with rasterio.open(f"/vsicurl/{BASE}/ESA_WorldCover_10m_2021_v200_{name}_Map.tif") as src:
                window = from_bounds(w, s, e, n, transform=src.transform).round_offsets().round_lengths()
                classes = src.read(1, window=window, boundless=True, fill_value=0)
                wet = np.isin(classes, (WATER, WETLAND)).astype(np.uint8)
                for geom, value in shapes(wet, mask=wet.astype(bool), transform=src.window_transform(window)):
                    if value == 1:
                        found.append(c.local_geometry(shape(geom)).intersection(piece))
    merged = unary_union([g for g in found if not g.is_empty]).simplify(SIMPLIFY_M) if found else None
    areas = []
    for part in getattr(merged, "geoms", [merged] if merged is not None else []):
        if part.geom_type != "Polygon" or part.area < MIN_AREA_M2:
            continue
        back = c.lonlat_geometry(part)
        areas.append([[[lat, lon] for lon, lat in ring.coords] for ring in [back.exterior, *back.interiors]])
    log.info("WorldCover: %d areas of water and wetland", len(areas))
    return {"water": areas}
