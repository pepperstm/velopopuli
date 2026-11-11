//! Dreeve (formerly "Statistics for Strava"), a self-hosted sports dashboard:
//! https://github.com/dreeveapp/dreeve
//!
//! API: bearer token `drv_<64 hex>` (generated in the admin panel and set as
//! `DREEVE_API_KEY` on the server), `GET /api/v1/status` to verify the URL and key,
//! `POST /api/v1/activity/upload` with a multipart `file` part. Uploads are queued
//! and answered with `202 Accepted`; the server imports them within minutes, so
//! there is no activity page to link to. `GET /api/v1/activities` lists rides and
//! `GET /api/v1/activities/{id}/streams` gives a ride's GPS track for racing it as a ghost.

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use velopopuli_core::types::UploadRecord;

use super::{field, AuthKind, ConfigField, Connector, ConnectorInfo, ConnectorStatus, Ctx, FieldKind, OpenBrowser, RemoteRide, RidePage, RideTrack, SetupStep, UploadRequest};
use crate::now_ms;
use velopopuli_core::segments::decode_polyline;

pub const ID: &str = "dreeve";
const STATUS_PATH: &str = "/api/v1/status";
const UPLOAD_PATH: &str = "/api/v1/activity/upload";
const ACTIVITIES_PATH: &str = "/api/v1/activities";
/// Dreeve's cycling sport types (Strava's vocabulary); the others are runs, swims, gym sessions.
const RIDE_TYPES: &[&str] = &["Ride", "MountainBikeRide", "GravelRide", "EBikeRide", "EMountainBikeRide", "VirtualRide", "Velomobile", "Handcycle"];

pub struct Dreeve;

#[derive(Default, Serialize, Deserialize)]
struct Creds {
    url: String,
    api_key: String,
    /// The status call succeeded with these credentials.
    #[serde(default)]
    verified: bool,
    /// "Dreeve 1.2.3", from the status call.
    #[serde(default)]
    server: String,
}

fn load(ctx: &Ctx) -> Creds {
    ctx.load_creds(ID)
}

fn to_status(c: &Creds) -> ConnectorStatus {
    ConnectorStatus { id: ID, configured: !c.url.is_empty() && !c.api_key.is_empty(), connected: c.verified, account: c.server.clone(), fields: BTreeMap::from([("url", c.url.clone())]) }
}

/// The instance URL without a trailing slash, or why it can't be used.
fn normalise_url(url: &str) -> Result<String, String> {
    if url.trim().is_empty() {
        return Err("Enter your Dreeve URL.".into());
    }
    // https only (this machine excepted), so the API key isn't sent unencrypted.
    super::secure_server_url(url).map_err(|e| e.replace("server address", "Dreeve URL"))
}

/// A Dreeve API key is "drv_" followed by 64 hex characters.
fn key_ok(key: &str) -> bool {
    key.len() == 68 && key.starts_with("drv_") && key[4..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Turns an error reply into a sentence: the server's own message when it sends one.
fn error_message(status: u16, json: &serde_json::Value) -> String {
    if let Some(message) = json["message"].as_str() {
        return match json["error"].as_str() {
            Some(code) => format!("{message} ({code}, HTTP {status})"),
            None => format!("{message} (HTTP {status})"),
        };
    }
    match status {
        401 | 403 => format!("Dreeve refused the API key (HTTP {status}). Check the key and that DREEVE_API_KEY is set on the server."),
        404 => format!("Nothing at that address (HTTP {status}). Check the Dreeve URL."),
        409 => format!("Dreeve is not accepting uploads (HTTP {status}). It must run with IMPORT_MODE=files."),
        413 => format!("The file is too large for the server (HTTP {status})."),
        _ => format!("Unexpected reply from Dreeve (HTTP {status})."),
    }
}

fn reach_error(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "Dreeve did not respond in time.".into()
    } else {
        format!("Could not reach Dreeve: {e}")
    }
}

/// A GET against the instance with the saved key; errors become sentences.
async fn get_json(ctx: &Ctx, creds: &Creds, path_and_query: &str) -> Result<serde_json::Value, String> {
    if !creds.verified {
        return Err("Not connected to Dreeve.".into());
    }
    let res = ctx.http.get(format!("{}{path_and_query}", creds.url)).bearer_auth(&creds.api_key).timeout(Duration::from_secs(15)).send().await.map_err(reach_error)?;
    let status = res.status().as_u16();
    let json: serde_json::Value = res.json().await.unwrap_or_default();
    if status != 200 {
        return Err(error_message(status, &json));
    }
    Ok(json)
}

/// Activity ids go into the URL path; only letters, digits, `-` and `_` are let through.
fn check_id(id: &str) -> Result<(), String> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err("Not a Dreeve activity id.".into());
    }
    Ok(())
}

#[async_trait]
impl Connector for Dreeve {
    fn info(&self) -> ConnectorInfo {
        ConnectorInfo {
            id: ID,
            name: "Dreeve",
            description: "Rides are queued on your own Dreeve server and imported within a few minutes.",
            auth: AuthKind::Token,
            fields: vec![
                ConfigField { key: "url", label: "Dreeve URL", hint: "https://dreeve.example.com", kind: FieldKind::Url },
                ConfigField { key: "apiKey", label: "API key", hint: "drv_…", kind: FieldKind::Secret },
            ],
            setup: vec![
                SetupStep { text: "In Dreeve's admin panel open Settings → Security → API access and generate a key.", link: None },
                SetupStep { text: "Set it as DREEVE_API_KEY in Dreeve's .env and recreate the containers. Dreeve must run with IMPORT_MODE=files.", link: None },
                SetupStep { text: "Enter your instance's address and the key below, then save and test.", link: None },
            ],
        }
    }

    fn status(&self, ctx: &Ctx) -> ConnectorStatus {
        to_status(&load(ctx))
    }

    fn configure(&self, ctx: &Ctx, fields: &BTreeMap<String, String>) -> Result<ConnectorStatus, String> {
        let prev = load(ctx);
        let url = normalise_url(&field(fields, "url"))?;
        let typed = field(fields, "apiKey");
        let api_key = if typed.is_empty() { prev.api_key } else { typed };
        if api_key.is_empty() {
            return Err("Enter your Dreeve API key.".into());
        }
        if !key_ok(&api_key) {
            return Err("A Dreeve API key is \"drv_\" followed by 64 hex characters.".into());
        }
        // New credentials are unverified until `connect` checks them.
        let next = Creds { url, api_key, ..Default::default() };
        ctx.save_creds(ID, &next)?;
        Ok(to_status(&next))
    }

    async fn connect(&self, ctx: &Ctx, _open_browser: OpenBrowser) -> Result<ConnectorStatus, String> {
        let mut creds = load(ctx);
        if creds.url.is_empty() || creds.api_key.is_empty() {
            return Err("Enter your Dreeve URL and API key first.".into());
        }
        let res = ctx.http.get(format!("{}{STATUS_PATH}", creds.url)).bearer_auth(&creds.api_key).timeout(Duration::from_secs(15)).send().await.map_err(reach_error)?;
        let status = res.status().as_u16();
        let json: serde_json::Value = res.json().await.unwrap_or_default();
        if status != 200 {
            return Err(error_message(status, &json));
        }
        if json["app"].as_str() != Some("dreeve") {
            return Err("That address answered, but not like a Dreeve instance.".into());
        }
        let version = json["version"].as_str().unwrap_or("unknown version");
        if json["canUpload"].as_bool() != Some(true) {
            return Err(format!("Connected to Dreeve {version}, but uploads are disabled: it must run with IMPORT_MODE=files."));
        }
        creds.verified = true;
        creds.server = format!("Dreeve {version}");
        ctx.save_creds(ID, &creds)?;
        Ok(to_status(&creds))
    }

    async fn disconnect(&self, ctx: &Ctx) -> Result<ConnectorStatus, String> {
        // Keep the URL and key so connecting again is one click.
        let next = Creds { verified: false, server: String::new(), ..load(ctx) };
        ctx.save_creds(ID, &next)?;
        Ok(to_status(&next))
    }

    async fn upload(&self, ctx: &Ctx, req: &UploadRequest) -> Result<UploadRecord, String> {
        let creds = load(ctx);
        if !creds.verified {
            return Err("Not connected to Dreeve.".into());
        }
        // Dreeve reads everything from the FIT file; the title and description have nowhere to go.
        let file_name = req.file.file_name().and_then(|n| n.to_str()).unwrap_or("ride.fit").to_string();
        let bytes = tokio::task::spawn_blocking({
            let file = req.file.clone();
            move || std::fs::read(file)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("Could not read the ride file: {e}"))?;
        let form = reqwest::multipart::Form::new().part("file", reqwest::multipart::Part::bytes(bytes).file_name(file_name));
        let res = ctx.http.post(format!("{}{UPLOAD_PATH}", creds.url)).bearer_auth(&creds.api_key).multipart(form).timeout(Duration::from_secs(120)).send().await.map_err(reach_error)?;
        let status = res.status().as_u16();
        let json: serde_json::Value = res.json().await.unwrap_or_default();
        if !matches!(status, 200 | 202) {
            return Err(error_message(status, &json));
        }
        let message = json["message"].as_str().unwrap_or("File queued for import.").to_string();
        Ok(UploadRecord { uploaded_at: now_ms(), reference: None, url: None, message: Some(message) })
    }

    fn lists_rides(&self) -> bool {
        true
    }

    /// One page of cycling activities with a GPS track; the server filters by sport type and `hasGpx`.
    async fn recent_rides(&self, ctx: &Ctx, page: u32) -> Result<RidePage, String> {
        let creds = load(ctx);
        let json = get_json(ctx, &creds, &format!("{ACTIVITIES_PATH}?filters[sportType]={}&filters[hasGpx]=true&pagination[size]=100&pagination[page]={page}", RIDE_TYPES.join(","))).await?;
        let rides = json["activities"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| a["sportType"].as_str().is_some_and(|t| RIDE_TYPES.contains(&t)) && a["hasGpx"].as_bool() != Some(false))
            .filter_map(|a| {
                Some(RemoteRide {
                    id: a["id"].as_str()?.to_string(),
                    name: a["name"].as_str().unwrap_or("Ride").to_string(),
                    start: a["startDateLocal"].as_str().unwrap_or("").to_string(),
                    distance: a["distanceInMeter"].as_f64().unwrap_or(0.0),
                    moving_time: a["movingTimeInSeconds"].as_f64().unwrap_or(0.0),
                    sport: a["sportType"].as_str().unwrap_or("").to_string(),
                    outline: decode_polyline(a["encodedPolyline"].as_str().unwrap_or("")),
                    ..Default::default()
                })
            })
            .collect();
        let pagination = &json["pagination"];
        let more = pagination["page"].as_u64().zip(pagination["totalPages"].as_u64()).is_some_and(|(page, total)| page < total);
        Ok(RidePage { rides, more })
    }

    fn imports_routes(&self) -> bool {
        true
    }

    /// Dreeve's own GPX export of the ride (Dreeve 5.4 and later), with elevation where it has it.
    async fn ride_gpx(&self, ctx: &Ctx, id: &str) -> Result<String, String> {
        check_id(id)?;
        let creds = load(ctx);
        if !creds.verified {
            return Err("Not connected to Dreeve.".into());
        }
        let res = ctx.http.get(format!("{}{ACTIVITIES_PATH}/{id}/gpx", creds.url)).bearer_auth(&creds.api_key).timeout(Duration::from_secs(30)).send().await.map_err(reach_error)?;
        let status = res.status().as_u16();
        let text = res.text().await.map_err(|e| format!("Could not read the GPX from Dreeve: {e}"))?;
        if status != 200 {
            return Err(error_message(status, &serde_json::from_str(&text).unwrap_or_default()));
        }
        Ok(text)
    }

    /// The ride's `latLng` and `timeInSeconds` streams, lined up by index; points without a fix are skipped.
    async fn ride_track(&self, ctx: &Ctx, id: &str) -> Result<RideTrack, String> {
        check_id(id)?;
        let creds = load(ctx);
        let info = get_json(ctx, &creds, &format!("{ACTIVITIES_PATH}/{id}")).await?;
        // Local time without a zone; ghosts only use the differences between timestamps.
        let start = info["startDateLocal"].as_str().and_then(velopopuli_core::gpx::parse_iso_time).unwrap_or(0.0);
        let streams = get_json(ctx, &creds, &format!("{ACTIVITIES_PATH}/{id}/streams")).await?;
        let (Some(ll), Some(tt)) = (streams["latLng"].as_array(), streams["timeInSeconds"].as_array()) else {
            return Err("That ride has no GPS track.".into());
        };
        let points: Vec<(f64, f64, f64)> = ll.iter().zip(tt).filter_map(|(p, t)| Some((start + t.as_f64()?, p[0].as_f64()?, p[1].as_f64()?))).collect();
        if points.is_empty() {
            return Err("That ride has no GPS track.".into());
        }
        Ok(RideTrack { name: info["name"].as_str().unwrap_or("Dreeve ride").to_string(), points })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "drv_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn url_is_normalised_and_checked() {
        assert_eq!(normalise_url(" https://dreeve.example.com/ ").unwrap(), "https://dreeve.example.com");
        assert_eq!(normalise_url("http://localhost:8080//").unwrap(), "http://localhost:8080");
        assert!(normalise_url("http://10.0.0.5:8080").unwrap_err().contains("https://"));
        assert!(normalise_url("dreeve.example.com").is_err());
        assert!(normalise_url("ftp://dreeve.example.com").is_err());
        assert!(normalise_url("").is_err());
    }

    #[test]
    fn key_format() {
        assert!(key_ok(KEY));
        assert!(!key_ok("not-a-key"));
        assert!(!key_ok(&KEY[..67]));
        assert!(!key_ok(&format!("{}g", &KEY[..67])));
    }

    #[test]
    fn error_messages_prefer_the_servers_words() {
        let body = serde_json::json!({ "error": "import_mode_not_files", "message": "Import mode is not files." });
        assert_eq!(error_message(409, &body), "Import mode is not files. (import_mode_not_files, HTTP 409)");
        assert!(error_message(401, &serde_json::Value::Null).contains("API key"));
        assert!(error_message(404, &serde_json::Value::Null).contains("URL"));
        assert!(error_message(413, &serde_json::Value::Null).contains("too large"));
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[test]
    fn configure_validates_keeps_the_saved_key_and_never_echoes_it() {
        let dir = std::env::temp_dir().join(format!("vp-dreeve-{}", std::process::id()));
        let ctx = Ctx::new(&dir, &reqwest::Client::new());
        let fields = |url: &str, key: &str| BTreeMap::from([("url".to_string(), url.to_string()), ("apiKey".to_string(), key.to_string())]);

        assert!(Dreeve.configure(&ctx, &fields("https://d.example.com", "bad")).is_err());
        assert!(Dreeve.configure(&ctx, &fields("d.example.com", KEY)).is_err());
        assert!(Dreeve.configure(&ctx, &fields("https://d.example.com", "")).is_err(), "no key saved yet");

        let s = Dreeve.configure(&ctx, &fields("https://d.example.com/", KEY)).unwrap();
        assert!(s.configured && !s.connected);
        assert_eq!(s.fields["url"], "https://d.example.com");
        assert!(!serde_json::to_string(&s).unwrap().contains(KEY));

        // A blank key keeps the saved one; a changed URL drops the verified state.
        let s = Dreeve.configure(&ctx, &fields("https://other.example.com", "")).unwrap();
        assert!(s.configured);
        assert_eq!(load(&ctx).api_key, KEY);
        assert_eq!(Dreeve.status(&ctx).fields["url"], "https://other.example.com");
        std::fs::remove_dir_all(dir).ok();
    }

    /// An HTTP server that takes one connection per reply, records each request and answers
    /// the reply's status with its JSON body.
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    async fn fake_server(replies: &[(&'static str, &'static str)]) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let replies: Vec<String> =
            replies.iter().map(|(status, body)| format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())).collect();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let mut requests = Vec::new();
            for reply in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut got = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = socket.read(&mut buf).await.unwrap();
                    got.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&got);
                    let Some(head_end) = text.find("\r\n\r\n") else { continue };
                    let length = text[..head_end].lines().find_map(|l| l.strip_prefix("content-length: ")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                    if got.len() >= head_end + 4 + length || n == 0 {
                        break;
                    }
                }
                socket.write_all(reply.as_bytes()).await.unwrap();
                socket.shutdown().await.ok();
                requests.push(String::from_utf8_lossy(&got).into_owned());
            }
            requests
        });
        (url, handle)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[tokio::test]
    async fn connect_and_upload_speak_the_dreeve_api() {
        let dir = std::env::temp_dir().join(format!("vp-dreeve-http-{}", std::process::id()));
        let ctx = Ctx::new(&dir, &reqwest::Client::new());
        let fit = dir.join("ride.fit");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&fit, b"FITBYTES").unwrap();
        let no_browser: OpenBrowser = Box::new(|_| {});

        // Status: wrong import mode is a clear error and leaves the connector unlinked.
        let (url, server) = fake_server(&[("200 OK", "{\"app\":\"dreeve\",\"version\":\"2.1.0\",\"canUpload\":false}")]).await;
        Dreeve.configure(&ctx, &BTreeMap::from([("url".to_string(), url), ("apiKey".to_string(), KEY.to_string())])).unwrap();
        let err = Dreeve.connect(&ctx, Box::new(|_| {})).await.unwrap_err();
        assert!(err.contains("IMPORT_MODE=files"), "{err}");
        let request = server.await.unwrap().remove(0);
        assert!(request.starts_with("GET /api/v1/status HTTP/1.1"), "{request}");
        assert!(request.to_lowercase().contains(&format!("authorization: bearer {KEY}")), "{request}");
        assert!(!Dreeve.status(&ctx).connected);

        // Status: a healthy instance links the connector and names the server.
        let (url, server) = fake_server(&[("200 OK", "{\"app\":\"dreeve\",\"version\":\"2.1.0\",\"canUpload\":true}")]).await;
        Dreeve.configure(&ctx, &BTreeMap::from([("url".to_string(), url), ("apiKey".to_string(), String::new())])).unwrap();
        let status = Dreeve.connect(&ctx, no_browser).await.unwrap();
        server.await.unwrap();
        assert!(status.connected);
        assert_eq!(status.account, "Dreeve 2.1.0");

        // Upload: multipart "file" part with the file's name and bytes; 202 is success with the server's message.
        let (url, server) = fake_server(&[("202 Accepted", "{\"status\":\"queued\",\"message\":\"Queued, five minutes.\"}")]).await;
        let mut creds = load(&ctx);
        creds.url = url;
        ctx.save_creds(ID, &creds).unwrap();
        let record = Dreeve.upload(&ctx, &UploadRequest { file: fit.clone(), name: "Alpe".into(), description: String::new() }).await.unwrap();
        let request = server.await.unwrap().remove(0);
        assert!(request.starts_with("POST /api/v1/activity/upload HTTP/1.1"), "{request}");
        assert!(request.contains("name=\"file\"") && request.contains("filename=\"ride.fit\"") && request.contains("FITBYTES"), "{request}");
        assert_eq!(record.message.as_deref(), Some("Queued, five minutes."));
        assert!(record.url.is_none());

        // Upload: an error reply becomes the server's own words.
        let (url, server) = fake_server(&[("401 Unauthorized", "{\"error\":\"invalid_token\",\"message\":\"A valid token is required.\"}")]).await;
        creds.url = url;
        ctx.save_creds(ID, &creds).unwrap();
        let err = Dreeve.upload(&ctx, &UploadRequest { file: fit, name: String::new(), description: String::new() }).await.unwrap_err();
        server.await.unwrap();
        assert_eq!(err, "A valid token is required. (invalid_token, HTTP 401)");
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[tokio::test]
    async fn rides_and_tracks_speak_the_dreeve_api() {
        let dir = std::env::temp_dir().join(format!("vp-dreeve-rides-{}", std::process::id()));
        let ctx = Ctx::new(&dir, &reqwest::Client::new());
        let page = concat!(
            "{\"activities\":[",
            "{\"id\":\"activity-1\",\"name\":\"Evening Ride\",\"sportType\":\"GravelRide\",\"startDateLocal\":\"2026-11-09T18:05:38\",\"distanceInMeter\":42000,\"movingTimeInSeconds\":5400,\"hasGpx\":true,\"encodedPolyline\":\"_p~iF~ps|U_ulLnnqC_mqNvxq`@\"},",
            "{\"id\":\"activity-2\",\"name\":\"Lunch Run\",\"sportType\":\"Run\",\"startDateLocal\":\"2026-11-08T12:00:00\",\"distanceInMeter\":8000,\"movingTimeInSeconds\":2400,\"hasGpx\":true},",
            "{\"id\":\"activity-3\",\"name\":\"Trainer\",\"sportType\":\"VirtualRide\",\"startDateLocal\":\"2026-11-07T07:00:00\",\"distanceInMeter\":30000,\"movingTimeInSeconds\":3600,\"hasGpx\":false}",
            "],\"pagination\":{\"page\":1,\"size\":100,\"total\":250,\"totalPages\":3}}"
        );
        let detail = "{\"id\":\"activity-1\",\"name\":\"Evening Ride\",\"startDateLocal\":\"2026-11-09T18:05:38\",\"hasGpx\":true}";
        let streams = "{\"timeInSeconds\":[0,1,2,3],\"latLng\":[[51.2,3.18],null,[51.2001,3.1802],[51.2002,3.1804]]}";
        let gpx = "<?xml version=\"1.0\"?><gpx><trk><name>Evening Ride</name><trkseg><trkpt lat=\"51.2\" lon=\"3.18\"><ele>12</ele></trkpt></trkseg></trk></gpx>";
        let (url, server) = fake_server(&[
            ("200 OK", page),
            ("200 OK", detail),
            ("200 OK", streams),
            ("200 OK", gpx),
            ("404 Not Found", "{\"error\":\"FILE_NOT_AVAILABLE\",\"message\":\"Activity \\\"activity-3\\\" has no GPS data to export as GPX.\"}"),
        ])
        .await;
        ctx.save_creds(ID, &Creds { url, api_key: KEY.into(), verified: true, server: "Dreeve 2.1.0".into() }).unwrap();

        let page = Dreeve.recent_rides(&ctx, 1).await.unwrap();
        assert!(page.more, "page 1 of 3");
        let rides = page.rides;
        assert_eq!(rides[0].outline.len(), 3, "the encoded polyline is the outline");
        assert!((rides[0].outline[0].0 - 38.5).abs() < 1e-6 && (rides[0].outline[0].1 + 120.2).abs() < 1e-6);
        assert_eq!(rides.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["activity-1"], "runs and rides without GPS are left out");
        assert_eq!(rides[0].name, "Evening Ride");
        assert_eq!(rides[0].start, "2026-11-09T18:05:38");
        assert_eq!((rides[0].distance, rides[0].moving_time), (42000.0, 5400.0));
        assert_eq!(rides[0].sport, "GravelRide");

        let track = Dreeve.ride_track(&ctx, "activity-1").await.unwrap();
        assert_eq!(track.name, "Evening Ride");
        assert_eq!(track.points.len(), 3, "the point without a fix is skipped");
        assert_eq!(track.points[1].0 - track.points[0].0, 2.0);
        assert_eq!((track.points[2].1, track.points[2].2), (51.2002, 3.1804));
        assert!(track.points[0].0 > 1.7e9, "absolute start from startDateLocal");

        assert_eq!(Dreeve.ride_gpx(&ctx, "activity-1").await.unwrap(), gpx);
        assert!(Dreeve.ride_gpx(&ctx, "activity-3").await.unwrap_err().contains("no GPS data"));

        let requests = server.await.unwrap();
        assert!(requests[0].starts_with("GET /api/v1/activities?filters[sportType]=Ride,MountainBikeRide,GravelRide,EBikeRide,EMountainBikeRide,VirtualRide,Velomobile,Handcycle&filters[hasGpx]=true&pagination[size]=100&pagination[page]=1 HTTP/1.1"), "{}", requests[0]);
        assert!(requests[0].to_lowercase().contains(&format!("authorization: bearer {KEY}")));
        assert!(requests[1].starts_with("GET /api/v1/activities/activity-1 HTTP/1.1"), "{}", requests[1]);
        assert!(requests[2].starts_with("GET /api/v1/activities/activity-1/streams HTTP/1.1"), "{}", requests[2]);
        assert!(requests[3].starts_with("GET /api/v1/activities/activity-1/gpx HTTP/1.1"), "{}", requests[3]);
        assert!(requests[3].to_lowercase().contains(&format!("authorization: bearer {KEY}")));

        assert!(Dreeve.ride_track(&ctx, "../etc").await.unwrap_err().contains("activity id"));
        assert!(Dreeve.ride_gpx(&ctx, "../etc").await.unwrap_err().contains("activity id"));
        std::fs::remove_dir_all(dir).ok();
    }

    /// Needs a real instance: `DREEVE_URL=https://… DREEVE_API_KEY=drv_… cargo test -p velopopuli live_dreeve -- --ignored --nocapture`,
    /// optionally `DREEVE_ROUTE_RIDE=<activity id>` for the ride the test route is made of.
    /// Read-only: links, lists rides, fetches one track and its GPX; uploads nothing.
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[tokio::test]
    #[ignore]
    async fn live_dreeve_lists_rides_and_tracks() {
        let (Ok(url), Ok(key)) = (std::env::var("DREEVE_URL"), std::env::var("DREEVE_API_KEY")) else {
            eprintln!("skipped: set DREEVE_URL and DREEVE_API_KEY");
            return;
        };
        let dir = std::env::temp_dir().join(format!("vp-dreeve-live-{}", std::process::id()));
        let http = reqwest::Client::builder().user_agent("VeloPopuli/0.3 (open-source indoor cycling app)").connect_timeout(Duration::from_secs(8)).build().unwrap();
        let ctx = Ctx::new(&dir, &http);
        let fields = BTreeMap::from([("url".to_string(), url), ("apiKey".to_string(), key)]);
        let status = Dreeve.configure(&ctx, &fields).unwrap();
        assert!(status.configured && !status.connected);
        let status = Dreeve.connect(&ctx, Box::new(|_| {})).await.unwrap();
        assert!(status.connected);
        eprintln!("connected: {}", status.account);

        let page = Dreeve.recent_rides(&ctx, 1).await.unwrap();
        let rides = page.rides;
        eprintln!("page 1: {} rides with GPS, more: {}, with outline: {}", rides.len(), page.more, rides.iter().filter(|r| !r.outline.is_empty()).count());
        for r in rides.iter().take(5) {
            eprintln!("  {} | {} | {} | {:.1} km | {:.0} min", r.id, r.start, r.name, r.distance / 1000.0, r.moving_time / 60.0);
        }
        // An outdoor ride (a virtual one's GPS is a made-up world): make a route of its track and
        // the ride search must find that ride as one that rides it.
        // DREEVE_ROUTE_RIDE=<activity id> picks the ride to make the route of.
        let pick = std::env::var("DREEVE_ROUTE_RIDE").ok();
        if let Some(outdoor) = rides.iter().find(|r| pick.as_deref().map_or(!r.name.contains("Virtual") && r.distance > 5000.0, |id| r.id == id)) {
            let track = Dreeve.ride_track(&ctx, &outdoor.id).await.unwrap();
            let (t0, tn) = (track.points[0].0, track.points[track.points.len() - 1].0);
            eprintln!("track of {:?}: {} points over {:.0} min", track.name, track.points.len(), (tn - t0) / 60.0);
            assert!(track.points.len() > 10 && tn > t0);
            let pts: Vec<velopopuli_core::gpx::RawPoint> = track.points.iter().map(|p| velopopuli_core::gpx::RawPoint { lat: p.1, lon: p.2, ele: None }).collect();
            let route = velopopuli_core::gpx::build_route("live", &pts).unwrap();
            let found = super::super::find_rides(&Dreeve, &ctx, &route).await.unwrap();
            let fitting: Vec<String> = found
                .rides
                .iter()
                .filter(|r| r.follows_route)
                .map(|r| format!("{} {} ({:.0}/{:.0} km)", r.start.get(..10).unwrap_or(""), r.name, r.followed_m / 1000.0, route.total / 1000.0))
                .collect();
            let partial = found.rides.iter().filter(|r| !r.follows_route && r.followed_m > 0.0).count();
            eprintln!(
                "route from {:?}, {:.0} km: searched {} rides (more: {}), {} fit, {} share only its start",
                outdoor.name,
                route.total / 1000.0,
                found.rides.len(),
                found.more,
                fitting.len(),
                partial
            );
            for f in &fitting {
                eprintln!("  fits: {f}");
            }
            assert!(found.rides.iter().any(|r| r.id == outdoor.id && r.follows_route), "the ride the route was made from fits it");

            // The same ride as a route: Dreeve's GPX export must build one with elevation.
            let gpx = Dreeve.ride_gpx(&ctx, &outdoor.id).await.unwrap();
            let (name, pts) = velopopuli_core::gpx::parse_gpx(&gpx).unwrap();
            let route = velopopuli_core::gpx::build_route(&name, &pts).unwrap();
            eprintln!("GPX of {name:?}: {} points, {:.0} km, {:.0} m up", pts.len(), route.total / 1000.0, route.ascent);
            assert!(pts.iter().any(|p| p.ele.is_some()), "the export carries elevation");
        }

        let all = super::super::list_rides(&Dreeve, &ctx).await.unwrap();
        let mut sports: Vec<&str> = all.rides.iter().map(|r| r.sport.as_str()).collect();
        sports.sort_unstable();
        sports.dedup();
        eprintln!("listed {} rides (more: {}), sports {sports:?}, with preview: {}", all.rides.len(), all.more, all.rides.iter().filter(|r| !r.preview.is_empty()).count());
        assert!(all.rides.len() >= rides.len());
        std::fs::remove_dir_all(dir).ok();
    }
}
