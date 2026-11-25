//! Intervals.icu, a training log and planner: https://intervals.icu
//!
//! API (https://intervals.icu/api/v1/docs): HTTP basic auth with the user name `API_KEY` and
//! the rider's API key as the password, both from Settings → Developer Settings, where the
//! athlete id (`i12345`) is shown too. `GET /api/v1/athlete/{id}` checks the key,
//! `POST /api/v1/athlete/{id}/activities` takes a multipart `file`, and the calendar's planned
//! workouts come from `GET /api/v1/athlete/{id}/events?category=WORKOUT`, each downloadable as
//! a Zwift `.zwo` file with `GET …/events/{event id}/download.zwo`.

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use velopopuli_core::types::UploadRecord;

use super::{field, AuthKind, ConfigField, Connector, ConnectorInfo, ConnectorStatus, Ctx, FieldKind, OpenBrowser, PlannedWorkout, SetupStep, UploadRequest};
use crate::now_ms;

pub const ID: &str = "intervals";
const BASE: &str = "https://intervals.icu";
/// Its cycling activity types; workouts for runs, swims and the rest are left out.
const RIDE_TYPES: &[&str] = &["Ride", "VirtualRide", "GravelRide", "MountainBikeRide", "EBikeRide", "EMountainBikeRide", "TrackRide", "Velomobile", "Handcycle"];

pub struct Intervals;

#[derive(Default, Serialize, Deserialize)]
struct Creds {
    /// `i12345`.
    athlete_id: String,
    api_key: String,
    /// The athlete call succeeded with these credentials.
    #[serde(default)]
    verified: bool,
    /// The rider's name on Intervals.icu, from the athlete call.
    #[serde(default)]
    athlete: String,
    /// Where the API is; only tests change it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    base: String,
}

impl Creds {
    fn url(&self, path: &str) -> String {
        format!("{}/api/v1/athlete/{}{path}", if self.base.is_empty() { BASE } else { &self.base }, self.athlete_id)
    }
}

fn load(ctx: &Ctx) -> Creds {
    ctx.load_creds(ID)
}

fn to_status(c: &Creds) -> ConnectorStatus {
    ConnectorStatus {
        id: ID,
        configured: !c.athlete_id.is_empty() && !c.api_key.is_empty(),
        connected: c.verified,
        account: c.athlete.clone(),
        fields: BTreeMap::from([("athleteId", c.athlete_id.clone())]),
    }
}

/// The athlete id as the API wants it (`i12345`), or why it can't be one. A bare number gets
/// its `i`.
fn normalise_athlete_id(raw: &str) -> Result<String, String> {
    let id = raw.trim();
    if id.is_empty() {
        return Err("Enter your Intervals.icu athlete ID.".into());
    }
    let digits = id.strip_prefix('i').or_else(|| id.strip_prefix('I')).unwrap_or(id);
    if digits.is_empty() || digits.len() > 12 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err("An Intervals.icu athlete ID is an i followed by digits, such as i12345. It's shown under Settings → Developer Settings.".into());
    }
    Ok(format!("i{digits}"))
}

/// Turns an error reply into a sentence.
fn error_message(status: u16, json: &serde_json::Value) -> String {
    match status {
        401 | 403 => format!("Intervals.icu refused the API key (HTTP {status}). Check the athlete ID and API key under Settings → Developer Settings on intervals.icu."),
        404 => format!("Intervals.icu doesn't know that athlete ID, or the item is gone (HTTP {status})."),
        429 => "Intervals.icu is busy (too many requests). Try again in a minute.".into(),
        _ => match json["error"].as_str().or(json["message"].as_str()) {
            Some(message) => format!("Intervals.icu: {message} (HTTP {status})"),
            None => format!("Unexpected reply from Intervals.icu (HTTP {status})."),
        },
    }
}

fn reach_error(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "Intervals.icu did not respond in time.".into()
    } else {
        format!("Could not reach Intervals.icu: {e}")
    }
}

/// A GET with the saved key: the body as text, or the error as a sentence.
async fn get_text(ctx: &Ctx, creds: &Creds, path: &str, query: &[(&str, String)]) -> Result<String, String> {
    let res = ctx.http.get(creds.url(path)).query(query).basic_auth("API_KEY", Some(&creds.api_key)).timeout(Duration::from_secs(20)).send().await.map_err(reach_error)?;
    let status = res.status().as_u16();
    let text = res.text().await.map_err(reach_error)?;
    if status != 200 {
        return Err(error_message(status, &serde_json::from_str(&text).unwrap_or_default()));
    }
    Ok(text)
}

fn connected(ctx: &Ctx) -> Result<Creds, String> {
    let creds = load(ctx);
    if !creds.verified {
        return Err("Not connected to Intervals.icu. Connect it in Settings.".into());
    }
    Ok(creds)
}

/// The rider's name from the athlete record, or their id when it has none.
fn athlete_name(json: &serde_json::Value, id: &str) -> String {
    let name = json["name"].as_str().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
    let parts = [&json["firstname"], &json["lastname"]].iter().filter_map(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
    name.or((!parts.is_empty()).then_some(parts)).unwrap_or_else(|| id.to_string())
}

/// The cycling workouts in a calendar listing that have steps to ride.
fn planned_from(events: &serde_json::Value) -> Vec<PlannedWorkout> {
    events
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["category"].as_str().is_none_or(|c| c == "WORKOUT"))
        .filter(|e| e["type"].as_str().is_some_and(|t| RIDE_TYPES.contains(&t)))
        // A workout written as a note has no steps, so there'd be nothing to ride.
        .filter(|e| e["workout_doc"].is_null() || e["workout_doc"]["steps"].as_array().is_some_and(|s| !s.is_empty()))
        .filter_map(|e| {
            Some(PlannedWorkout {
                id: e["id"].as_i64()?.to_string(),
                date: e["start_date_local"].as_str().and_then(|d| d.get(..10)).unwrap_or("").to_string(),
                name: e["name"].as_str().unwrap_or("Workout").to_string(),
                description: e["description"].as_str().unwrap_or("").trim().to_string(),
                duration_s: e["moving_time"].as_f64().or(e["workout_doc"]["duration"].as_f64()).unwrap_or(0.0),
                load: e["icu_training_load"].as_f64(),
                added: false,
            })
        })
        .collect()
}

#[async_trait]
impl Connector for Intervals {
    fn info(&self) -> ConnectorInfo {
        ConnectorInfo {
            id: ID,
            name: "Intervals.icu",
            description: "Rides go straight to your training log, and the workouts on your calendar can be ridden here.",
            auth: AuthKind::Token,
            fields: vec![
                ConfigField { key: "athleteId", label: "Athlete ID", hint: "i12345", kind: FieldKind::Text },
                ConfigField { key: "apiKey", label: "API key", hint: "", kind: FieldKind::Secret },
            ],
            setup: vec![
                SetupStep { text: "On intervals.icu, open Settings and scroll down to Developer Settings.", link: Some("https://intervals.icu/settings") },
                SetupStep { text: "Copy your athlete ID (it starts with i) and create an API key, then enter both below.", link: None },
            ],
        }
    }

    fn status(&self, ctx: &Ctx) -> ConnectorStatus {
        to_status(&load(ctx))
    }

    fn configure(&self, ctx: &Ctx, fields: &BTreeMap<String, String>) -> Result<ConnectorStatus, String> {
        let prev = load(ctx);
        let athlete_id = normalise_athlete_id(&field(fields, "athleteId"))?;
        let typed = field(fields, "apiKey");
        let api_key = if typed.is_empty() { prev.api_key } else { typed };
        if api_key.is_empty() {
            return Err("Enter your Intervals.icu API key.".into());
        }
        if api_key.chars().any(char::is_whitespace) {
            return Err("That API key has spaces in it. Copy it again from Developer Settings.".into());
        }
        // New credentials are unverified until `connect` checks them.
        let next = Creds { athlete_id, api_key, base: prev.base, ..Default::default() };
        ctx.save_creds(ID, &next)?;
        Ok(to_status(&next))
    }

    async fn connect(&self, ctx: &Ctx, _open_browser: OpenBrowser) -> Result<ConnectorStatus, String> {
        let mut creds = load(ctx);
        if creds.athlete_id.is_empty() || creds.api_key.is_empty() {
            return Err("Enter your Intervals.icu athlete ID and API key first.".into());
        }
        let text = get_text(ctx, &creds, "", &[]).await?;
        let json: serde_json::Value = serde_json::from_str(&text).map_err(|_| "Intervals.icu answered, but not with an athlete.".to_string())?;
        creds.verified = true;
        creds.athlete = athlete_name(&json, &creds.athlete_id);
        ctx.save_creds(ID, &creds)?;
        Ok(to_status(&creds))
    }

    async fn disconnect(&self, ctx: &Ctx) -> Result<ConnectorStatus, String> {
        // Keep the athlete id and key so connecting again is one click.
        let next = Creds { verified: false, athlete: String::new(), ..load(ctx) };
        ctx.save_creds(ID, &next)?;
        Ok(to_status(&next))
    }

    async fn upload(&self, ctx: &Ctx, req: &UploadRequest) -> Result<UploadRecord, String> {
        let creds = connected(ctx)?;
        let file_name = req.file.file_name().and_then(|n| n.to_str()).unwrap_or("ride.fit").to_string();
        let bytes = tokio::task::spawn_blocking({
            let file = req.file.clone();
            move || std::fs::read(file)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("Could not read the ride file: {e}"))?;
        // The file name is the ride's own id here, so Intervals.icu can tell a second upload of it.
        let external_id = file_name.trim_end_matches(".fit").to_string();
        let mut query = vec![("device_name", "VeloPopuli".to_string()), ("external_id", external_id)];
        if !req.name.is_empty() {
            query.push(("name", req.name.clone()));
        }
        if !req.description.is_empty() {
            query.push(("description", req.description.clone()));
        }
        let form = reqwest::multipart::Form::new().part("file", reqwest::multipart::Part::bytes(bytes).file_name(file_name));
        let res =
            ctx.http.post(creds.url("/activities")).query(&query).basic_auth("API_KEY", Some(&creds.api_key)).multipart(form).timeout(Duration::from_secs(120)).send().await.map_err(reach_error)?;
        let status = res.status().as_u16();
        let json: serde_json::Value = res.json().await.unwrap_or_default();
        if !matches!(status, 200 | 201) {
            return Err(error_message(status, &json));
        }
        let activity = json["activities"][0]["id"].as_str().or(json["id"].as_str()).map(str::to_string);
        Ok(UploadRecord {
            uploaded_at: now_ms(),
            url: activity.as_ref().map(|id| format!("https://intervals.icu/activities/{id}")),
            reference: activity,
            message: Some("Uploaded to Intervals.icu.".into()),
        })
    }

    fn plans_workouts(&self) -> bool {
        true
    }

    async fn planned_workouts(&self, ctx: &Ctx, days: u32) -> Result<Vec<PlannedWorkout>, String> {
        let creds = connected(ctx)?;
        let today = chrono::Local::now().date_naive();
        let last = today + chrono::Days::new(u64::from(days.max(1) - 1));
        let query = [("oldest", today.to_string()), ("newest", last.to_string()), ("category", "WORKOUT".to_string())];
        let text = get_text(ctx, &creds, "/events", &query).await?;
        let json: serde_json::Value = serde_json::from_str(&text).map_err(|_| "Intervals.icu sent a calendar that couldn't be read.".to_string())?;
        let mut planned = planned_from(&json);
        planned.sort_by(|a, b| a.date.cmp(&b.date));
        Ok(planned)
    }

    async fn planned_workout_file(&self, ctx: &Ctx, id: &str) -> Result<(String, String), String> {
        // The id goes into the URL path: digits only.
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err("Not an Intervals.icu workout id.".into());
        }
        let creds = connected(ctx)?;
        let zwo = get_text(ctx, &creds, &format!("/events/{id}/download.zwo"), &[]).await?;
        Ok((format!("intervals-{id}.zwo"), zwo))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    use crate::connectors::testing::fake_server;

    #[test]
    fn athlete_ids_are_normalised_and_checked() {
        assert_eq!(normalise_athlete_id(" i12345 ").unwrap(), "i12345");
        assert_eq!(normalise_athlete_id("12345").unwrap(), "i12345");
        assert_eq!(normalise_athlete_id("I77").unwrap(), "i77");
        assert!(normalise_athlete_id("").is_err());
        assert!(normalise_athlete_id("i").is_err());
        assert!(normalise_athlete_id("i12/../x").is_err());
        assert!(normalise_athlete_id("bob").unwrap_err().contains("Developer Settings"));
    }

    #[test]
    fn errors_say_what_to_do() {
        assert!(error_message(401, &serde_json::Value::Null).contains("API key"));
        assert!(error_message(403, &serde_json::json!({ "error": "Forbidden" })).contains("Developer Settings"));
        assert!(error_message(429, &serde_json::Value::Null).contains("Try again"));
        assert_eq!(error_message(422, &serde_json::json!({ "error": "File type not supported" })), "Intervals.icu: File type not supported (HTTP 422)");
    }

    #[test]
    fn the_name_comes_from_the_athlete_record() {
        assert_eq!(athlete_name(&serde_json::json!({ "name": "Ada Lovelace" }), "i1"), "Ada Lovelace");
        assert_eq!(athlete_name(&serde_json::json!({ "firstname": "Ada", "lastname": "Lovelace" }), "i1"), "Ada Lovelace");
        assert_eq!(athlete_name(&serde_json::json!({}), "i1"), "i1");
    }

    #[test]
    fn only_cycling_workouts_with_steps_are_offered() {
        let events = serde_json::json!([
            { "id": 11, "category": "WORKOUT", "type": "Ride", "name": "Sweet spot 3x12", "start_date_local": "2026-11-26T00:00:00", "moving_time": 3600, "icu_training_load": 72, "workout_doc": { "steps": [{}] } },
            { "id": 12, "category": "WORKOUT", "type": "Run", "name": "Easy run", "start_date_local": "2026-11-26T00:00:00", "workout_doc": { "steps": [{}] } },
            { "id": 13, "category": "WORKOUT", "type": "VirtualRide", "name": "Just ride", "start_date_local": "2026-11-27T00:00:00", "workout_doc": { "steps": [] } },
            { "id": 14, "category": "NOTE", "type": "Ride", "name": "Rest day" },
            { "id": 15, "category": "WORKOUT", "type": "VirtualRide", "name": "VO2 5x4", "start_date_local": "2026-11-28T00:00:00", "workout_doc": { "duration": 4200, "steps": [{}] } }
        ]);
        let planned = planned_from(&events);
        let names: Vec<_> = planned.iter().map(|p| (p.id.as_str(), p.name.as_str(), p.date.as_str())).collect();
        assert_eq!(names, [("11", "Sweet spot 3x12", "2026-11-26"), ("15", "VO2 5x4", "2026-11-28")]);
        assert_eq!(planned[0].duration_s, 3600.0);
        assert_eq!(planned[0].load, Some(72.0));
        assert_eq!(planned[1].duration_s, 4200.0, "the workout's own length when the event has none");
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[test]
    fn configure_keeps_the_saved_key_and_never_echoes_it() {
        let dir = std::env::temp_dir().join(format!("vp-intervals-{}", std::process::id()));
        let ctx = Ctx::new(&dir, &reqwest::Client::new());
        let fields = |id: &str, key: &str| BTreeMap::from([("athleteId".to_string(), id.to_string()), ("apiKey".to_string(), key.to_string())]);
        assert!(Intervals.configure(&ctx, &fields("i12345", "")).is_err(), "no key saved yet");
        assert!(Intervals.configure(&ctx, &fields("nope", "secretkey")).is_err());
        let s = Intervals.configure(&ctx, &fields("12345", "secretkey")).unwrap();
        assert!(s.configured && !s.connected);
        assert_eq!(s.fields["athleteId"], "i12345");
        assert!(!serde_json::to_string(&s).unwrap().contains("secretkey"));
        Intervals.configure(&ctx, &fields("i777", "")).unwrap();
        assert_eq!(load(&ctx).api_key, "secretkey", "a blank key keeps the saved one");
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[tokio::test]
    async fn speaks_the_intervals_api() {
        let dir = std::env::temp_dir().join(format!("vp-intervals-http-{}", std::process::id()));
        let ctx = Ctx::new(&dir, &reqwest::Client::new());
        std::fs::create_dir_all(&dir).unwrap();
        let fit = dir.join("velopopuli-2026-11-25-1800-alpe.fit");
        std::fs::write(&fit, b"FITBYTES").unwrap();
        let point_at = |url: String| {
            let mut creds = load(&ctx);
            creds.base = url;
            ctx.save_creds(ID, &creds).unwrap();
        };
        // "API_KEY:secretkey" in base64, as basic auth sends it.
        let auth = "authorization: basic qvbjx0tfwtpzzwnyzxrrzxk=";

        Intervals.configure(&ctx, &BTreeMap::from([("athleteId".to_string(), "i42".to_string()), ("apiKey".to_string(), "secretkey".to_string())])).unwrap();
        assert!(Intervals.upload(&ctx, &UploadRequest { file: fit.clone(), name: String::new(), description: String::new() }).await.is_err(), "not connected yet");

        // Connect: a refused key is a clear error and leaves it unlinked.
        let (url, server) = fake_server(&[("401 Unauthorized", "{\"status\":401,\"error\":\"Unauthorized\"}")]).await;
        point_at(url);
        let err = Intervals.connect(&ctx, Box::new(|_| {})).await.unwrap_err();
        assert!(err.contains("API key"), "{err}");
        let request = server.await.unwrap().remove(0);
        assert!(request.starts_with("GET /api/v1/athlete/i42 HTTP/1.1"), "{request}");
        assert!(request.to_lowercase().contains(auth), "{request}");
        assert!(!Intervals.status(&ctx).connected);

        // Connect: the athlete call links it and names the rider.
        let (url, server) = fake_server(&[("200 OK", "{\"id\":\"i42\",\"name\":\"Ada Lovelace\"}")]).await;
        point_at(url);
        let status = Intervals.connect(&ctx, Box::new(|_| {})).await.unwrap();
        server.await.unwrap();
        assert!(status.connected);
        assert_eq!(status.account, "Ada Lovelace");

        // Upload: the file as multipart, the title in the query, a link to the activity back.
        let (url, server) = fake_server(&[("200 OK", "{\"icu_athlete_id\":\"i42\",\"id\":\"i9001\",\"activities\":[{\"id\":\"i9001\"}]}")]).await;
        point_at(url);
        let record = Intervals.upload(&ctx, &UploadRequest { file: fit.clone(), name: "Alpe d'Huez".into(), description: "Up the hairpins".into() }).await.unwrap();
        let request = server.await.unwrap().remove(0);
        assert!(request.starts_with("POST /api/v1/athlete/i42/activities?"), "{request}");
        let head = request.lines().next().unwrap();
        assert!(head.contains("external_id=velopopuli-2026-11-25-1800-alpe") && head.contains("name=Alpe") && head.contains("device_name=VeloPopuli"), "{head}");
        assert!(request.contains("name=\"file\"") && request.contains("filename=\"velopopuli-2026-11-25-1800-alpe.fit\"") && request.contains("FITBYTES"), "{request}");
        assert_eq!(record.url.as_deref(), Some("https://intervals.icu/activities/i9001"));
        assert_eq!(record.reference.as_deref(), Some("i9001"));

        // The calendar: only workouts, asked for from today.
        let (url, server) = fake_server(&[(
            "200 OK",
            "[{\"id\":7,\"category\":\"WORKOUT\",\"type\":\"Ride\",\"name\":\"Tempo\",\"start_date_local\":\"2026-11-26T00:00:00\",\"moving_time\":1800,\"workout_doc\":{\"steps\":[{}]}}]",
        )])
        .await;
        point_at(url);
        let planned = Intervals.planned_workouts(&ctx, 14).await.unwrap();
        let request = server.await.unwrap().remove(0);
        assert!(request.starts_with("GET /api/v1/athlete/i42/events?oldest="), "{request}");
        assert!(request.lines().next().unwrap().contains("category=WORKOUT"), "{request}");
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].id, "7");

        // One workout as a .zwo file the importer reads.
        let (url, server) = fake_server(&[("200 OK", "<workout_file><name>Tempo</name><workout><SteadyState Duration=\"600\" Power=\"0.8\"/></workout></workout_file>")]).await;
        point_at(url);
        let (name, text) = Intervals.planned_workout_file(&ctx, "7").await.unwrap();
        let request = server.await.unwrap().remove(0);
        assert!(request.starts_with("GET /api/v1/athlete/i42/events/7/download.zwo HTTP/1.1"), "{request}");
        assert_eq!(name, "intervals-7.zwo");
        let workout = velopopuli_core::workout::parse_zwo(&text, "w").unwrap();
        assert_eq!(workout.name, "Tempo");
        assert!(Intervals.planned_workout_file(&ctx, "../7").await.is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
