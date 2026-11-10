//! Dreeve (formerly "Statistics for Strava"), a self-hosted sports dashboard:
//! https://github.com/dreeveapp/dreeve
//!
//! API: bearer token `drv_<64 hex>` (generated in the admin panel and set as
//! `DREEVE_API_KEY` on the server), `GET /api/v1/status` to verify the URL and key,
//! `POST /api/v1/activity/upload` with a multipart `file` part. Uploads are queued
//! and answered with `202 Accepted`; the server imports them within minutes, so
//! there is no activity page to link to.

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use velopopuli_core::types::UploadRecord;

use super::{field, AuthKind, ConfigField, Connector, ConnectorInfo, ConnectorStatus, Ctx, FieldKind, OpenBrowser, SetupStep, UploadRequest};
use crate::now_ms;

pub const ID: &str = "dreeve";
const STATUS_PATH: &str = "/api/v1/status";
const UPLOAD_PATH: &str = "/api/v1/activity/upload";

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
}
