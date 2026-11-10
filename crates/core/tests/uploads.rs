//! Ride upload records: one per connector, with rides from before connectors still readable.

use velopopuli_core::segments::{Segment, SegmentSource};
use velopopuli_core::types::{RideSummary, Settings, UploadRecord};

fn legacy_ride() -> RideSummary {
    // What a profile written by v0.3.x holds: a Strava upload as a bare activity id.
    serde_json::from_str(
        r#"{"id":"r1","routeId":"a","routeName":"Alpe","startedAt":1700000000000,"durationS":1,"distanceM":1,"ascentM":0,
            "avgPower":0,"maxPower":0,"np":0,"avgHr":0,"maxHr":0,"avgCadence":0,"avgSpeed":0,"kj":0,"tss":0,"completed":true,
            "isPb":false,"xp":0,"newBadges":[],"filePath":"/x.fit","stravaActivityId":42}"#,
    )
    .unwrap()
}

#[test]
fn legacy_strava_upload_is_still_found() {
    let ride = legacy_ride();
    assert!(ride.uploads.is_empty());
    let up = ride.upload("strava").expect("legacy id counts as an upload");
    assert_eq!(up.reference.as_deref(), Some("42"));
    assert_eq!(up.url.as_deref(), Some("https://www.strava.com/activities/42"));
    assert!(ride.upload("dreeve").is_none());
}

#[test]
fn migration_moves_legacy_upload_into_the_map_once() {
    let mut ride = legacy_ride();
    ride.migrate_uploads();
    assert!(ride.strava_activity_id.is_none());
    assert_eq!(ride.uploads["strava"].reference.as_deref(), Some("42"));
    // A record that is already there wins over the legacy field.
    ride.strava_activity_id = Some(7);
    ride.migrate_uploads();
    assert_eq!(ride.uploads["strava"].reference.as_deref(), Some("42"));
    let json = serde_json::to_string(&ride).unwrap();
    assert!(!json.contains("stravaActivityId"), "{json}");
    assert!(json.contains("\"uploads\":{\"strava\""), "{json}");
}

#[test]
fn uploads_round_trip_per_connector() {
    let mut ride = legacy_ride();
    ride.uploads.insert("dreeve".into(), UploadRecord { uploaded_at: 5, reference: None, url: None, message: Some("queued".into()) });
    let back: RideSummary = serde_json::from_str(&serde_json::to_string(&ride).unwrap()).unwrap();
    assert_eq!(back.upload("dreeve").unwrap().message.as_deref(), Some("queued"));
    assert_eq!(back.upload("strava").unwrap().reference.as_deref(), Some("42"));
}

#[test]
fn settings_from_before_connectors_still_load() {
    // A v0.3.x settings document: the single Strava toggle, no `autoUpload` list.
    let mut doc = serde_json::to_value(Settings::default()).unwrap();
    doc.as_object_mut().unwrap().remove("autoUpload");
    doc["stravaAutoUpload"] = serde_json::Value::Bool(true);
    let s: Settings = serde_json::from_value(doc).expect("old key is ignored, missing list defaults");
    assert!(s.auto_upload.is_empty(), "the page migrates the old toggle into the list");
}

#[test]
fn segments_saved_before_connectors_still_count_as_strava() {
    let seg: Segment = serde_json::from_str(r#"{"id":"strava-1","name":"Sprint","source":"strava","start":1.0,"end":2.0}"#).unwrap();
    assert_eq!(seg.source, SegmentSource::Remote);
    assert_eq!(seg.connector_id(), Some("strava"));
    let dreeve = Segment { connector: Some("dreeve".into()), ..seg.clone() };
    assert_eq!(dreeve.connector_id(), Some("dreeve"));
    let json = serde_json::to_string(&dreeve).unwrap();
    assert!(json.contains("\"source\":\"remote\"") && json.contains("\"connector\":\"dreeve\""), "{json}");
    let climb = Segment { source: SegmentSource::Climb, connector: None, ..seg };
    assert_eq!(climb.connector_id(), None);
}
