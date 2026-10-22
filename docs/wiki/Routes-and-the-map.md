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
- **Buildings** from OpenStreetMap, within about 350 m of the road — with mapped heights where the detailed source (Overpass) answers, otherwise estimated.
- **Forests, lakes, rivers, the surrounding roads and railways, bridges and tunnels** from OpenStreetMap's vector tiles, about a kilometre either side. Where your route crosses a mapped bridge or tunnel, you ride over the deck or through the bore.
- **Your road itself** is drawn as a closed race road, set apart from the map's road network by crowd barriers. It takes the real road's character from OpenStreetMap tags:
  - **Surface** — tarmac, concrete, cobbles, gravel or dirt (`surface` tag);
  - **Width** — from `width` or `lanes`, else typical for the road class (a 7 m main road, a 5 m village street, a 3 m track);
  - **Markings** — a centre line on main roads only.

  When the tags can't be fetched, surface and width are guessed from the road class, and fetching is tried again a day later.

  **Trails** — an unpaved path or bridleway, a walking trail mapped as a footway (unpaved, or with a hiking grade), steps, or anything graded for mountain bikes (`mtb:scale` S0–S6, or `mtb:scale:imba`), is ridden as a trail: earth unless the map says otherwise, 1.5 m wide unless a width is mapped, no centre line and no crowd barriers. Paved shared paths and town pavements are ridden like a lane.

  Where a trail has no mountain-bike grade, one is estimated from its hiking grade (`sac_scale`): hiking → S0, mountain hiking → S1, demanding mountain hiking → S2, alpine hiking → S3, harder → S4. Steps count as S2. The HUD shows **MTB trail · S2** for a mapped grade, **Trail · about S2** for an estimated one, or **Trail** when there's neither.

  **Off the mapped roads** — where your route crosses a field, a beach or a trail nobody has mapped, for 100 m or more, you ride a 3 m dirt track. Shorter gaps (junctions, a GPS trace wandering off the road) keep the road either side, and where no map data could be loaded nothing is assumed: the road stays tarmac.

![The Arenberg cobbles](https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/ride-cobbles.jpg)

- **Landmarks** — village and town signs, named summit boards, and cafés, fountains, bike shops and viewpoints by the road.

Map data © OpenStreetMap contributors (ODbL). Downloads send the route's coordinates (or which map squares it passes through) to the OpenStreetMap services; buildings and scenery can be switched off in **Settings → Virtual world**.
