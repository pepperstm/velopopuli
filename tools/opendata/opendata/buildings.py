"""Buildings OpenStreetMap lacks, from Overture Maps (which brings Google Open Buildings and
Microsoft ML Buildings), read straight from its GeoParquet files over S3."""

import logging

import duckdb
from shapely import wkb

from .corridor import Corridor

log = logging.getLogger(__name__)

RELEASE = "2026-11-18.0"
SOURCE = f"s3://overturemaps-us-west-2/release/{RELEASE}/theme=buildings/type=building/*"
MAX_BUILDINGS = 80_000
STOREY_M = 3.0
DEFAULT_HEIGHT_M = 6.0


def wanted(source: str | None) -> bool:
    """OpenStreetMap's own buildings come to the app from the map; the pack adds the rest."""
    return source != "OpenStreetMap"


def height_of(height: float | None, floors: int | None) -> float:
    if height:
        return float(height)
    if floors:
        return float(floors) * STOREY_M
    return DEFAULT_HEIGHT_M


PIECE_M = 10_000.0


def piece_filter(pieces: list[tuple[float, float, float, float]]) -> tuple[str, list[float]]:
    """A WHERE clause asking only for the boxes along the route (south, west, north, east each),
    not the whole route's bounding box, and its parameters."""
    clause = " OR ".join("(bbox.xmin > ? AND bbox.xmax < ? AND bbox.ymin > ? AND bbox.ymax < ?)" for _ in pieces)
    params = [v for s, w, n, e in pieces for v in (w, e, s, n)]
    return f"({clause})", params


def boxes(c: Corridor) -> list[tuple[float, float, float, float]]:
    """The boxes the buildings are asked for in: one per 10 km of route. A box per query lets the
    reader skip every file that misses it (one query OR-ing many boxes can't, and reads them all);
    long pieces keep the queries few, as each costs half a minute whatever its size."""
    return c.pieces(PIECE_M)


def fetch(c: Corridor) -> dict:
    con = duckdb.connect()
    con.execute("INSTALL httpfs; LOAD httpfs; SET s3_region='us-west-2';")
    seen: set[str] = set()
    rows = []
    for box in boxes(c):
        where, params = piece_filter([box])
        for row in con.execute(f"SELECT id, sources[1].dataset, height, num_floors, geometry FROM read_parquet('{SOURCE}', hive_partitioning=1) WHERE {where}", params).fetchall():
            if row[0] not in seen:
                seen.add(row[0])
                rows.append(row)
        log.info("Overture: %d buildings so far", len(rows))
    log.info("Overture: %d buildings along the route", len(rows))
    out = []
    for _id, source, height, floors, geometry in rows:
        if not wanted(source):
            continue
        shape = wkb.loads(bytes(geometry))
        if shape.geom_type == "MultiPolygon":
            shape = max(shape.geoms, key=lambda g: g.area)
        if shape.geom_type != "Polygon" or not c.area.contains(c.local_geometry(shape.centroid)):
            continue
        out.append({"ring": [[lat, lon] for lon, lat in shape.exterior.coords], "height": height_of(height, floors)})
        if len(out) >= MAX_BUILDINGS:
            log.warning("Overture: stopping at %d buildings", MAX_BUILDINGS)
            break
    return {"buildings": out}
