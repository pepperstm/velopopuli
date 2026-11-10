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

pub mod strava;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;
use velopopuli_core::types::UploadRecord;

use crate::secrets;

/// Every connector there is. Adding a service means adding it here.
pub static ALL: &[&(dyn Connector + Sync)] = &[&strava::Strava];

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
#[allow(dead_code)] // `Token` is for key-based services such as Dreeve.
pub enum AuthKind {
    OAuth,
    Token,
}

/// What kind of input a configuration field wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)] // `Url` is for self-hosted services such as Dreeve.
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
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRide {
    /// The service's own id, handed back to `ride_track`.
    pub id: String,
    pub name: String,
    /// Local start time as the service formats it (ISO 8601 where possible).
    pub start: String,
    pub distance: f64,
    pub moving_time: f64,
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

    /// The rider's recent rides with a GPS track, newest first.
    async fn recent_rides(&self, _ctx: &Ctx) -> Result<Vec<RemoteRide>, String> {
        Err(format!("{} can't list your rides.", self.info().name))
    }

    /// One ride's track, by the id `recent_rides` gave.
    async fn ride_track(&self, _ctx: &Ctx, _id: &str) -> Result<RideTrack, String> {
        Err(format!("{} can't fetch a ride.", self.info().name))
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
}

/// Reads a field, trimmed; missing is the empty string.
pub fn field(fields: &BTreeMap<String, String>, key: &str) -> String {
    fields.get(key).map(|v| v.trim().to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let err = c.recent_rides(&ctx).await.unwrap_err();
            assert!(err.contains(c.info().name), "{err}");
            assert!(c.ride_track(&ctx, "1").await.is_err());
        }
        for c in ALL.iter().filter(|c| !c.finds_segments()) {
            let err = c.segments_near(&ctx, &[[0.0, 0.0, 1.0, 1.0]]).await.unwrap_err();
            assert!(err.contains(c.info().name), "{err}");
        }
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
