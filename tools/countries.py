"""Builds crates/core/src/countries.txt: simplified country outlines, for
finding which country a point is in (imagery sources, regional building styles).

Source: Natural Earth admin-0 countries, public domain (https://www.naturalearthdata.com):
1:110m for the world, and 1:50m for the countries in DETAILED, whose borders decide
which imagery source is used. Run from the repository root:

    python3 tools/countries.py ne_110m_admin_0_countries.geojson ne_50m_admin_0_countries.geojson

Format: one line per ring, `CC lat,lon lat,lon …` (ISO 3166-1 alpha-2, degrees to
three decimals), with `CC+` for a detailed country. A country can have many rings; holes
are left out. Detailed countries come first, so a lookup that takes the first match
prefers their sharper borders.
"""

import json
import sys

TOLERANCE = 0.01  # degrees, about a kilometre
# Countries with a national imagery source (crates/core/src/world/imagery.rs).
DETAILED = {"CH", "ES", "FR", "NL", "US"}
MIN_AREA = 0.0004  # square degrees: drop islets smaller than about 2 km across


def simplify(points, tol):
    """Douglas–Peucker, iterative."""
    if len(points) < 3:
        return points
    keep = [False] * len(points)
    keep[0] = keep[-1] = True
    stack = [(0, len(points) - 1)]
    while stack:
        a, b = stack.pop()
        (ax, ay), (bx, by) = points[a], points[b]
        dx, dy = bx - ax, by - ay
        length = (dx * dx + dy * dy) ** 0.5
        best, at = 0.0, None
        for i in range(a + 1, b):
            px, py = points[i]
            if length == 0:
                d = ((px - ax) ** 2 + (py - ay) ** 2) ** 0.5
            else:
                d = abs(dy * px - dx * py + bx * ay - by * ax) / length
            if d > best:
                best, at = d, i
        if at is not None and best > tol:
            keep[at] = True
            stack += [(a, at), (at, b)]
    return [p for p, k in zip(points, keep) if k]


def area(ring):
    return abs(sum(x0 * y1 - x1 * y0 for (x0, y0), (x1, y1) in zip(ring, ring[1:] + ring[:1]))) / 2


def rings(path, codes=None):
    out = []
    for f in sorted(json.load(open(path))["features"], key=lambda f: f["properties"]["ISO_A2_EH"]):
        code = f["properties"]["ISO_A2_EH"]
        if len(code) != 2 or not code.isalpha() or (codes is not None and code not in codes):
            continue
        g = f["geometry"]
        polygons = g["coordinates"] if g["type"] == "MultiPolygon" else [g["coordinates"]]
        for polygon in polygons:
            outer = polygon[0]
            if area(outer) < MIN_AREA:
                continue
            ring = simplify(outer, TOLERANCE)
            if ring[0] == ring[-1]:
                ring = ring[:-1]
            if len(ring) >= 3:
                out.append((code, ring))
    return out


def main(world, detailed):
    picked = rings(detailed, DETAILED) + [r for r in rings(world) if r[0] not in DETAILED]
    lines = [code + ("+" if code in DETAILED else "") + " " + " ".join(f"{lat:.3f},{lon:.3f}" for lon, lat in ring) for code, ring in picked]
    out = "crates/core/src/countries.txt"
    with open(out, "w") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"{out}: {len(lines)} rings, {len(set(c for c, _ in picked))} countries")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
