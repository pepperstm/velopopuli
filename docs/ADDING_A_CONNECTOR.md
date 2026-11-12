# Adding a connector

A *connector* is a service a finished ride can be sent to. Strava (OAuth) and
Dreeve (API key) are the two there are. Each is one Rust file in `src-tauri/src/connectors/`, registered in one
place. The page (settings card, upload buttons, auto-upload toggle, the record of
where a ride went) is generic and needs no change for a new service.

## The pieces

| Where | What |
|---|---|
| `src-tauri/src/connectors/mod.rs` | The `Connector` trait, the descriptor types the page draws from, the `ALL` registry, `Ctx` (credential storage + HTTP client) |
| `src-tauri/src/connectors/<id>.rs` | One file per service: a unit struct implementing `Connector` |
| `src-tauri/src/commands.rs`, *Connectors* section | Generic commands routed by connector id: `connectors_list`, `connector_status`, `connector_configure`, `connector_connect`, `connector_disconnect`, `connector_upload`, `connector_rides`, `ghost_import_connector`, `segments_fetch`; for the optional route features `connector_routes`, `connector_route` (saved routes) and `connector_ride_list`, `route_import_connector` (ride as route) |
| `crates/core/src/types.rs` | `UploadRecord` (what a connector returns, kept per ride under `RideSummary.uploads[id]`), `Settings.auto_upload` (ids to send every ride to) |
| `web/src/ui/Settings.tsx` `ConnectorCard` | Builds the card from `ConnectorInfo`: setup steps, one input per field, connect/disconnect, the auto-upload toggle |
| `web/src/ui/Summary.tsx` `UploadButtons` | One button per connected connector on the summary and history screens |
| `web/src/ui/Explore.tsx` | "Your <name> routes" in Find a route, for connectors with `lists_routes` |
| `web/src/ui/Home.tsx`, `web/src/ui/RemoteImport.tsx` | "Import from <name>…" on the routes page and its dialog, for connectors with `imports_routes` |
| `web/src/ui/RideSetup.tsx` | "From <name>…" ghost import and "Find <name> segments" for every connected connector that offers them |

## Steps

1. **Create `src-tauri/src/connectors/<id>.rs`.** Sketch for a self-hosted service
   that takes a URL and an API key:

   ```rust
   use std::collections::BTreeMap;
   use async_trait::async_trait;
   use serde::{Deserialize, Serialize};
   use velopopuli_core::types::UploadRecord;
   use super::{field, AuthKind, ConfigField, Connector, ConnectorInfo, ConnectorStatus, Ctx, FieldKind, OpenBrowser, SetupStep, UploadRequest};
   use crate::now_ms;

   pub const ID: &str = "example"; // persisted in settings and rides: never change it
   pub struct Example;

   #[derive(Default, Serialize, Deserialize)]
   struct Creds { url: String, api_key: String, verified: bool, server: String }

   fn status(c: &Creds) -> ConnectorStatus {
       ConnectorStatus {
           id: ID,
           configured: !c.url.is_empty() && !c.api_key.is_empty(),
           connected: c.verified,
           account: c.server.clone(),
           fields: BTreeMap::from([("url", c.url.clone())]), // never the secret
       }
   }

   #[async_trait]
   impl Connector for Example {
       fn info(&self) -> ConnectorInfo {
           ConnectorInfo {
               id: ID,
               name: "Example",
               description: "Rides are queued on your server and imported within minutes.",
               auth: AuthKind::Token,
               fields: vec![
                   ConfigField { key: "url", label: "Server URL", hint: "https://example.org", kind: FieldKind::Url },
                   ConfigField { key: "apiKey", label: "API key", hint: "", kind: FieldKind::Secret },
               ],
               setup: vec![SetupStep { text: "Create an API key in the admin panel.", link: None }],
           }
       }

       fn status(&self, ctx: &Ctx) -> ConnectorStatus { status(&ctx.load_creds::<Creds>(ID)) }

       fn configure(&self, ctx: &Ctx, fields: &BTreeMap<String, String>) -> Result<ConnectorStatus, String> {
           let prev: Creds = ctx.load_creds(ID);
           let key = field(fields, "apiKey");
           let next = Creds {
               url: connectors::secure_server_url(&field(fields, "url"))?, // https only: the key goes to this URL
               api_key: if key.is_empty() { prev.api_key } else { key }, // blank keeps the saved secret
               ..Default::default()                                        // new credentials: not verified yet
           };
           ctx.save_creds(ID, &next)?;
           Ok(status(&next))
       }

       async fn connect(&self, ctx: &Ctx, _open_browser: OpenBrowser) -> Result<ConnectorStatus, String> {
           // Token services: the cheapest request that proves URL and key work, then mark verified.
           let mut creds: Creds = ctx.load_creds(ID);
           let res = ctx.http.get(format!("{}/api/status", creds.url)).bearer_auth(&creds.api_key).send().await
               .map_err(|e| format!("Could not reach the server: {e}"))?;
           if !res.status().is_success() { return Err(format!("The server refused the key ({}).", res.status().as_u16())); }
           creds.verified = true;
           ctx.save_creds(ID, &creds)?;
           Ok(status(&creds))
       }

       async fn disconnect(&self, ctx: &Ctx) -> Result<ConnectorStatus, String> {
           let creds = Creds { verified: false, ..ctx.load_creds(ID) }; // keep URL and key so reconnecting is one click
           ctx.save_creds(ID, &creds)?;
           Ok(status(&creds))
       }

       async fn upload(&self, ctx: &Ctx, req: &UploadRequest) -> Result<UploadRecord, String> {
           // Send req.file (a FIT file this app recorded); req.name / req.description are the title and text.
           Ok(UploadRecord { uploaded_at: now_ms(), reference: None, url: None, message: Some("Queued for import.".into()) })
       }
   }
   ```

   `dreeve.rs` is this shape, with tests against a fake HTTP server to copy.
   `AuthKind::OAuth` connectors (see `strava.rs`) get `open_browser` to show the
   consent page and listen on a loopback port for the redirect. The page shows
   "Connect to X" / "Waiting for X in your browser…" for OAuth and "Save and
   test" / "Checking X…" for tokens.

2. **Register it** in `connectors/mod.rs`:

   ```rust
   pub mod example;
   pub static ALL: &[&(dyn Connector + Sync)] = &[&strava::Strava, &example::Example];
   ```

   That is the only place the app enumerates connectors.

3. **Optional CSS.** Upload buttons get the classes `upload <id>`; add a
   `button.<id>` rule in `web/src/styles.css` for a branded colour: define
   `--<id>` next to `--strava` and `--dreeve` in `:root` and use it for the
   background and border (Strava and Dreeve have one each).

## Rules of the road

- `id` is persisted in `Settings.auto_upload` and `RideSummary.uploads`. Changing
  it orphans users' settings and upload records.
- Credentials go through `ctx.load_creds` / `ctx.save_creds` (keychain, or a
  user-only file on Linux) under the connector id. Never put them in the
  settings document: it is included in diagnostics reports.
- `ConnectorStatus.fields` is sent to the page to prefill the form. Put the
  non-secret values there; never the secret. `FieldKind::Secret` inputs show
  "saved — leave blank to keep" once `configured` is true, and `configure`
  must treat an empty secret as "keep".
- Errors are `String`s shown to the user as they are: write a sentence that
  says what to do ("Reconnect Strava in Settings…"), not a status code.
- `upload` returns an `UploadRecord`: `url` when the service has a page for the
  activity (the button becomes "View on X ↗"), otherwise `message` for what
  happened ("queued for import"). The app records it against the ride so the
  ride is not sent to that connector twice.
- Only files under the app's `activities` folder can be uploaded; `commands.rs`
  checks that before calling the connector.
- **Listing the rider's rides is optional.** Override `lists_rides` to return
  `true` and implement `recent_rides(ctx, page)` (one page, newest first, only
  rides with a GPS track, ids as strings, `more` when an older page exists) and
  `ride_track` (the timed track for one id). Fill in each ride's `outline` from
  the listing's summary polyline: `connectors::find_rides` uses it to mark the
  rides that ride the chosen route from its start, and pages back until a few
  do (at most five pages). The ride setup then shows "From <name>…" with the
  fitting rides first. The defaults say the service can't list rides. Set each
  ride's `sport` to the service's sport type in Strava's vocabulary ("Ride",
  "GravelRide", …) so the rider can filter by bike.
- **Saved routes are optional.** Override `lists_routes` to return `true` and
  implement `saved_routes` (the routes the rider planned or starred, starred
  first) and `route_gpx` (one of them as GPX, by the id `saved_routes` gave;
  keep ids as text). Find a route then shows "Your <name> routes". Strava does
  this; a service that only holds finished rides does not.
- **Importing a ride as a route is optional** and builds on listing rides.
  Override `imports_routes` to return `true` and implement `ride_gpx`, which
  returns one ride as GPX text with elevation. The routes page then shows
  "Import from <name>…": a dialog listing every ride (`connectors::list_rides`,
  up to twenty pages) with search, a bike filter and a distance range.
  Don't confuse the two: `lists_routes` offers routes the rider *planned*
  (Find a route); `imports_routes` turns a ride the rider *has ridden* into a
  route (the routes page). Both end in a GPX file in the library. Dreeve sets
  only `imports_routes`, Strava only `lists_routes`.
- **Finding segments is optional too.** Override `finds_segments` to return
  `true` and implement `segments_near`, which gets `[south, west, north, east]`
  boxes along the route and returns each segment's shape as `(lat, lon)` points
  (decode the service's polyline yourself). The app places them on the route,
  keeps those the route rides end to end under the id `<connector>-<id>` with
  `SegmentSource::Remote` and `connector = <id>`, times them live and awards
  segment bests. The ride setup shows "Find <name> segments" per connector and
  merges the results of several services.
- `cargo test -p velopopuli connectors` checks ids are unique and every
  connector describes its fields.
