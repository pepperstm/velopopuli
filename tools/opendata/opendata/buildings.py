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


def fetch(c: Corridor) -> dict:
    s, w, n, e = c.bbox()
    con = duckdb.connect()
    con.execute("INSTALL httpfs; LOAD httpfs; SET s3_region='us-west-2';")
    rows = con.execute(
        f"""SELECT sources[1].dataset, height, num_floors, geometry FROM read_parquet('{SOURCE}', hive_partitioning=1)
            WHERE bbox.xmin > ? AND bbox.xmax < ? AND bbox.ymin > ? AND bbox.ymax < ?""",
        [w, e, s, n],
    ).fetchall()
    log.info("Overture: %d buildings in the route's box", len(rows))
    out = []
    for source, height, floors, geometry in rows:
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
