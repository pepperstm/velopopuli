# Routes and the map

![Route library](https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/screen-home.jpg)

## Getting routes

- **Import GPX** from anywhere. Routes without elevation (planner exports) get it filled in from open terrain data.
- **Find a route** — search for a place, click waypoints and get a route along bike-friendly roads with real elevation (via [BRouter](https://brouter.de)), or pick one of the famous climbs (Alpe d'Huez, Mont Ventoux, Stelvio, Galibier, Tourmalet, Mortirolo, Zoncolan, Box Hill). **Prefer: Mountain bike (trails)** plans along tracks and singletrack instead of roads.
- **Mountain-bike routes** — on **Find a route**, move the map to an area and press **Find MTB routes here** to list the waymarked mountain-bike routes mapped there (OpenStreetMap `route=mtb`), longest first. Pick one to see it on the map with its distance, climbing and elevation profile, then save it. Its mapped pieces are joined end to end; if some don't meet, you're told how many gaps there are, and you ride straight across them. The area can be up to about 60 km across.

![Route finder](https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/screen-explore.jpg)

## How the world is built

Everything comes from open data and is downloaded once per route, then kept on your computer.

- **Terrain** from open elevation tiles, out to the horizon. Near the road the ground is shaped to the route's own elevation so the road never floats or sinks.
- **Imagery on the ground** — real aerial photos in Switzerland, France, Spain, the Netherlands and the US, and Sentinel-2 satellite imagery everywhere else, downloaded as you ride and kept for next time. Sharp near you (about a metre a pixel), coarser further out; the distant hills take their colours from it too. Steep slopes keep their painted rock, as a photo from above can't show a cliff face. The credits for the imagery in view are at the bottom left. Turn it off, or use your own provider (a tile URL with `{z}`, `{x}` and `{y}` and your key, e.g. from Mapbox or MapTiler), under **Settings → Virtual world**.
- **Buildings** from OpenStreetMap, within about 350 m of the road — with mapped heights where the detailed source (Overpass) answers, otherwise estimated. They are built in the local style: Mediterranean terracotta and render, Alpine chalets with timber upper floors and deep eaves, British brick and slate, Scandinavian painted timber, North American siding, and so on, with barns, industrial halls, apartment blocks and churches each shaped their own way. Mapped roof shapes, colours and materials are followed.
- **Forests, lakes, rivers, the surrounding roads and railways, bridges and tunnels** from OpenStreetMap's vector tiles, about a kilometre either side. Where your route crosses a mapped bridge or tunnel, you ride over the deck or through the bore.
- **Your road itself** is drawn as a closed race road, set apart from the map's road network by crowd barriers. It takes the real road's character from OpenStreetMap tags:
  - **Surface** — tarmac, concrete, cobbles, gravel or dirt (`surface` tag);
  - **Width** — from `width` or `lanes`, else typical for the road class (a 7 m main road, a 5 m village street, a 3 m track);
  - **Markings** — a centre line on main roads only.

  When the tags can't be fetched, surface and width are guessed from the road class, and fetching is tried again a day later.

  **Trails** — an unpaved path or bridleway, a walking trail mapped as a footway (unpaved, or with a hiking grade), steps, or anything graded for mountain bikes (`mtb:scale` S0–S6, or `mtb:scale:imba`), is ridden as a trail: earth unless the map says otherwise, 1.5 m wide unless a width is mapped, no centre line and no crowd barriers. A narrow earth trail looks like singletrack — one worn line, with roots and stones across it — where a wider dirt track has two wheel ruts. Paved shared paths and town pavements are ridden like a lane.

  Where a trail has no mountain-bike grade, one is estimated from its hiking grade (`sac_scale`): hiking → S0, mountain hiking → S1, demanding mountain hiking → S2, alpine hiking → S3, harder → S4. Steps count as S2. The HUD shows **MTB trail · S2** for a mapped grade, **Trail · about S2** for an estimated one, or **Trail** when there's neither.

  **Berms** — tight turns on trails with a mapped mountain-bike grade are banked: the outside edge rises (up to 35°, and never more than a metre), the inside stays at ground level, and riders lean into the turn with it. A bend counts as tight from a radius of about 30 m, fully banked from 12 m — measured on the route as ridden, whose corners are rounded off a little. Walking paths and farm tracks stay flat.

  **Log bridges** — where a trail crosses a mapped bridge, it's a timber one: a plank deck barely wider than the trail, logs underneath, a wooden handrail on posts each side, and timber trestles under a longer span. Roads keep their concrete bridges.

  **Fords** — where a trail crosses a mapped stream with no bridge, it goes through the water: the trail dips about 30 cm, its bed is wet stones, and the stream runs on across it, shallow at the edges and deepest in the middle. Riding through is about as heavy as an S2 trail and held to 12 km/h for a few metres either side; the HUD shows **Ford**. Roads over streams keep their culverts.

  **Off the mapped roads** — where your route crosses a field, a beach or a trail nobody has mapped, for 100 m or more, you ride a 3 m dirt track. Shorter gaps (junctions, a GPS trace wandering off the road) keep the road either side, and where no map data could be loaded nothing is assumed: the road stays tarmac.

![The Arenberg cobbles](https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/ride-cobbles.jpg)

- **Landmarks** — village and town signs, named summit boards, and cafés, fountains, bike shops and viewpoints by the road.

Map data © OpenStreetMap contributors (ODbL). Downloads send the route's coordinates (or which map squares it passes through) to the OpenStreetMap services; buildings and scenery can be switched off in **Settings → Virtual world**.
