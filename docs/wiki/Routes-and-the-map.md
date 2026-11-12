# Routes and the map

![Your routes](https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/screen-home.jpg)

**Routes** is your library. Every route you add stays here, with its distance, climbing and your best time. Press **Ride** on a card to set up a ride, or **Remove** to delete it.

Two routes are always there: the **Demo Loop**, and **Herne Hill Velodrome** (see [below](#herne-hill-velodrome)).

## Adding a route

### Import a GPX file

Press **Import GPX…**, or drag files onto the window. Any GPX works: exports from Strava, Komoot, RideWithGPS, Garmin Connect or a bike computer. If the file has no heights (many planners leave them out), they're filled in for you.

### Plan one on the map

Open **Find a route**.

1. Search for a place, or move the map there.
2. Click to add waypoints from start to finish. Drag one to move it; click it to remove it.
3. The route follows bike-friendly roads between them, with real heights. **Prefer** picks the kind of roads: **Road bike (fast roads)**, **Quiet roads and lanes**, **Safest (avoid traffic)**, or **Mountain bike (trails)** for tracks and singletrack.
4. Give it a name and press **Save to library**.

**Undo**, **Reverse** and **Clear** do what they say.

![Find a route](https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/screen-explore.jpg)

### Pick a ready-made route

Also on **Find a route**:

- **Your Strava routes.** With [[Strava]] connected, press **Show my Strava routes** to list the routes you've made or starred there, starred ones first. Pick one to see it on the map with its climbing and profile, then press **Save to library**. If you connected Strava before version 0.3.10, only your public routes are listed: connect again in Settings to see your private ones too.
- **Signposted road routes.** Move the map to an area and press **Find road routes here**. You get the signposted cycle routes there: local loops, regional routes and national ones. A long national route is cut down to the part on the map.
- **Mountain-bike routes.** **Find MTB routes here** does the same for waymarked mountain-bike trails. If a trail's pieces don't quite join up, you're told, and you ride straight across the gaps.
- **Famous climbs.** Alpe d'Huez, Mont Ventoux, the Stelvio, Galibier, Tourmalet, Mortirolo, Zoncolan and Box Hill, ready to ride.

The area searched can be up to about 60 km across: if the app asks you to zoom in a little, do that and search again.

### Ride one of your Dreeve rides again

With [[Dreeve]] connected, **Import from Dreeve…** on the Routes page lists the rides you've done. Search by name or date, or filter by bike and distance, then pick one to turn it into a route. It needs Dreeve 5.4 or later.

## Herne Hill Velodrome

The outdoor track in south London where the 1948 Olympic track races were held, built into the app. It's 450 m round, banked steeply into the bends, with the pavilion, the stands and the floodlights around it.

When you set up a ride there, choose the **Direction**: anticlockwise, as they race it, or clockwise. Pick a number of **Laps** too (see [[Riding#laps]]).

![The home straight at Herne Hill](https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/ride-herne-hill.jpg)

## The world around your route

The first time you ride a route, the app downloads the map and the landscape for it. That takes from a few seconds to a minute. After that it's kept on your computer, and the route starts straight away, even offline.

What you ride through is built from real map data:

- **The landscape** is the real one, out to the horizon, so the mountains you see are the ones that are there.
- **Towns and villages** have their buildings, and there are village signs, summit boards, cafés, fountains and bike shops by the road.
- **Woods, lakes, rivers, railways and other roads** are where the map puts them. You ride over bridges and through tunnels where the real road does.
- **Fields** have hedges, stone walls or fences, in the style of the region. In hedge country, such as Devon, narrow lanes run between high hedge banks.

![A Devon lane between hedge banks](https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/ride-lane.jpg)

### The road you ride

Your route is a closed road, lined with crowd barriers like a race. It has the real road's surface and width:

- **Surface:** tarmac, concrete, cobbles, gravel or dirt. You can feel it: rougher surfaces are slower and make the trainer harder (see [[Riding#the-road-surface]]).
- **Width:** a wide main road, a village street or a narrow lane, as it is on the map.
- **Off-road:** paths, bridleways and mountain-bike trails become narrow earth trails without barriers. The HUD shows a trail's difficulty grade, from S0 (easy) to S5 and up, where the map has one. Tight turns on graded trails are banked. Where a trail crosses a stream with no bridge, you ride through the ford.

If a road's surface or width is wrong, it's usually wrong on the map. See the [[FAQ]].

![The Arenberg cobbles](https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/ride-cobbles.jpg)

## Privacy and turning things off

To build the world, the app sends the route's position to the OpenStreetMap services. Nothing else is sent. If you'd rather keep things simple, or your computer is slow, you can turn buildings and scenery off under **Settings → Virtual world**.

Map data © OpenStreetMap contributors.
