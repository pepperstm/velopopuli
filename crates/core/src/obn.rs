//! OpenBikeNet: meet riders of other indoor cycling apps on a shared track.
//!
//! This module is pure: the messages of protocol v0.2, the display-name rules and the mapping
//! from a built-in track to the route ID OpenBikeNet publishes. The connection itself lives in
//! `src-tauri/src/obn.rs`, because sockets are I/O.
//!
//! Spec: <https://github.com/OpenBikeNet/protocol/blob/main/spec/v0.md>

use serde::{Deserialize, Serialize};

/// The app ID this app reports to OpenBikeNet (spec 5).
pub const APP_ID: &str = "velopopuli";
pub const APP_NAME: &str = "VeloPopuli";
pub const APP_URL: &str = "https://velopopuli.com";

/// The shared route for a built-in track. OpenBikeNet publishes one route per direction, so a
/// clockwise ride is a different route rather than the same one ridden backwards (spec 4.2).
pub fn shared_route(track_id: &str) -> Option<&'static str> {
    match track_id {
        "herne-hill" => Some("herne-hill-450"),
        "herne-hill-clockwise" => Some("herne-hill-450-clockwise"),
        _ => None,
    }
}

/// The lap OpenBikeNet expects: laps finished, from 0. The ride engine counts the lap being
/// ridden from 1, so the first lap is 0 for everyone else.
pub fn lap(engine_lap: u32) -> u32 {
    engine_lap.saturating_sub(1)
}

/// A position OpenBikeNet accepts: `0 ≤ distance < length` (spec 6.4).
pub fn position(distance: f64, length: f64) -> f64 {
    distance.clamp(0.0, (length - 0.01).max(0.0))
}

/// The app's description in `hello`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppInfo {
    pub id: &'static str,
    pub version: &'static str,
    pub name: &'static str,
    pub url: &'static str,
}

pub fn app_info() -> AppInfo {
    AppInfo { id: APP_ID, version: env!("CARGO_PKG_VERSION"), name: APP_NAME, url: APP_URL }
}

/// A rider in an `update` (spec 6.4).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClientRider {
    pub rider: String,
    pub lap: u32,
    pub distance: f64,
    pub speed: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cadence: Option<u32>,
}

/// Messages this app sends (spec 6).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Hello {
        app: AppInfo,
    },
    Subscribe {
        route: String,
        room: String,
    },
    Join {
        rider: String,
        route: String,
        room: String,
        name: String,
        lap: u32,
        distance: f64,
        speed: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        power: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cadence: Option<u32>,
    },
    Update {
        riders: Vec<ClientRider>,
    },
    Leave {
        rider: String,
    },
    Ping {
        #[serde(skip_serializing_if = "Option::is_none")]
        r#ref: Option<String>,
    },
}

/// The limits a server announces in `welcome` (spec 7.1, 9).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Limits {
    #[serde(default)]
    pub max_update_hz: u32,
    #[serde(default)]
    pub rider_timeout_s: u32,
    #[serde(default)]
    pub max_riders: u32,
    #[serde(default)]
    pub max_subscriptions: u32,
    #[serde(default)]
    pub max_message_bytes: u32,
}

/// A rider in `snapshot` or `joined` (spec 7.2, 7.3).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ServerRider {
    pub id: String,
    #[serde(default)]
    pub app: String,
    pub name: String,
    pub lap: u32,
    pub distance: f64,
    pub speed: f64,
    #[serde(default)]
    pub power: Option<u32>,
    #[serde(default)]
    pub cadence: Option<u32>,
}

/// A rider in `updated` (spec 7.4): the app and name were sent when the rider joined.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ServerRiderUpdate {
    pub id: String,
    pub lap: u32,
    pub distance: f64,
    pub speed: f64,
    #[serde(default)]
    pub power: Option<u32>,
    #[serde(default)]
    pub cadence: Option<u32>,
}

/// Messages a server sends (spec 7). Unknown types are ignored, as spec 3.2 asks.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        #[serde(default)]
        protocol: String,
        #[serde(default)]
        connection: String,
        #[serde(default)]
        limits: Limits,
    },
    Snapshot {
        route: String,
        room: String,
        riders: Vec<ServerRider>,
    },
    Joined {
        route: String,
        room: String,
        rider: ServerRider,
    },
    Updated {
        route: String,
        room: String,
        riders: Vec<ServerRiderUpdate>,
    },
    Left {
        route: String,
        room: String,
        id: String,
        reason: String,
    },
    Error {
        code: String,
        message: String,
        #[serde(default)]
        fatal: bool,
    },
    Pong {
        #[serde(default)]
        r#ref: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

/// A display name as spec 6.3.1 wants it: no control, format, private-use, line or paragraph
/// characters (except U+200D, which emoji sequences need), at most 32 characters, and never
/// only whitespace. Receivers show the result as plain text.
pub fn clean_name(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = 0;
    for c in raw.chars() {
        if c == '\u{200D}' || !forbidden(c) {
            if chars == 32 {
                break;
            }
            out.push(c);
            chars += 1;
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        "Rider".into()
    } else {
        trimmed.to_string()
    }
}

/// Unicode general categories Cc, Cf, Co, Zl and Zp, minus U+200D (handled by the caller).
fn forbidden(c: char) -> bool {
    matches!(
        c,
        '\u{0000}'..='\u{001F}'
            | '\u{007F}'..='\u{009F}'
            | '\u{00AD}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{13430}'..='\u{1343F}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
            | '\u{E000}'..='\u{F8FF}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_both_directions_of_the_track_and_nothing_else() {
        assert_eq!(shared_route("herne-hill"), Some("herne-hill-450"));
        assert_eq!(shared_route("herne-hill-clockwise"), Some("herne-hill-450-clockwise"));
        assert_eq!(shared_route("demo"), None);
        assert_eq!(shared_route("herne-hill-900"), None);
    }

    #[test]
    fn counts_laps_the_way_the_protocol_does() {
        assert_eq!(lap(1), 0);
        assert_eq!(lap(4), 3);
        assert_eq!(lap(0), 0);
    }

    #[test]
    fn keeps_a_position_inside_the_route() {
        assert_eq!(position(-1.0, 450.0), 0.0);
        assert_eq!(position(450.0, 450.0), 449.99);
        assert!((position(12.3, 450.0) - 12.3).abs() < 1e-9);
    }

    #[test]
    fn cleans_display_names() {
        assert_eq!(clean_name("  Anna  "), "Anna");
        assert_eq!(clean_name("A\u{202E}nn\u{200B}a"), "Anna");
        assert_eq!(clean_name("👩\u{200D}🚴"), "👩\u{200D}🚴");
        assert_eq!(clean_name("\u{202E}\u{200B}"), "Rider");
        assert_eq!(clean_name("   "), "Rider");
        assert_eq!(clean_name(&"a".repeat(40)), "a".repeat(32));
    }

    #[test]
    fn writes_the_messages_the_spec_shows() {
        let hello = serde_json::to_value(ClientMessage::Hello { app: app_info() }).unwrap();
        assert_eq!(hello["type"], "hello");
        assert_eq!(hello["app"]["id"], "velopopuli");
        assert_eq!(hello["app"]["name"], "VeloPopuli");

        let join = serde_json::to_value(ClientMessage::Join {
            rider: "vp-1".into(),
            route: "herne-hill-450".into(),
            room: "public".into(),
            name: "Anna".into(),
            lap: 0,
            distance: 0.0,
            speed: 0.0,
            power: None,
            cadence: None,
        })
        .unwrap();
        assert_eq!(join["type"], "join");
        assert_eq!(join["route"], "herne-hill-450");
        assert!(join.get("power").is_none());

        let update =
            serde_json::to_value(ClientMessage::Update { riders: vec![ClientRider { rider: "vp-1".into(), lap: 3, distance: 112.4, speed: 11.8, power: Some(245), cadence: Some(92) }] }).unwrap();
        assert_eq!(update["type"], "update");
        assert_eq!(update["riders"][0]["lap"], 3);
        assert_eq!(update["riders"][0]["power"], 245);

        let leave = serde_json::to_value(ClientMessage::Leave { rider: "vp-1".into() }).unwrap();
        assert_eq!(leave, serde_json::json!({ "type": "leave", "rider": "vp-1" }));

        let ping = serde_json::to_value(ClientMessage::Ping { r#ref: Some("hb-17".into()) }).unwrap();
        assert_eq!(ping, serde_json::json!({ "type": "ping", "ref": "hb-17" }));
    }

    #[test]
    fn reads_the_server_messages_and_ignores_unknown_ones() {
        let welcome: ServerMessage = serde_json::from_str(
            r#"{"type":"welcome","protocol":"0.2","connection":"c9","limits":{"max_update_hz":4,"rider_timeout_s":10,"max_riders":200,"max_subscriptions":16,"max_message_bytes":65536}}"#,
        )
        .unwrap();
        assert!(matches!(welcome, ServerMessage::Welcome { ref protocol, .. } if protocol == "0.2"));

        let snapshot: ServerMessage = serde_json::from_str(
            r#"{"type":"snapshot","route":"herne-hill-450","room":"public","riders":[{"id":"r7Kx2","app":"openradlr","name":"Ben","lap":5,"distance":201.3,"speed":12.1,"power":260}]}"#,
        )
        .unwrap();
        let ServerMessage::Snapshot { riders, .. } = snapshot else { panic!("not a snapshot") };
        assert_eq!(riders[0].app, "openradlr");
        assert_eq!(riders[0].name, "Ben");

        let updated: ServerMessage = serde_json::from_str(r#"{"type":"updated","route":"herne-hill-450","room":"public","riders":[{"id":"r7Kx2","lap":5,"distance":213.4,"speed":12.1}]}"#).unwrap();
        let ServerMessage::Updated { riders, .. } = updated else { panic!("not an update") };
        assert_eq!(riders[0].id, "r7Kx2");

        let left: ServerMessage = serde_json::from_str(r#"{"type":"left","route":"herne-hill-450","room":"public","id":"r7Kx2","reason":"timeout"}"#).unwrap();
        assert!(matches!(left, ServerMessage::Left { ref id, .. } if id == "r7Kx2"));

        let error: ServerMessage = serde_json::from_str(r#"{"type":"error","code":"unknown_route","message":"no","fatal":false}"#).unwrap();
        assert!(matches!(error, ServerMessage::Error { ref code, .. } if code == "unknown_route"));

        let pong: ServerMessage = serde_json::from_str(r#"{"type":"pong","ref":"hb-17"}"#).unwrap();
        assert!(matches!(pong, ServerMessage::Pong { .. }));

        let unknown: ServerMessage = serde_json::from_str(r#"{"type":"something_new","value":1}"#).unwrap();
        assert_eq!(unknown, ServerMessage::Unknown);
    }
}
