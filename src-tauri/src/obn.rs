//! OpenBikeNet: one connection that shares this rider with other indoor cycling apps.
//!
//! The messages, name rules and route mapping are pure and live in `velopopuli_core::obn`.
//! This module owns the socket: connect, say hello, subscribe to the shared track, join, send
//! about one update a second, and bring the other apps' riders back. It reconnects with backoff
//! and treats a silent server as gone, as spec 3.5 asks.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use velopopuli_core::obn::{app_info, clean_name, position, ClientMessage, ClientRider, ServerMessage};

/// Colours for other apps' riders. Known apps keep one colour; anything else gets a stable one.
const COLORS: [u32; 8] = [0x7dff8a, 0x3ec6ff, 0xffc93e, 0xff7ad9, 0xb38bff, 0xff9f43, 0x2ee6c5, 0xf5f5f5];

pub enum Update {
    Rider { id: String, name: String, app: String, distance: f64, speed: f64 },
    Left(String),
}

enum Out {
    Message(String),
    Leave,
}

enum Outcome {
    Stopped,
    Retry { reason: String, connected: bool },
}

pub struct OBNLink {
    out: mpsc::UnboundedSender<Out>,
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
        let (tx, rx) = mpsc::unbounded_channel();
        let link = Arc::new(OBNLink {
            out: tx,
            updates: Mutex::default(),
            status: Mutex::new("OpenBikeNet · connecting…".into()),
            stopped: AtomicBool::new(false),
            rider,
            route,
            length,
            name: clean_name(name),
            share_power,
        });
        tokio::spawn(run(link.clone(), server.to_string(), rx));
        link
    }

    /// `distance` is the position on the track; `lap` counts laps finished from 0.
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
        if let Ok(text) = serde_json::to_string(&ClientMessage::Update { riders: vec![rider] }) {
            let _ = self.out.send(Out::Message(text));
        }
    }

    pub fn take_updates(&self) -> Vec<Update> {
        std::mem::take(&mut *self.updates.lock().unwrap())
    }

    pub fn status(&self) -> String {
        self.status.lock().unwrap().clone()
    }

    pub fn leave(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.out.send(Out::Leave);
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

async fn run(link: Arc<OBNLink>, url: String, mut rx: mpsc::UnboundedReceiver<Out>) {
    let mut backoff = Duration::from_secs(1);
    loop {
        if link.stopped.load(Ordering::SeqCst) {
            return;
        }
        match connect_and_ride(&link, &url, &mut rx).await {
            Outcome::Stopped => return,
            Outcome::Retry { reason, connected } => {
                if link.stopped.load(Ordering::SeqCst) {
                    return;
                }
                if connected {
                    backoff = Duration::from_secs(1);
                }
                *link.status.lock().unwrap() = format!("OpenBikeNet · {reason}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(60));
            }
        }
    }
}

async fn connect_and_ride(link: &Arc<OBNLink>, url: &str, rx: &mut mpsc::UnboundedReceiver<Out>) -> Outcome {
    let set = |s: String| *link.status.lock().unwrap() = s;
    set("OpenBikeNet · connecting…".into());
    let ws = match tokio::time::timeout(Duration::from_secs(8), tokio_tungstenite::connect_async(url)).await {
        Ok(Ok((ws, _))) => ws,
        Ok(Err(e)) => return Outcome::Retry { reason: format!("could not connect: {e}"), connected: false },
        Err(_) => return Outcome::Retry { reason: "could not connect (timed out)".into(), connected: false },
    };
    let (mut sink, mut source) = ws.split();

    let hello = ClientMessage::Hello { app: app_info() };
    let subscribe = ClientMessage::Subscribe { route: link.route.to_string(), room: "public".into() };
    let join =
        ClientMessage::Join { rider: link.rider.clone(), route: link.route.to_string(), room: "public".into(), name: link.name.clone(), lap: 0, distance: 0.0, speed: 0.0, power: None, cadence: None };
    for message in [hello, subscribe, join] {
        let Ok(text) = serde_json::to_string(&message) else { continue };
        if sink.send(Message::text(text)).await.is_err() {
            return Outcome::Retry { reason: "lost the connection".into(), connected: false };
        }
    }

    let mut riders: HashMap<String, (String, String)> = HashMap::new();
    let mut last_rx = Instant::now();
    let mut pinged = false;
    let mut connected = false;
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Some(Out::Message(text)) => {
                    if sink.send(Message::text(text)).await.is_err() {
                        return Outcome::Retry { reason: "lost the connection".into(), connected };
                    }
                }
                Some(Out::Leave) | None => {
                    let leave = ClientMessage::Leave { rider: link.rider.clone() };
                    if let Ok(text) = serde_json::to_string(&leave) {
                        let _ = sink.send(Message::text(text)).await;
                    }
                    let _ = sink.close().await;
                    return Outcome::Stopped;
                }
            },
            msg = source.next() => {
                let Some(Ok(message)) = msg else {
                    return match msg {
                        None | Some(Err(_)) | Some(Ok(Message::Close(_))) => Outcome::Retry { reason: "lost the connection".into(), connected },
                        _ => continue,
                    };
                };
                // Any frame, including a WebSocket ping, shows the server is alive.
                last_rx = Instant::now();
                pinged = false;
                let Message::Text(text) = message else { continue };
                let Ok(message) = serde_json::from_str::<ServerMessage>(&text) else { continue };
                match message {
                    ServerMessage::Welcome { .. } => {
                        connected = true;
                        set("OpenBikeNet · connected".into());
                    }
                    ServerMessage::Snapshot { riders: list, .. } => {
                        for rider in list {
                            let app = rider.app.clone();
                            let name = clean_name(&rider.name);
                            riders.insert(rider.id.clone(), (app.clone(), name.clone()));
                            link.updates.lock().unwrap().push(Update::Rider { id: rider.id, name, app, distance: rider.distance, speed: rider.speed });
                        }
                        set(count_status(riders.len()));
                    }
                    ServerMessage::Joined { rider, .. } => {
                        let app = rider.app.clone();
                        let name = clean_name(&rider.name);
                        riders.insert(rider.id.clone(), (app.clone(), name.clone()));
                        link.updates.lock().unwrap().push(Update::Rider { id: rider.id, name, app, distance: rider.distance, speed: rider.speed });
                        set(count_status(riders.len()));
                    }
                    ServerMessage::Updated { riders: list, .. } => {
                        for rider in list {
                            if let Some((app, name)) = riders.get(&rider.id) {
                                link.updates.lock().unwrap().push(Update::Rider { id: rider.id.clone(), name: name.clone(), app: app.clone(), distance: rider.distance, speed: rider.speed });
                            }
                        }
                    }
                    ServerMessage::Left { id, .. } => {
                        riders.remove(&id);
                        link.updates.lock().unwrap().push(Update::Left(id));
                        set(count_status(riders.len()));
                    }
                    ServerMessage::Error { message, fatal, .. } => {
                        set(format!("OpenBikeNet · {message}"));
                        if fatal {
                            return Outcome::Retry { reason: "the server refused the connection".into(), connected };
                        }
                    }
                    ServerMessage::Pong { .. } | ServerMessage::Unknown => {}
                }
            },
            _ = tick.tick() => {
                if last_rx.elapsed() > Duration::from_secs(30) {
                    return Outcome::Retry { reason: "the server stopped answering".into(), connected };
                }
                if last_rx.elapsed() > Duration::from_secs(20) && !pinged {
                    let ping = ClientMessage::Ping { r#ref: Some("hb".into()) };
                    if let Ok(text) = serde_json::to_string(&ping) {
                        if sink.send(Message::text(text)).await.is_err() {
                            return Outcome::Retry { reason: "lost the connection".into(), connected };
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
}
