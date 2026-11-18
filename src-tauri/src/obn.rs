//! OpenBikeNet: one connection that shares this rider with other indoor cycling apps.
//!
//! The messages, name rules and route mapping are pure and live in `velopopuli_core::obn`.
//! This module owns the socket: connect, say hello, subscribe to the shared track, join, send
//! about one update a second, and bring the other apps' riders back. It reconnects with backoff
//! and treats a silent server as gone, as spec 3.5 asks.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use tokio::sync::{watch, Notify};
use tokio_tungstenite::tungstenite::Message;

use velopopuli_core::obn::{app_info, clean_name, position, refusal, ClientMessage, ClientRider, ServerMessage, ServerRider};

/// Colours for other apps' riders. Known apps keep one colour; anything else gets a stable one.
const COLORS: [u32; 8] = [0x7dff8a, 0x3ec6ff, 0xffc93e, 0xff7ad9, 0xb38bff, 0xff9f43, 0x2ee6c5, 0xf5f5f5];

/// The `ref` on `subscribe` and `join`. An `error` carrying it means the server refused this
/// rider a place on the track, which trying again won't change.
const PLACE_REF: &str = "place";

/// A connection that drops sooner than this after `welcome` doesn't reset the backoff, so a
/// server that accepts and then drops us straight away isn't asked again every second.
const STABLE: Duration = Duration::from_secs(30);

/// Ids are the server's, so they can't clash with each other, but they could clash with the
/// ride engine's own rivals (`ghost`, `bot0`, …). The session prefixes them with this.
pub const ID_PREFIX: &str = "obn:";

pub enum Update {
    Rider { id: String, name: String, app: String, distance: f64, speed: f64 },
    Left(String),
}

enum Outcome {
    Stopped,
    Retry {
        reason: String,
        stable: bool,
    },
    /// The server refused us for good: stop and show why.
    Refused(String),
}

/// Riders of other apps the engine knows about, by server id: (app, name).
type Riders = HashMap<String, (String, String)>;

pub struct OBNLink {
    /// Only the latest position: after an outage one update goes out, not a backlog.
    position: watch::Sender<Option<ClientRider>>,
    leave: Notify,
    updates: Mutex<Vec<Update>>,
    status: Mutex<String>,
    stopped: AtomicBool,
    rider: String,
    route: &'static str,
    length: f64,
    name: String,
    share_power: bool,
}

impl OBNLink {
    /// Starts a link to `server` for the shared route `route`. Nothing is sent until the link
    /// connects, and the rider can leave at any time with [`leave`](Self::leave).
    pub fn join(server: &str, route: &'static str, length: f64, name: &str, share_power: bool) -> Arc<OBNLink> {
        let rider = format!("vp-{:x}", crate::now_ms());
        // The receiver is made here, not in the task: one made later counts a position sent
        // before it as already seen.
        let (position, receiver) = watch::channel(None);
        let link = Arc::new(OBNLink {
            position,
            leave: Notify::new(),
            updates: Mutex::default(),
            status: Mutex::new("OpenBikeNet · connecting…".into()),
            stopped: AtomicBool::new(false),
            rider,
            route,
            length,
            name: clean_name(name),
            share_power,
        });
        tokio::spawn(run(link.clone(), server.to_string(), receiver));
        link
    }

    /// `distance` is the position on the track; `lap` counts laps finished from 0. Replaces
    /// any position not sent yet.
    pub fn send(&self, distance: f64, lap: u32, speed: f64, power: f64, cadence: Option<f64>) {
        let mut rider = ClientRider {
            rider: self.rider.clone(),
            lap,
            distance: (position(distance, self.length) * 10.0).round() / 10.0,
            speed: (speed.clamp(0.0, 30.0) * 100.0).round() / 100.0,
            power: None,
            cadence: None,
        };
        if self.share_power {
            if power > 0.0 {
                rider.power = Some(power.round().clamp(0.0, 3000.0) as u32);
            }
            if let Some(c) = cadence {
                rider.cadence = Some(c.round().clamp(0.0, 250.0) as u32);
            }
        }
        self.position.send_replace(Some(rider));
    }

    pub fn take_updates(&self) -> Vec<Update> {
        std::mem::take(&mut *self.updates.lock().unwrap())
    }

    pub fn status(&self) -> String {
        self.status.lock().unwrap().clone()
    }

    pub fn leave(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.leave.notify_one();
    }

    fn set_status(&self, status: String) {
        *self.status.lock().unwrap() = status;
    }

    fn push(&self, update: Update) {
        self.updates.lock().unwrap().push(update);
    }

    /// A rider from `snapshot` or `joined`. Our own rider never comes back: servers don't send
    /// it (spec 7), and if one did it would be the rider standing next to us.
    fn add_rider(&self, riders: &mut Riders, rider: ServerRider) {
        if rider.id == self.rider {
            return;
        }
        let (app, name) = (clean_name(&rider.app), clean_name(&rider.name));
        riders.insert(rider.id.clone(), (app.clone(), name.clone()));
        self.push(Update::Rider { id: rider.id, name, app, distance: rider.distance, speed: rider.speed });
    }

    /// Takes every known rider off the track, when we stop for good.
    fn clear(&self, riders: &mut Riders) {
        for (id, _) in riders.drain() {
            self.push(Update::Left(id));
        }
    }
}

/// The colour a rider of `app` is drawn in.
pub fn app_color(app: &str) -> u32 {
    match app {
        "openradlr" => COLORS[0],
        "incyclist" => COLORS[1],
        "velopopuli" => COLORS[2],
        _ => COLORS[app.bytes().map(usize::from).sum::<usize>() % COLORS.len()],
    }
}

async fn run(link: Arc<OBNLink>, url: String, mut position: watch::Receiver<Option<ClientRider>>) {
    // Known across reconnects, so riders who left while we were offline can be taken away.
    let mut riders = Riders::new();
    let mut backoff = Duration::from_secs(1);
    loop {
        if link.stopped.load(Ordering::SeqCst) {
            return;
        }
        match connect_and_ride(&link, &url, &mut position, &mut riders).await {
            Outcome::Stopped => return,
            Outcome::Refused(reason) => {
                link.clear(&mut riders);
                link.set_status(format!("OpenBikeNet · {reason}"));
                return;
            }
            Outcome::Retry { reason, stable } => {
                if stable {
                    backoff = Duration::from_secs(1);
                }
                link.set_status(format!("OpenBikeNet · {reason}"));
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = link.leave.notified() => return,
                }
                backoff = (backoff * 2).min(Duration::from_secs(60));
            }
        }
    }
}

async fn connect_and_ride(link: &Arc<OBNLink>, url: &str, position: &mut watch::Receiver<Option<ClientRider>>, riders: &mut Riders) -> Outcome {
    let lost = |welcomed: Option<Instant>| Outcome::Retry { reason: "lost the connection".into(), stable: welcomed.is_some_and(|t| t.elapsed() >= STABLE) };
    link.set_status("OpenBikeNet · connecting…".into());
    let ws = tokio::select! {
        ws = tokio::time::timeout(Duration::from_secs(8), tokio_tungstenite::connect_async(url)) => match ws {
            Ok(Ok((ws, _))) => ws,
            Ok(Err(e)) => return Outcome::Retry { reason: format!("could not connect: {e}"), stable: false },
            Err(_) => return Outcome::Retry { reason: "could not connect (timed out)".into(), stable: false },
        },
        _ = link.leave.notified() => return Outcome::Stopped,
    };
    let (mut sink, mut source) = ws.split();

    let hello = ClientMessage::Hello { app: app_info() };
    let subscribe = ClientMessage::Subscribe { route: link.route.to_string(), room: "public".into(), r#ref: Some(PLACE_REF.into()) };
    let join = ClientMessage::Join {
        r#ref: Some(PLACE_REF.into()),
        rider: link.rider.clone(),
        route: link.route.to_string(),
        room: "public".into(),
        name: link.name.clone(),
        lap: 0,
        distance: 0.0,
        speed: 0.0,
        power: None,
        cadence: None,
    };
    for message in [hello, subscribe, join] {
        let Ok(text) = serde_json::to_string(&message) else { continue };
        if sink.send(Message::text(text)).await.is_err() {
            return lost(None);
        }
    }

    let mut welcomed: Option<Instant> = None;
    let mut last_rx = Instant::now();
    let mut pinged = false;
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            changed = position.changed() => {
                if changed.is_err() {
                    return Outcome::Stopped;
                }
                let Some(rider) = position.borrow_and_update().clone() else { continue };
                let Ok(text) = serde_json::to_string(&ClientMessage::Update { riders: vec![rider] }) else { continue };
                if sink.send(Message::text(text)).await.is_err() {
                    return lost(welcomed);
                }
            },
            _ = link.leave.notified() => {
                let leave = ClientMessage::Leave { rider: link.rider.clone() };
                if let Ok(text) = serde_json::to_string(&leave) {
                    let _ = sink.send(Message::text(text)).await;
                }
                let _ = sink.close().await;
                return Outcome::Stopped;
            },
            msg = source.next() => {
                let message = match msg {
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return lost(welcomed),
                    Some(Ok(message)) => message,
                };
                // Any frame, including a WebSocket ping, shows the server is alive.
                last_rx = Instant::now();
                pinged = false;
                let Message::Text(text) = message else { continue };
                let Ok(message) = serde_json::from_str::<ServerMessage>(&text) else { continue };
                match message {
                    ServerMessage::Welcome { .. } => {
                        welcomed = Some(Instant::now());
                        link.set_status("OpenBikeNet · connected".into());
                    }
                    ServerMessage::Snapshot { riders: list, .. } => {
                        // The whole room: anyone we still show who isn't in it left while we
                        // were away (after a reconnect every rider has a new id, spec 3.5).
                        let present: HashSet<&str> = list.iter().map(|r| r.id.as_str()).collect();
                        let gone: Vec<String> = riders.keys().filter(|id| !present.contains(id.as_str())).cloned().collect();
                        for id in gone {
                            riders.remove(&id);
                            link.push(Update::Left(id));
                        }
                        for rider in list {
                            link.add_rider(riders, rider);
                        }
                        link.set_status(count_status(riders.len()));
                    }
                    ServerMessage::Joined { rider, .. } => {
                        link.add_rider(riders, rider);
                        link.set_status(count_status(riders.len()));
                    }
                    ServerMessage::Updated { riders: list, .. } => {
                        for rider in list {
                            if let Some((app, name)) = riders.get(&rider.id) {
                                link.push(Update::Rider { id: rider.id.clone(), name: name.clone(), app: app.clone(), distance: rider.distance, speed: rider.speed });
                            }
                        }
                    }
                    ServerMessage::Left { id, .. } => {
                        if riders.remove(&id).is_some() {
                            link.push(Update::Left(id));
                        }
                        link.set_status(count_status(riders.len()));
                    }
                    ServerMessage::Error { code, message, fatal, r#ref } => {
                        // A fatal error closes the connection, and asking again gets the same
                        // answer; so does a refused `subscribe` or `join` (track full, unknown
                        // route, name not accepted). Anything else is about one update.
                        if fatal || r#ref.as_deref() == Some(PLACE_REF) {
                            let leave = ClientMessage::Leave { rider: link.rider.clone() };
                            if let Ok(text) = serde_json::to_string(&leave) {
                                let _ = sink.send(Message::text(text)).await;
                            }
                            let _ = sink.close().await;
                            return Outcome::Refused(refusal(&code, &message));
                        }
                    }
                    ServerMessage::Pong { .. } | ServerMessage::Unknown => {}
                }
            },
            _ = tick.tick() => {
                if last_rx.elapsed() > Duration::from_secs(30) {
                    return Outcome::Retry { reason: "the server stopped answering".into(), stable: welcomed.is_some_and(|t| t.elapsed() >= STABLE) };
                }
                if last_rx.elapsed() > Duration::from_secs(20) && !pinged {
                    let ping = ClientMessage::Ping { r#ref: Some("hb".into()) };
                    if let Ok(text) = serde_json::to_string(&ping) {
                        if sink.send(Message::text(text)).await.is_err() {
                            return lost(welcomed);
                        }
                    }
                    pinged = true;
                }
            }
        }
    }
}

fn count_status(riders: usize) -> String {
    if riders == 0 {
        "OpenBikeNet · no other riders".into()
    } else {
        format!("OpenBikeNet · {riders} rider{} from other apps", if riders == 1 { "" } else { "s" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;

    type Ws = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;

    async fn accept(listener: &TcpListener) -> Ws {
        let (stream, _) = listener.accept().await.unwrap();
        tokio_tungstenite::accept_async(stream).await.unwrap()
    }

    /// Reads `hello`, `subscribe` and `join`.
    async fn opening(ws: &mut Ws) {
        for _ in 0..3 {
            let Some(Ok(Message::Text(_))) = ws.next().await else { panic!("no opening message") };
        }
    }

    const WELCOME: &str = r#"{"type":"welcome","protocol":"0.2","connection":"c1","limits":{}}"#;

    async fn next_message(rx: &mut mpsc::UnboundedReceiver<String>) -> String {
        tokio::time::timeout(Duration::from_secs(2), rx.recv()).await.expect("timed out").expect("server closed")
    }

    async fn next_updates(link: &OBNLink) -> Vec<Update> {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let updates = link.take_updates();
                if !updates.is_empty() {
                    return updates;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("no updates")
    }

    #[tokio::test]
    async fn joins_shares_and_brings_riders_back() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            for _ in 0..3 {
                let Some(Ok(Message::Text(text))) = ws.next().await else { panic!("no opening message") };
                tx.send(text.to_string()).unwrap();
            }
            ws.send(Message::text(r#"{"type":"welcome","protocol":"0.2","connection":"c1","limits":{}}"#)).await.unwrap();
            ws.send(Message::text(r#"{"type":"snapshot","route":"herne-hill-450","room":"public","riders":[{"id":"r1","app":"openradlr","name":"Ben","lap":0,"distance":10.0,"speed":5.0}]}"#))
                .await
                .unwrap();
            let Some(Ok(Message::Text(text))) = ws.next().await else { panic!("no update") };
            tx.send(text.to_string()).unwrap();
            ws.send(Message::text(r#"{"type":"updated","route":"herne-hill-450","room":"public","riders":[{"id":"r1","lap":0,"distance":20.0,"speed":6.0}]}"#)).await.unwrap();
            ws.send(Message::text(r#"{"type":"left","route":"herne-hill-450","room":"public","id":"r1","reason":"leave"}"#)).await.unwrap();
            while let Some(Ok(Message::Text(text))) = ws.next().await {
                tx.send(text.to_string()).unwrap();
                if text.contains("\"leave\"") {
                    break;
                }
            }
            let _ = ws.close(None).await;
        });

        let link = OBNLink::join(&format!("ws://{addr}"), "herne-hill-450", 450.0, "Anna", false);
        let hello = next_message(&mut rx).await;
        assert!(hello.contains("\"hello\"") && hello.contains("velopopuli"), "{hello}");
        let subscribe = next_message(&mut rx).await;
        assert!(subscribe.contains("\"subscribe\"") && subscribe.contains("herne-hill-450"), "{subscribe}");
        let join = next_message(&mut rx).await;
        assert!(join.contains("\"join\"") && join.contains("Anna"), "{join}");

        let updates = next_updates(&link).await;
        assert!(matches!(&updates[0], Update::Rider { name, app, distance, .. } if name == "Ben" && app == "openradlr" && (*distance - 10.0).abs() < 1e-9));

        link.send(100.0, 2, 12.0, 250.0, Some(90.0));
        let update = next_message(&mut rx).await;
        assert!(update.contains("\"update\"") && update.contains("\"distance\":100.0"), "{update}");
        // Power and cadence stay private until the rider allows them.
        assert!(!update.contains("\"power\""), "{update}");

        let (mut moved, mut left) = (false, false);
        tokio::time::timeout(Duration::from_secs(2), async {
            while !(moved && left) {
                for update in link.take_updates() {
                    match update {
                        Update::Rider { distance, .. } if (distance - 20.0).abs() < 1e-9 => moved = true,
                        Update::Left(id) if id == "r1" => left = true,
                        _ => {}
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("no move and leave");

        link.leave();
        let leave = next_message(&mut rx).await;
        assert!(leave.contains("\"leave\""), "{leave}");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn takes_away_riders_who_left_while_offline() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut ws = accept(&listener).await;
            opening(&mut ws).await;
            ws.send(Message::text(WELCOME)).await.unwrap();
            ws.send(Message::text(r#"{"type":"snapshot","route":"herne-hill-450","room":"public","riders":[{"id":"r1","app":"openradlr","name":"Ben","lap":0,"distance":10.0,"speed":5.0},{"id":"r2","app":"incyclist","name":"Cleo","lap":0,"distance":20.0,"speed":5.0}]}"#)).await.unwrap();
            drop(ws);
            // Back again: Ben has gone, Cleo is still riding but has a new id, as spec 3.5 says.
            let mut ws = accept(&listener).await;
            opening(&mut ws).await;
            ws.send(Message::text(WELCOME)).await.unwrap();
            ws.send(Message::text(r#"{"type":"snapshot","route":"herne-hill-450","room":"public","riders":[{"id":"r3","app":"incyclist","name":"Cleo","lap":0,"distance":30.0,"speed":5.0}]}"#))
                .await
                .unwrap();
            while let Some(Ok(_)) = ws.next().await {}
        });

        let link = OBNLink::join(&format!("ws://{addr}"), "herne-hill-450", 450.0, "Anna", false);
        let mut shown: HashSet<String> = HashSet::new();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                for update in link.take_updates() {
                    match update {
                        Update::Rider { id, .. } => shown.insert(id),
                        Update::Left(id) => shown.remove(&id),
                    };
                }
                if shown.contains("r3") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("never came back");
        assert_eq!(shown, HashSet::from(["r3".to_string()]));
        link.leave();
        server.abort();
    }

    #[tokio::test]
    async fn stops_and_says_why_when_refused() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let link = OBNLink::join(&format!("ws://{addr}"), "herne-hill-450", 450.0, "Anna", false);
        let mut ws = accept(&listener).await;
        opening(&mut ws).await;
        ws.send(Message::text(WELCOME)).await.unwrap();
        ws.send(Message::text(r#"{"type":"snapshot","route":"herne-hill-450","room":"public","riders":[{"id":"r1","app":"openradlr","name":"Ben","lap":0,"distance":10.0,"speed":5.0}]}"#))
            .await
            .unwrap();
        ws.send(Message::text(r#"{"type":"error","code":"limit_exceeded","message":"max_riders reached","ref":"place","fatal":false}"#)).await.unwrap();
        // We say goodbye and close, and Ben, whom we can no longer follow, goes too.
        let mut said_leave = false;
        while let Some(Ok(Message::Text(text))) = ws.next().await {
            said_leave |= text.contains("\"leave\"");
        }
        assert!(said_leave);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(link.status(), "OpenBikeNet · the track is full (max_riders reached)");
        assert!(link.take_updates().iter().any(|u| matches!(u, Update::Left(id) if id == "r1")));
        // And we don't ask again: a retry would come after a second.
        assert!(tokio::time::timeout(Duration::from_millis(2500), listener.accept()).await.is_err(), "it tried again");
    }

    #[tokio::test]
    async fn sends_only_the_latest_position_after_waiting() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let link = OBNLink::join(&format!("ws://{addr}"), "herne-hill-450", 450.0, "Anna", false);
        // Positions pile up while the server hasn't answered yet.
        for d in [10.0, 20.0, 30.0, 40.0] {
            link.send(d, 0, 10.0, 0.0, None);
        }
        let mut ws = accept(&listener).await;
        opening(&mut ws).await;
        let Some(Ok(Message::Text(update))) = ws.next().await else { panic!("no update") };
        assert!(update.contains("\"distance\":40.0"), "{update}");
        assert!(tokio::time::timeout(Duration::from_millis(300), ws.next()).await.is_err(), "more than one update");
        link.leave();
    }
}
