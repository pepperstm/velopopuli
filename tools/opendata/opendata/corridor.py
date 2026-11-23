"""The strip of land either side of a route that the pack covers, in a local metric frame."""

import math

import numpy as np
import shapely
from shapely.geometry import LineString, Point, Polygon, box

M_PER_DEG = 111_320.0


class Corridor:
    """A route and everything within `width_m` of it.

    Positions are projected about the route's centroid (equirectangular, as the app does), which
    is accurate to well under a metre over any distance anyone rides.
    """

    def __init__(self, points: list[tuple[float, float]], width_m: float) -> None:
        if len(points) < 2:
            raise ValueError("a route needs at least two points")
        self.points = points
        self.width_m = width_m
        self.lat0 = sum(p[0] for p in points) / len(points)
        self.lon0 = sum(p[1] for p in points) / len(points)
        self._kx = M_PER_DEG * math.cos(math.radians(self.lat0))
        self.line = LineString([self.to_local(lat, lon) for lat, lon in points])
        self.area: Polygon = self.line.buffer(width_m)

    def to_local(self, lat: float, lon: float) -> tuple[float, float]:
        """(x east, y north) in metres from the centroid."""
        return ((lon - self.lon0) * self._kx, (lat - self.lat0) * M_PER_DEG)

    def to_latlon(self, x: float, y: float) -> tuple[float, float]:
        return (self.lat0 + y / M_PER_DEG, self.lon0 + x / self._kx)

    def local_geometry(self, geom):
        """A shapely geometry in (lon, lat) moved into local metres."""
        return shapely.transform(geom, lambda a: np.column_stack(((a[:, 0] - self.lon0) * self._kx, (a[:, 1] - self.lat0) * M_PER_DEG)))

    def lonlat_geometry(self, geom):
        """A shapely geometry in local metres moved back into (lon, lat)."""
        return shapely.transform(geom, lambda a: np.column_stack((self.lon0 + a[:, 0] / self._kx, self.lat0 + a[:, 1] / M_PER_DEG)))

    def contains(self, lat: float, lon: float) -> bool:
        return self.area.contains(Point(self.to_local(lat, lon)))

    def distance_m(self, lat: float, lon: float) -> float:
        """How far a point is from the route's line."""
        return self.line.distance(Point(self.to_local(lat, lon)))

    def bbox(self, margin_m: float = 0.0) -> tuple[float, float, float, float]:
        """(south, west, north, east) of the corridor, widened by `margin_m`."""
        x0, y0, x1, y1 = self.area.bounds
        s, w = self.to_latlon(x0 - margin_m, y0 - margin_m)
        n, e = self.to_latlon(x1 + margin_m, y1 + margin_m)
        return (s, w, n, e)

    def pieces(self, length_m: float) -> list[tuple[float, float, float, float]]:
        """Boxes (south, west, north, east) covering the corridor a piece of route at a time, so
        no one download spans the whole route's bounding box."""
        out = []
        total = self.line.length
        steps = max(1, math.ceil(total / length_m))
        for k in range(steps):
            a, b = k * total / steps, (k + 1) * total / steps
            piece = LineString([self.line.interpolate(d) for d in (a, (a + b) / 2, b)]).buffer(self.width_m)
            x0, y0, x1, y1 = piece.bounds
            s, w = self.to_latlon(x0, y0)
            n, e = self.to_latlon(x1, y1)
            out.append((s, w, n, e))
        return out

    def sample_line(self, every_m: float) -> list[tuple[float, float]]:
        """Points along the route every `every_m`, as (lat, lon), for Overpass `around:` filters."""
        total = self.line.length
        steps = max(1, math.ceil(total / every_m))
        return [self.to_latlon(*self.line.interpolate(k * total / steps).coords[0]) for k in range(steps + 1)]

    def local_box(self, s: float, w: float, n: float, e: float) -> Polygon:
        (x0, y0), (x1, y1) = self.to_local(s, w), self.to_local(n, e)
        return box(x0, y0, x1, y1)
