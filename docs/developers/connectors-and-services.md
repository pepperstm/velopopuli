# Connectors and other services

Everything the app talks to over the network besides map data (which is in [The world](the-world.md#the-inputs)). Every request goes through one shared `reqwest::Client` (`AppState::http`) with the app's own User-Agent.

| Service | What for | Code |
|---|---|---|
| Strava | Uploads, ghosts, segments, your routes | `connectors/strava.rs` |
| Dreeve (self-hosted) | Uploads, ghosts, rides as routes | `connectors/dreeve.rs` |
| A group-ride relay (self-hosted) | Riding together | `group.rs`, `crates/relay` |
| BRouter, Nominatim | Planning routes, finding places | `geo.rs` |
| Overpass | Waymarked routes (and map detail) | `geo.rs`, `mapdata.rs` |
| Open-Meteo | Live weather | `weather_now` in `commands.rs` |
| Mapillary | Street photos | `photos.rs` |
| Home Assistant, Shelly, Tasmota, a webhook | Fan and lights | `smart.rs` |
| GitHub | Filing a problem report | `issues.rs` |

## Connectors

A connector is a service a finished ride can be sent to, and maybe more. Each is one file in [`src-tauri/src/connectors`](../../src-tauri/src/connectors) implementing the `Connector` trait, registered once in `ALL`. Everything else is generic: the Settings screen draws a card from the connector's `ConnectorInfo`, the summary and history screens show an upload button per connected connector, and `commands.rs` routes by connector id.

| Method | Needed | What it does |
|---|---|---|
| `info` | yes | Name, description, set-up steps, fields to fill in |
| `status` | yes | Connected or not, and as whom |
| `configure` | yes | Saves the fields the rider filled in (a server address, a key) |
| `connect`, `disconnect` | yes | Signs in, or out |
| `upload` | yes | Sends a FIT file; returns an `UploadRecord` |
| `lists_rides`, `recent_rides`, `ride_track` | optional | The rider's rides, and a ride's GPS track for a ghost |
| `imports_routes`, `ride_gpx` | optional | A ride as a GPX, to add to the library |
| `finds_segments`, `segments_near` | optional | Segments within boxes along a route |
| `lists_routes`, `saved_routes`, `route_gpx` | optional | The rider's saved routes |

The optional parts default to "not supported", so a connector only writes what its service can do. [Adding a connector](../ADDING_A_CONNECTOR.md) is the walkthrough, with a test pattern against a fake server.

**Credentials** live in the operating system's keychain under the connector's id (see [Data and diagnostics](data-and-diagnostics.md#secrets)), never in the settings document, which goes into diagnostics reports.

**A server address** a rider types in goes through `secure_server_url`: it must be `https://`, or `http://` only to this machine (`localhost`, `127.0.0.1`, `[::1]`), so an API key is never sent unencrypted.

### Uploads

`UploadRecord { uploadedAt, reference, url, message }` is stored on the ride under the connector's id (`RideSummary.uploads`), so the history shows where each ride went and links to it. Rides from before connectors kept one Strava id (`strava_activity_id`). `RideSummary::upload("strava")` reads it as an upload record, and it's still written alongside `uploads`, so going back to 0.3.9 doesn't offer to upload the ride again (tested in `crates/core/tests/uploads.rs`). The ride list is changed under `AppState::rides_lock`, so two uploads finishing together can't lose one another's record.

### Strava

[`connectors/strava.rs`](../../src-tauri/src/connectors/strava.rs) implements every part of the trait.

- **Each rider makes their own API app.** Strava requires a client id and secret per application, and a secret can't be shipped in an open-source binary. So the rider registers a free app at strava.com/settings/api with the callback domain `localhost`, and pastes its id and secret in.
- **Sign-in** is OAuth through the system browser: the app listens on `127.0.0.1:53682`, opens Strava's authorise page (scopes `read,read_all,activity:read_all,activity:write`), and takes the code from the redirect, checking the `state` it sent. The refresh token and expiry are saved; access tokens are refreshed when needed. Private routes need `read_all`; if the rider didn't grant it, the route list says private routes are hidden (`private_hidden`). The scopes are always asked for in full, so a link made before routes were read gets the chance to add `read_all`. The rider has 5 minutes to finish signing in.
- **Upload:** `POST /api/v3/uploads` with the FIT file, as a Virtual Ride. Strava processes it in the background, so the app polls the upload every 2 s for up to a minute for the activity id.
- **Disconnect** deauthorises the token but keeps the client id and secret, so connecting again needs no setup.
- **Segments** for a route are asked for in boxes along it, then placed on the route (see [The ride](the-ride.md#climbs-and-segments)).

### Dreeve

[`connectors/dreeve.rs`](../../src-tauri/src/connectors/dreeve.rs): [Dreeve](https://github.com/dreeveapp/dreeve) is a self-hosted sports dashboard. The rider gives their server's address and an API key (`drv_` and 64 hex digits, made in Dreeve's admin panel).

- `GET /api/v1/status` checks the address and key (**Save and test**).
- `POST /api/v1/activity/upload` with the FIT file as a multipart `file`. The server queues it (`202 Accepted`) and imports it within minutes, so there's no activity page to link to; the record keeps the server's message ("File queued for import.").
- `GET /api/v1/activities` lists rides, and `GET /api/v1/activities/{id}/streams` gives a ride's track, for a ghost or a route.

## Group rides

Riders on the same **relay** with the same **room code** see each other on the road.

- **The relay** ([`crates/relay`](../../crates/relay)) is a small WebSocket server: `velopopuli-relay [address]`, by default `0.0.0.0:47800`. It's built by CI and attached to each release. Nothing is stored: a room disappears when its last rider leaves. Limits: 1,000 rooms, 50 riders a room, 8 MB a message.
- **The protocol** is JSON text messages with a type in `t`:

| Message | From | Meaning |
|---|---|---|
| `{"t":"join","room"}` | rider | Join a room (codes are letters, digits and dashes) |
| `{"t":"route","name","gpx","key"}` | rider | The route; the first rider's is kept for the room |
| `{"t":"welcome","id","riders"}` | relay | Joined, with this id, so many riders |
| `{"t":"pos","name","key","d","v","p"}` | rider → everyone | Distance (to 0.1 m), speed (m/s) and power; the relay adds the sender's `id` |
| `{"t":"left","id"}` | relay | A rider left |
| `{"t":"peek","room"}` | rider | Ask for the room's route before joining; the answer is the `route` message or `{"t":"noroom"}` |
| `{"t":"error","message"}` | relay | Refused: bad code, the relay or the room is full |

- **The client** ([`group.rs`](../../src-tauri/src/group.rs)): `group_fetch_route` peeks at a room and saves its route to the library. Joining is part of starting the ride (`room` in `RideRequest`). During a ride, this rider's position goes out every 500 ms, and positions from others become `Remote` rivals, each with a colour from a fixed set of eight.
- **The same route:** `key` is a fingerprint of the GPX text (FNV-1a). Positions are only used from riders whose key matches, so riders on different routes in one room don't appear on each other's road.

There's no clock sync or prediction: a rider is drawn where they last said they were, moved on at their speed (see [Drawing](drawing.md#the-3d-scene)).

## Finding places and planning routes

[`geo.rs`](../../src-tauri/src/geo.rs):

- **Place search:** [Nominatim](https://nominatim.openstreetmap.org), up to 8 results.
- **Planning:** [BRouter](https://brouter.de), one request per section between waypoints, each in its own profile, as GeoJSON with heights and way tags. Sections are kept in memory, so an edit only re-plans what changed. Loops use BRouter's round-trip mode. See [Routes](routes.md#planning-on-the-map).
- **Famous climbs** (`CLASSICS`): eight climbs as start and finish points, planned through BRouter.
- **Waymarked routes:** Overpass (see [Routes](routes.md#waymarked-routes)).

The page shows the map with [Leaflet](https://leafletjs.com) and OpenStreetMap's tiles.

## Live weather

`weather_now` asks [Open-Meteo](https://open-meteo.com) (no key) for the current WMO weather code at the route's centre. The page turns it into clear, cloudy, fog, rain or snow (see [Drawing](drawing.md#light-and-weather)). If it can't be had, the ride is a clear day.

## Street photos

[`photos.rs`](../../src-tauri/src/photos.rs): with **Street photos** on and the rider's own free [Mapillary](https://www.mapillary.com) client token, photos along the route are shown in a corner of the ride.

- Images are asked for in boxes along the route. `pick_photos` keeps those taken on the route and facing along it in the riding direction, at most one every 25 m, the newest winning.
- The picks are cached for 30 days as `photos-<route id>`.
- The image itself is fetched by Rust (`photo_image`), and only from Mapillary's own hosts over https.

## Fan and lights

[`smart.rs`](../../src-tauri/src/smart.rs) drives a fan from speed, power or heart rate, and a light in the colour of the current power zone.

- **Fan level:** 0–100 in steps of 25%, from off at `fan_from` to full at `fan_full` (`fan_level`).
- **Light:** the power zone's colour (seven zones, see [The ride](the-ride.md#fitness)), off when not pedalling.
- **Devices:**
  - **Home Assistant:** a `fan.*` entity gets a percentage; any other entity is switched on and off. A light entity gets an RGB colour. The access token is kept with the secrets (`smart_set_token`).
  - **Shelly** plugs (the Gen 2 API, then Gen 1).
  - **Tasmota** plugs, with a dimmer level for fan modules.
  - **A webhook** for anything else: a URL with `{level}`, `{zone}`, `{power}`, `{speed}` and `{hr}` filled in.
- Changes are sent at most every 8 s, and only when the level or zone changes. At the end of the ride the fan and light are turned off. **Test** (`smart_test`) runs the fan on full for a few seconds.

## Problem reports to GitHub

[`issues.rs`](../../src-tauri/src/issues.rs): **Report a problem** saves a diagnostics report (see [Data and diagnostics](data-and-diagnostics.md#diagnostics)), then opens GitHub's new-issue page with a title and the report's summary filled in. The rider signs in as usual and drags the saved file onto the page. Nothing is sent without them.
