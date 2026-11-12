//! Connectors: the services a finished ride can be sent to (Strava today, Dreeve next).
//!
//! Each service is one file in this folder implementing [`Connector`], registered
//! once in [`ALL`]. Everything else is generic: the settings screen draws a card
//! from the connector's [`ConnectorInfo`], the summary and history screens show an
//! upload button per connected connector, and `commands.rs` routes by connector id.
//! See `docs/ADDING_A_CONNECTOR.md` for the walkthrough.
//!
//! A connector's credentials live in the OS keychain (or a user-only file on
//! Linux) under its id — see `secrets.rs` — and never in the settings document,
//! which goes into diagnostics reports.

pub mod dreeve;
pub mod strava;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;
use velopopuli_core::gpx::Route;
use velopopuli_core::segments::{outline_follow_distance, outline_follows_route};
use velopopuli_core::types::UploadRecord;

use crate::secrets;

/// Every connector there is. Adding a service means adding it here.
pub static ALL: &[&(dyn Connector + Sync)] = &[&strava::Strava, &dreeve::Dreeve];

pub fn get(id: &str) -> Result<&'static (dyn Connector + Sync), String> {
    ALL.iter().copied().find(|c| c.info().id == id).ok_or_else(|| format!("Unknown connector: {id}"))
}

/// What a connector needs from the app: where to keep credentials and an HTTP client.
#[derive(Clone)]
pub struct Ctx {
    pub dir: PathBuf,
    pub http: reqwest::Client,
}

impl Ctx {
    pub fn new(dir: &Path, http: &reqwest::Client) -> Self {
        Self { dir: dir.to_path_buf(), http: http.clone() }
    }

    /// The connector's saved credentials, or the default when none are saved yet.
    pub fn load_creds<T: DeserializeOwned + Default>(&self, connector: &str) -> T {
        secrets::load_named(&self.dir, connector).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save_creds<T: Serialize>(&self, connector: &str, creds: &T) -> Result<(), String> {
        secrets::save_named(&self.dir, connector, &serde_json::to_string(creds).map_err(|e| e.to_string())?)
    }
}

/// How the user links the service: by granting access in the browser, or by
/// pasting a key that is checked with a test request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    OAuth,
    Token,
}

/// What kind of input a configuration field wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Text,
    Url,
    /// Masked, never read back to the page; an empty value on save keeps the stored one.
    Secret,
}

/// One thing the user enters to set the connector up.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigField {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub kind: FieldKind,
}

/// One numbered instruction on the settings card, optionally with a link to open.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStep {
    pub text: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<&'static str>,
}

/// A connector described for the page, which builds the settings card from it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorInfo {
    /// Stable id, persisted in settings and ride records. Never change it.
    pub id: &'static str,
    pub name: &'static str,
    /// One line on what uploading to it does.
    pub description: &'static str,
    pub auth: AuthKind,
    pub fields: Vec<ConfigField>,
    /// Shown above the fields while the connector is not connected.
    pub setup: Vec<SetupStep>,
}

/// The connector's state for the page: whether it is set up, linked, and as whom.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorStatus {
    pub id: &'static str,
    /// Every required field has a value.
    pub configured: bool,
    /// Credentials are in place and uploads can be attempted.
    pub connected: bool,
    /// The account or server the connector is linked to, for display.
    pub account: String,
    /// Current values of the non-secret fields, so the form can show them.
    pub fields: BTreeMap<&'static str, String>,
}

/// One of the rider's past rides on a service, for picking one to race as a ghost.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRide {
    /// The service's own id, handed back to `ride_track`.
    pub id: String,
    pub name: String,
    /// Local start time as the service formats it (ISO 8601 where possible).
    pub start: String,
    pub distance: f64,
    pub moving_time: f64,
    /// The service's sport type, in Strava's vocabulary ("Ride", "GravelRide", …); empty if unknown.
    pub sport: String,
    /// The service's simplified outline of the ride as `(lat, lon)`, empty if it gives none.
    #[serde(skip)]
    pub outline: Vec<(f64, f64)>,
    /// Set by `find_rides`: the outline rides this route from its start for most of it.
    pub follows_route: bool,
    /// Set by `find_rides`: how far along the route the outline follows it, in metres.
    pub followed_m: f64,
    /// Set by `list_rides`: a few points of the outline as `(lat, lon)`, for drawing it small.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub preview: Vec<(f64, f64)>,
}

/// One page of `recent_rides`.
#[derive(Clone, Debug, Default)]
pub struct RidePage {
    pub rides: Vec<RemoteRide>,
    /// The service has older rides on the next page.
    pub more: bool,
}

/// What `find_rides` found: rides newest first, each marked whether it follows the route.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RideSearch {
    pub rides: Vec<RemoteRide>,
    /// The service has older rides that were not looked at.
    pub more: bool,
}

/// A ride's timed GPS track: `(unix seconds, lat, lon)` per point.
#[derive(Clone, Debug)]
pub struct RideTrack {
    pub name: String,
    pub points: Vec<(f64, f64, f64)>,
}

/// A segment a service knows near a route: its shape as `(lat, lon)` points.
#[derive(Clone, Debug)]
pub struct RemoteSegment {
    /// The service's own id.
    pub id: String,
    pub name: String,
    pub points: Vec<(f64, f64)>,
    /// Length in metres.
    pub distance: f64,
    /// Average grade as a fraction (0.05 = 5 %).
    pub avg_grade: f64,
    /// The fastest known time, as the service formats it.
    pub record: Option<String>,
}

/// One of the rider's saved routes on a service, for adding to the library.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRoute {
    /// The service's own id, handed back to `route_gpx`. Kept as text: Strava's are too big for
    /// a JavaScript number.
    pub id: String,
    pub name: String,
    /// Length in metres.
    pub distance: f64,
    /// Climbing in metres.
    pub ascent: f64,
    /// Only the rider can see it on the service.
    pub private: bool,
    /// The rider starred it; listed first.
    pub starred: bool,
}

/// What `saved_routes` found.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteList {
    pub routes: Vec<RemoteRoute>,
    /// The link to the service doesn't allow reading private routes, so any are left out.
    /// Connecting again asks for that permission.
    pub private_hidden: bool,
}

/// A ride to send: the FIT file this app recorded plus a title and a description.
pub struct UploadRequest {
    pub file: PathBuf,
    pub name: String,
    pub description: String,
}

/// Shows a page to the user in the system browser (OAuth consent).
pub type OpenBrowser = Box<dyn Fn(&str) + Send + Sync + 'static>;

/// A service rides can be uploaded to.
///
/// Implementations are stateless unit structs; everything per user comes from
/// `Ctx`. Errors are plain sentences shown to the user as they are.
#[async_trait]
pub trait Connector: Send + Sync {
    fn info(&self) -> ConnectorInfo;

    fn status(&self, ctx: &Ctx) -> ConnectorStatus;

    /// Stores what the user typed into the fields. Secrets left empty keep their saved value.
    /// Saving new credentials unlinks the connector; `connect` links it again.
    fn configure(&self, ctx: &Ctx, fields: &BTreeMap<String, String>) -> Result<ConnectorStatus, String>;

    /// Links the service: runs the OAuth flow (handing the page to open to `open_browser`)
    /// or checks the token with a test request.
    async fn connect(&self, ctx: &Ctx, open_browser: OpenBrowser) -> Result<ConnectorStatus, String>;

    /// Unlinks the service, keeping the user's configuration so they can connect again.
    async fn disconnect(&self, ctx: &Ctx) -> Result<ConnectorStatus, String>;

    async fn upload(&self, ctx: &Ctx, req: &UploadRequest) -> Result<UploadRecord, String>;

    // ── Optional: browsing the rider's past rides on the service ─────────────

    /// True when `recent_rides` and `ride_track` work; the ride setup then offers "From <name>…".
    fn lists_rides(&self) -> bool {
        false
    }

    /// The rider's rides with a GPS track, newest first, one page at a time (`page` from 1).
    /// Fill in `outline` when the listing carries one, so rides can be matched to a route.
    async fn recent_rides(&self, _ctx: &Ctx, _page: u32) -> Result<RidePage, String> {
        Err(format!("{} can't list your rides.", self.info().name))
    }

    /// One ride's track, by the id `recent_rides` gave.
    async fn ride_track(&self, _ctx: &Ctx, _id: &str) -> Result<RideTrack, String> {
        Err(format!("{} can't fetch a ride.", self.info().name))
    }

    // ── Optional: turning one of those rides into a route ────────────────────

    /// True when `ride_gpx` works; the routes page then offers "Import from <name>…".
    /// Needs `lists_rides` too, which is where the rider picks the ride.
    fn imports_routes(&self) -> bool {
        false
    }

    /// One ride as a GPX file with elevation, by the id `recent_rides` gave.
    async fn ride_gpx(&self, _ctx: &Ctx, _id: &str) -> Result<String, String> {
        Err(format!("{} can't export a ride as a route.", self.info().name))
    }

    // ── Optional: segments the service knows along a route ───────────────────

    /// True when `segments_near` works; the ride setup then offers "Find <name> segments".
    fn finds_segments(&self) -> bool {
        false
    }

    /// Segments in the given areas, each `[south, west, north, east]` in degrees.
    /// The caller places them on the route and times the ones it follows end to end.
    async fn segments_near(&self, _ctx: &Ctx, _boxes: &[[f64; 4]]) -> Result<Vec<RemoteSegment>, String> {
        Err(format!("{} can't find segments.", self.info().name))
    }

    // ── Optional: the rider's saved routes on the service ────────────────────

    /// True when `saved_routes` and `route_gpx` work; Find a route then offers "Your <name> routes".
    fn lists_routes(&self) -> bool {
        false
    }

    /// The routes the rider has made or saved on the service, starred ones first.
    async fn saved_routes(&self, _ctx: &Ctx) -> Result<RouteList, String> {
        Err(format!("{} can't list your routes.", self.info().name))
    }

    /// One route as a GPX file, by the id `saved_routes` gave.
    async fn route_gpx(&self, _ctx: &Ctx, _id: &str) -> Result<String, String> {
        Err(format!("{} can't fetch a route.", self.info().name))
    }
}

/// Enough matching rides to choose from; stop paging once there are this many.
const RIDES_WANTED: usize = 5;
/// Pages to look through at most: a few hundred rides on most services.
const MAX_PAGES: u32 = 5;

/// Pages through a connector's rides, marks each one whose outline follows `route`, and stops
/// when enough follow it, the service has no more, or `MAX_PAGES` were read.
pub async fn find_rides(connector: &(dyn Connector + Sync), ctx: &Ctx, route: &Route) -> Result<RideSearch, String> {
    let mut found = RideSearch::default();
    let mut matching = 0;
    for page in 1..=MAX_PAGES {
        let mut got = connector.recent_rides(ctx, page).await?;
        for ride in &mut got.rides {
            ride.followed_m = outline_follow_distance(route, &ride.outline);
            ride.follows_route = outline_follows_route(route, &ride.outline);
            matching += usize::from(ride.follows_route);
        }
        found.rides.append(&mut got.rides);
        found.more = got.more;
        if !got.more || matching >= RIDES_WANTED {
            break;
        }
    }
    Ok(found)
}

/// Pages to read at most when listing every ride to import one as a route.
const MAX_LIST_PAGES: u32 = 20;
/// Points kept of an outline for the small preview drawing.
const PREVIEW_POINTS: usize = 60;

/// Every ride the connector lists, newest first, up to `MAX_LIST_PAGES` pages, each with a preview.
pub async fn list_rides(connector: &(dyn Connector + Sync), ctx: &Ctx) -> Result<RideSearch, String> {
    let mut found = RideSearch::default();
    for page in 1..=MAX_LIST_PAGES {
        let mut got = connector.recent_rides(ctx, page).await?;
        for ride in &mut got.rides {
            ride.preview = thin(&ride.outline, PREVIEW_POINTS);
        }
        found.rides.append(&mut got.rides);
        found.more = got.more;
        if !got.more {
            break;
        }
    }
    Ok(found)
}

/// At most `n` points of `points`, evenly spaced, keeping the first and the last.
fn thin(points: &[(f64, f64)], n: usize) -> Vec<(f64, f64)> {
    if points.len() <= n || n < 2 {
        return points.to_vec();
    }
    (0..n).map(|i| points[i * (points.len() - 1) / (n - 1)]).collect()
}

/// Reads a field, trimmed; missing is the empty string.
pub fn field(fields: &BTreeMap<String, String>, key: &str) -> String {
    fields.get(key).map(|v| v.trim().to_string()).unwrap_or_default()
}

/// Checks a server URL typed by the user before an API key is sent to it: https only, so the key
/// isn't sent unencrypted. Plain http is allowed for this machine (`localhost`, `127.0.0.1`, `::1`),
/// which is where a service under development runs. Returns the URL without a trailing slash.
pub fn secure_server_url(raw: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| "That server address isn't a valid URL. Use the form https://example.org.".to_string())?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        "http" => return Err("The server address must start with https://, so your API key isn't sent unencrypted.".into()),
        _ => return Err("The server address must start with https://.".into()),
    }
    if url.host_str().is_none() {
        return Err("That server address has no host name.".into());
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_keys_only_go_over_https_or_to_this_machine() {
        assert_eq!(secure_server_url(" https://dreeve.example.org/ ").unwrap(), "https://dreeve.example.org");
        assert!(secure_server_url("http://dreeve.example.org").is_err());
        assert!(secure_server_url("HTTP://192.168.1.5:8080").is_err());
        assert!(secure_server_url("ftp://example.org").is_err());
        assert!(secure_server_url("dreeve.example.org").is_err());
        assert!(secure_server_url("http://localhost:3000").is_ok());
        assert!(secure_server_url("http://127.0.0.1:3000/").is_ok());
        assert!(secure_server_url("http://[::1]:3000").is_ok());
        // A look-alike host name isn't local.
        assert!(secure_server_url("http://localhost.evil.example").is_err());
    }

    #[test]
    fn ids_are_unique_and_resolvable() {
        let mut ids: Vec<&str> = ALL.iter().map(|c| c.info().id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "connector ids must be unique");
        for id in ids {
            assert_eq!(get(id).unwrap().info().id, id);
        }
        assert!(get("nope").is_err());
    }

    #[tokio::test]
    async fn connectors_without_an_optional_feature_say_so() {
        let ctx = Ctx::new(Path::new("/nonexistent"), &reqwest::Client::new());
        for c in ALL.iter().filter(|c| !c.lists_rides()) {
            let err = c.recent_rides(&ctx, 1).await.unwrap_err();
            assert!(err.contains(c.info().name), "{err}");
            assert!(c.ride_track(&ctx, "1").await.is_err());
        }
        for c in ALL.iter().filter(|c| !c.imports_routes()) {
            let err = c.ride_gpx(&ctx, "1").await.unwrap_err();
            assert!(err.contains(c.info().name), "{err}");
        }
        for c in ALL.iter().filter(|c| c.imports_routes()) {
            assert!(c.lists_rides(), "{}: importing a route needs the ride list", c.info().id);
        }
        for c in ALL.iter().filter(|c| !c.finds_segments()) {
            let err = c.segments_near(&ctx, &[[0.0, 0.0, 1.0, 1.0]]).await.unwrap_err();
            assert!(err.contains(c.info().name), "{err}");
        }
    }

    /// Serves `pages` of rides; each ride's outline either follows the test route or lies elsewhere.
    struct FakeRides {
        pages: Vec<Vec<bool>>,
        route: Route,
        asked: std::sync::Mutex<Vec<u32>>,
    }

    #[async_trait]
    impl Connector for FakeRides {
        fn info(&self) -> ConnectorInfo {
            ConnectorInfo { id: "fake", name: "Fake", description: "Test.", auth: AuthKind::Token, fields: vec![], setup: vec![] }
        }
        fn status(&self, _ctx: &Ctx) -> ConnectorStatus {
            ConnectorStatus::default()
        }
        fn configure(&self, _ctx: &Ctx, _fields: &BTreeMap<String, String>) -> Result<ConnectorStatus, String> {
            unimplemented!()
        }
        async fn connect(&self, _ctx: &Ctx, _open_browser: OpenBrowser) -> Result<ConnectorStatus, String> {
            unimplemented!()
        }
        async fn disconnect(&self, _ctx: &Ctx) -> Result<ConnectorStatus, String> {
            unimplemented!()
        }
        async fn upload(&self, _ctx: &Ctx, _req: &UploadRequest) -> Result<UploadRecord, String> {
            unimplemented!()
        }
        fn lists_rides(&self) -> bool {
            true
        }
        async fn recent_rides(&self, _ctx: &Ctx, page: u32) -> Result<RidePage, String> {
            self.asked.lock().unwrap().push(page);
            let on_route: Vec<(f64, f64)> = [0.0, 500.0, 1200.0, 1900.0].iter().map(|&d| self.route.sample_at(d)).map(|p| (p.lat, p.lon)).collect();
            let elsewhere = vec![(48.0, 11.0), (48.01, 11.01)];
            let rides = self.pages[page as usize - 1]
                .iter()
                .enumerate()
                .map(|(i, &follows)| RemoteRide { id: format!("{page}-{i}"), outline: if follows { on_route.clone() } else { elsewhere.clone() }, ..Default::default() })
                .collect();
            Ok(RidePage { rides, more: (page as usize) < self.pages.len() })
        }
    }

    fn straight_route() -> Route {
        use velopopuli_core::gpx::{build_route, RawPoint, M_PER_DEG};
        let pts: Vec<RawPoint> = (0..=200).map(|i| RawPoint { lat: 45.0 + i as f64 * 10.0 / M_PER_DEG, lon: 6.0, ele: Some(100.0) }).collect();
        build_route("S", &pts).unwrap()
    }

    #[tokio::test]
    async fn find_rides_pages_until_enough_follow_the_route() {
        let ctx = Ctx::new(Path::new("/nonexistent"), &reqwest::Client::new());
        let route = straight_route();
        // Two matches on page 1, three on page 3: enough after page 3, page 4 is never read.
        let pages = vec![vec![true, false, true], vec![false, false], vec![true, true, false, true], vec![true]];
        let fake = FakeRides { pages, route: route.clone(), asked: Default::default() };
        let found = find_rides(&fake, &ctx, &route).await.unwrap();
        assert_eq!(*fake.asked.lock().unwrap(), [1, 2, 3]);
        assert_eq!(found.rides.len(), 9);
        assert_eq!(found.rides.iter().filter(|r| r.follows_route).count(), 5);
        assert!(found.rides[0].follows_route && !found.rides[1].follows_route);
        assert!(found.more);

        // A service with fewer rides than that: every page, then stop.
        let fake = FakeRides { pages: vec![vec![false], vec![true]], route: route.clone(), asked: Default::default() };
        let found = find_rides(&fake, &ctx, &route).await.unwrap();
        assert_eq!(*fake.asked.lock().unwrap(), [1, 2]);
        assert!(!found.more);

        // Never more than MAX_PAGES.
        let fake = FakeRides { pages: vec![vec![false]; 9], route: route.clone(), asked: Default::default() };
        let found = find_rides(&fake, &ctx, &route).await.unwrap();
        assert_eq!(fake.asked.lock().unwrap().len(), MAX_PAGES as usize);
        assert!(found.more);
    }

    #[tokio::test]
    async fn list_rides_reads_every_page_and_thins_the_outline() {
        let ctx = Ctx::new(Path::new("/nonexistent"), &reqwest::Client::new());
        let route = straight_route();
        let fake = FakeRides { pages: vec![vec![true, false], vec![true], vec![false]], route, asked: Default::default() };
        let found = list_rides(&fake, &ctx).await.unwrap();
        assert_eq!(*fake.asked.lock().unwrap(), [1, 2, 3]);
        assert_eq!(found.rides.len(), 4);
        assert!(!found.more);
        assert_eq!(found.rides[0].preview.len(), 4, "short outlines are kept whole");

        let long: Vec<(f64, f64)> = (0..1000).map(|i| (i as f64, 0.0)).collect();
        let thinned = thin(&long, PREVIEW_POINTS);
        assert_eq!(thinned.len(), PREVIEW_POINTS);
        assert_eq!((thinned[0], thinned[PREVIEW_POINTS - 1]), (long[0], long[999]));
    }

    #[test]
    fn every_connector_describes_its_fields() {
        for c in ALL {
            let info = c.info();
            assert!(!info.name.is_empty() && !info.description.is_empty(), "{}", info.id);
            for f in &info.fields {
                assert!(!f.key.is_empty() && !f.label.is_empty(), "{}: field without key/label", info.id);
            }
        }
    }
}
