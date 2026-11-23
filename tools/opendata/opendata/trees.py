"""Trees measured from space: Meta/WRI's 1 m canopy height maps, read a piece of route at a time
from Cloud-Optimized GeoTIFFs, one tree per crown."""

import json
import logging
from functools import partial
from pathlib import Path

import numpy as np
import rasterio
import requests
from rasterio.features import geometry_mask
from rasterio.warp import transform, transform_bounds
from rasterio.windows import Window, from_bounds
from shapely.geometry import box, mapping, shape
from shapely.ops import transform as shp_transform

from . import USER_AGENT
from .corridor import Corridor

log = logging.getLogger(__name__)

BASE = "https://dataforgood-fb-data.s3.amazonaws.com/forests/v1/alsgedi_global_v6_float"
MIN_HEIGHT_M = 3
NEIGHBOURHOOD_PX = 5
MAX_TREES = 200_000
PIECE_M = 1000.0


def crown_tops(canopy: np.ndarray, min_height: int, neighbourhood: int) -> list[tuple[int, int, int]]:
    """(row, column, height) of each crown's highest pixel: at least `min_height` and the highest
    in its `neighbourhood` × `neighbourhood` square (the first such pixel where a crown is flat)."""
    pad = neighbourhood // 2
    padded = np.pad(canopy, pad, mode="constant")
    windows = np.lib.stride_tricks.sliding_window_view(padded, (neighbourhood, neighbourhood))
    local_max = windows.max(axis=(2, 3))
    tops = (canopy >= min_height) & (canopy == local_max)
    out, taken = [], np.zeros_like(tops)
    for r, c in zip(*np.nonzero(tops)):
        if taken[max(0, r - pad) : r + pad + 1, max(0, c - pad) : c + pad + 1].any():
            continue
        taken[r, c] = True
        out.append((int(r), int(c), int(canopy[r, c])))
    return out


def _tiles(cache: Path) -> list[dict]:
    path = cache / "canopy-tiles.geojson"
    if not path.exists():
        cache.mkdir(parents=True, exist_ok=True)
        reply = requests.get(f"{BASE}/tiles.geojson", headers={"User-Agent": USER_AGENT}, timeout=120)
        reply.raise_for_status()
        path.write_bytes(reply.content)
    return json.loads(path.read_text())["features"]


def fetch(c: Corridor, cache: Path) -> dict:
    tiles = _tiles(cache)
    found: dict[tuple[float, float], float] = {}
    for s, w, n, e in c.pieces(PIECE_M):
        piece = box(w, s, e, n)
        for tile in (t for t in tiles if shape(t["geometry"]).intersects(piece)):
            url = f"/vsicurl/{BASE}/chm/{tile['properties']['tile']}.tif"
            with rasterio.open(url) as src:
                bounds = transform_bounds("EPSG:4326", src.crs, w, s, e, n)
                window = from_bounds(*bounds, transform=src.transform).round_offsets().round_lengths()
                window = window.intersection(Window(0, 0, src.width, src.height))
                canopy = src.read(1, window=window)
                if canopy.size == 0:
                    continue
                win_transform = src.window_transform(window)
                # Only the crowns within the corridor.
                strip = shp_transform(partial(_to_crs, src.crs), c.lonlat_geometry(c.area.intersection(c.local_box(s, w, n, e))))
                outside = geometry_mask([mapping(strip)], out_shape=canopy.shape, transform=win_transform)
                canopy = np.where(outside, 0, canopy)
                tops = crown_tops(canopy, MIN_HEIGHT_M, NEIGHBOURHOOD_PX)
                if not tops:
                    continue
                xs, ys = rasterio.transform.xy(win_transform, [r for r, _, _ in tops], [col for _, col, _ in tops])
                lons, lats = transform(src.crs, "EPSG:4326", list(np.atleast_1d(xs)), list(np.atleast_1d(ys)))
                for lat, lon, (_, _, h) in zip(lats, lons, tops):
                    found[(round(lat, 6), round(lon, 6))] = float(h)
        log.info("canopy: %d trees so far", len(found))
    trees = sorted(([lat, lon, h] for (lat, lon), h in found.items()), key=lambda t: -t[2])[:MAX_TREES]
    return {"trees": trees}



def _to_crs(crs, x, y, z=None):
    """For shapely's transform: (lon, lat) arrays into `crs`."""
    xs, ys = transform("EPSG:4326", crs, list(np.atleast_1d(x)), list(np.atleast_1d(y)))
    return np.array(xs), np.array(ys)
