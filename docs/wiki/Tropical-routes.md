# Tropical routes

In the lowland tropics (within about 23.5° of the equator and below about 1,200 m) a route looks like the place it is in, not like a European lane. Nothing changes for routes anywhere else.

## What you'll see

- **Trees:** palms, low fan palms and wide rain trees instead of spruce, pine and birch.
- **Main roads in town (avenues):** where the route is one side of a divided road through a built-up area, the median between the two sides is planted with palms and bushes and lit by tall double-arm lamps; more palms stand along the outer side.
- **Divided roads out of town:** a concrete median barrier runs down the middle.
- **Town streets:** single-arm lamps along the right.
- **Housing estates:** a cream wall runs along the edge of an estate that faces the road.
- **Road paint in the Americas:** a yellow line down the middle of two-way roads (a double one on country roads), and along the left edge of a divided road's carriageway.

What kind of road each stretch is comes from the map: whether the route runs on one side of a divided road, and whether buildings or housing are nearby.

## Open road

**Settings → Virtual world → Open road (no race barriers)** rides the road as it is on an ordinary day: the crowd barriers go, and kerbs and sidewalks line the road where it is built up.

## Open data for a route

For more of the real things along a tropical route, in their real places, prepare the route's open-data pack once. You need [uv](https://docs.astral.sh/uv/) and a copy of the source code.

```bash
cd tools/opendata
uv run opendata --list          # your routes, with their ids
uv run opendata --route <id>    # prepares that route
```

On a Mac you can double-click `tools/opendata/preparar_datos.command` instead.

The pack covers 500 m either side of the route and takes a few minutes:

| What | Where it comes from |
|---|---|
| Power lines, with their towers and poles; street lamps; cycle lanes; concrete median barriers; water towers | OpenStreetMap |
| The houses OpenStreetMap hasn't got yet | Overture Maps (Google Open Buildings, Microsoft ML Buildings) |
| One tree per crown, as tall as measured | Meta and World Resources Institute canopy height maps |
| Ponds, flooded fields and wetland | ESA WorldCover |
| Street lamps and poles seen in street-level photos | Mapillary (needs your free token in **Settings → Virtual world**) |

If a source is down, the pack is made without it and the script says which; run it again later. The next ride on the route uses the pack. Things that are in the pack replace the ones made up by rule: real lamps stand where they are and the rule only fills the gaps, real trees stand where they grow.
