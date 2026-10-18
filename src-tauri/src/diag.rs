//! Diagnostics log: what the sensors, trainer and ride did, for bug reports.
//!
//! Each run of the app writes `logs/diagnostics-<start time>.log` in the data folder.
//! Events (connections, drop-outs, errors, one ride line a second) are always logged;
//! the raw bytes every sensor sends are logged too while the "detailed diagnostics"
//! setting is on. `report` gathers the recent logs into one text file with a summary
//! at the top, ready to attach to a bug report.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Logs kept, newest first; older ones are deleted at start-up.
const KEEP_LOGS: usize = 12;
/// Raw sensor data stops being written once a log reaches this size (events carry on).
const RAW_LIMIT: u64 = 60 * 1024 * 1024;
/// How much log text a report includes at most.
const REPORT_BUDGET: usize = 12 * 1024 * 1024;

struct Log {
    dir: PathBuf,
    file: Mutex<Option<BufWriter<File>>>,
}

static LOG: OnceLock<Log> = OnceLock::new();
static DETAILED: AtomicBool = AtomicBool::new(true);
static WRITTEN: AtomicU64 = AtomicU64::new(0);

/// Opens this run's log in `<data dir>/logs` and removes old ones.
pub fn init(data_dir: &Path) {
    let dir = data_dir.join("logs");
    let _ = fs::create_dir_all(&dir);
    let mut old = logs(&dir);
    for stale in old.split_off(old.len().min(KEEP_LOGS - 1)) {
        let _ = fs::remove_file(stale);
    }
    let name = format!("diagnostics-{}.log", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let file = File::create(dir.join(name)).ok().map(BufWriter::new);
    if LOG.set(Log { dir, file: Mutex::new(file) }).is_ok() {
        event("app", format!("Velo Populi {} on {} {} ({})", env!("CARGO_PKG_VERSION"), std::env::consts::OS, os_version(), std::env::consts::ARCH));
    }
}

/// Turns logging of raw sensor data on or off (the setting).
pub fn set_detailed(on: bool) {
    if DETAILED.swap(on, Ordering::Relaxed) != on {
        event("app", format!("detailed diagnostics {}", if on { "on" } else { "off" }));
    }
}

pub fn detailed() -> bool {
    DETAILED.load(Ordering::Relaxed) && WRITTEN.load(Ordering::Relaxed) < RAW_LIMIT
}

fn write_line(category: &str, message: &str) {
    let Some(log) = LOG.get() else { return };
    let line = format!("{} [{category}] {message}\n", chrono::Local::now().format("%H:%M:%S%.3f"));
    WRITTEN.fetch_add(line.len() as u64, Ordering::Relaxed);
    if let Some(f) = log.file.lock().unwrap().as_mut() {
        let _ = f.write_all(line.as_bytes());
        // Flushed every line so a crash or force-quit loses nothing.
        let _ = f.flush();
    }
}

/// Something worth knowing happened: always logged.
pub fn event(category: &str, message: impl AsRef<str>) {
    write_line(category, message.as_ref());
}

/// Raw data: logged only while detailed diagnostics are on.
pub fn raw(category: &str, message: impl AsRef<str>) {
    if detailed() {
        write_line(category, message.as_ref());
    }
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Readings as `power=210 cad=85 spd=8.31 hr=-`, for log lines.
pub fn readings(r: &velopopuli_core::devices::Readings) -> String {
    let f = |v: Option<f64>, digits: usize| v.map_or_else(|| "-".to_string(), |v| format!("{v:.digits$}"));
    format!("power={} cad={} spd={} hr={}", f(r.power, 0), f(r.cadence, 0), f(r.wheel_speed, 2), f(r.hr, 0))
}

pub fn dir() -> Option<PathBuf> {
    LOG.get().map(|l| l.dir.clone())
}

/// Log files, newest first.
fn logs(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("diagnostics-") && n.ends_with(".log"))).collect())
        .unwrap_or_default();
    files.sort();
    files.reverse();
    files
}

fn os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("sw_vers").arg("-productVersion").output() {
            return String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    String::new()
}

// ── Reports ─────────────────────────────────────────────────────────────────

/// Writes a report to `path`: environment, settings and devices, an automatic
/// summary of the recent logs, then the logs themselves (newest first).
pub fn report(path: &Path, settings: &serde_json::Value, devices: &serde_json::Value, note: &str) -> Result<(), String> {
    let log = LOG.get().ok_or("Diagnostics are not running.")?;
    if let Some(f) = log.file.lock().unwrap().as_mut() {
        let _ = f.flush();
    }
    // Logs from the last two days, newest first, within the size budget.
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(48 * 3600);
    let mut texts: Vec<(String, String)> = Vec::new();
    let mut budget = REPORT_BUDGET;
    for file in logs(&log.dir) {
        if !texts.is_empty() && fs::metadata(&file).and_then(|m| m.modified()).is_ok_and(|t| t < cutoff) {
            break;
        }
        let Ok(text) = fs::read_to_string(&file) else { continue };
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        if text.len() > budget {
            // Too long: keep the start (connection set-up) and the end (most recent).
            let head = floor(&text, budget / 4);
            let tail = &text[ceil(&text, text.len() - (budget - head.len()))..];
            texts.push((name, format!("{head}\n… {} bytes left out …\n{tail}", text.len() - head.len() - tail.len())));
            break;
        }
        budget -= text.len();
        texts.push((name, text));
        if budget < 4096 {
            break;
        }
    }

    let mut out = String::new();
    let _ = writeln!(out, "VELO POPULI DIAGNOSTICS REPORT");
    let _ = writeln!(out, "Created {} · Velo Populi {} · {} {} ({})", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), env!("CARGO_PKG_VERSION"), std::env::consts::OS, os_version(), std::env::consts::ARCH);
    let _ = writeln!(out, "Contains device names and the data they sent, ride numbers and settings. No account details, tokens or locations.\n");
    if !note.trim().is_empty() {
        let _ = writeln!(out, "== What happened (from the rider)\n{}\n", note.trim());
    }
    let _ = writeln!(out, "== Summary (worked out from the logs below)");
    for (name, text) in &texts {
        let _ = writeln!(out, "\n-- {name}");
        out.push_str(&summarise(text));
    }
    let _ = writeln!(out, "\n== Devices now\n{}", serde_json::to_string_pretty(devices).unwrap_or_default());
    let _ = writeln!(out, "\n== Settings (account details removed)\n{}", serde_json::to_string_pretty(&redact(settings.clone())).unwrap_or_default());
    for (name, text) in &texts {
        let _ = writeln!(out, "\n== Log {name}\n{text}");
    }
    fs::write(path, out).map_err(|e| format!("Could not write the report: {e}"))?;
    event("app", format!("diagnostics report saved ({})", path.display()));
    Ok(())
}

fn floor(s: &str, mut i: usize) -> &str {
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    &s[..i]
}

fn ceil(s: &str, mut i: usize) -> usize {
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Drops anything that could identify an account or a place.
fn redact(mut v: serde_json::Value) -> serde_json::Value {
    const SECRET: [&str; 7] = ["token", "secret", "password", "clientid", "clientsecret", "mapillarytoken", "url"];
    if let Some(map) = v.as_object_mut() {
        for (k, val) in map.iter_mut() {
            let key = k.to_ascii_lowercase();
            if SECRET.iter().any(|s| key.contains(s)) && val.is_string() {
                *val = serde_json::Value::String(if val.as_str().is_some_and(str::is_empty) { String::new() } else { "(set)".into() });
            } else if key == "name" && val.is_string() {
                *val = serde_json::Value::String("(removed)".into());
            } else {
                *val = redact(val.take());
            }
        }
    }
    v
}

/// What happened to each device and ride in one log, from its lines.
fn summarise(text: &str) -> String {
    #[derive(Default)]
    struct Dev {
        name: String,
        connects: u32,
        drops: u32,
        rx: u64,
        first_rx: Option<f64>,
        last_rx: Option<f64>,
        longest_gap: (f64, f64),
        kinds: BTreeMap<String, u64>,
        errors: Vec<String>,
    }
    #[derive(Default)]
    struct Ride {
        seconds: u64,
        pedalling: u64,
        pedalling_no_power: u64,
        power_sum: f64,
        power_max: f64,
        cmds: u64,
    }
    let mut devs: BTreeMap<String, Dev> = BTreeMap::new();
    let mut rides: Vec<Ride> = Vec::new();
    let mut other_errors: Vec<String> = Vec::new();
    let mut first: Option<String> = None;
    let mut last = String::new();
    let mut day_offset = 0.0;
    let mut prev_t = 0.0;

    for line in text.lines() {
        let Some((stamp, rest)) = line.split_once(' ') else { continue };
        let Some(mut t) = seconds(stamp) else { continue };
        // Logs that run past midnight.
        if t + day_offset < prev_t - 3600.0 {
            day_offset += 86400.0;
        }
        t += day_offset;
        prev_t = t;
        first.get_or_insert_with(|| stamp.to_string());
        last = stamp.to_string();
        let Some((cat, msg)) = rest.strip_prefix('[').and_then(|r| r.split_once("] ")) else { continue };
        let fields = parse_fields(msg);
        let dev = fields.get("dev").map(|d| devs.entry(d.to_string()).or_default());
        match (cat, dev) {
            ("dev", Some(d)) => {
                if let Some(n) = fields.get("name") {
                    d.name = n.to_string();
                }
                match fields.get("status").copied() {
                    Some("Connected") => d.connects += 1,
                    Some("Searching") => d.drops += 1,
                    _ => {}
                }
                if msg.contains("error") || msg.contains("failed") {
                    d.errors.push(format!("{stamp} {msg}"));
                }
            }
            ("ble:rx" | "ant:rx", Some(d)) => {
                d.rx += 1;
                if let Some(prev) = d.last_rx {
                    if t - prev > d.longest_gap.0 {
                        d.longest_gap = (t - prev, prev);
                    }
                }
                d.first_rx.get_or_insert(t);
                d.last_rx = Some(t);
                let kind = fields.get("chr").or(fields.get("page")).map_or("?", |v| v);
                *d.kinds.entry(kind.to_string()).or_default() += 1;
            }
            ("ble:tx" | "ant:tx", Some(d)) => {
                if msg.contains("failed") {
                    d.errors.push(format!("{stamp} {msg}"));
                }
            }
            ("ride", _) if msg.starts_with("start") => rides.push(Ride::default()),
            ("ride", _) if msg.starts_with("t=") => {
                let r = match rides.last_mut() {
                    Some(r) => r,
                    None => {
                        rides.push(Ride::default());
                        rides.last_mut().unwrap()
                    }
                };
                let num = |k: &str| fields.get(k).and_then(|v| v.parse::<f64>().ok());
                r.seconds += 1;
                let power = num("power");
                // Pedalling: cadence from the ride, or any device seeing the cranks or the wheel turn.
                let device_moving = fields.get("devices").is_some_and(|d| {
                    d.split([' ', ',', ':']).any(|kv| match kv.split_once('=') {
                        Some(("cad", v)) => v.parse::<f64>().is_ok_and(|c| c > 20.0),
                        Some(("spd", v)) => v.parse::<f64>().is_ok_and(|s| s > 1.0),
                        _ => false,
                    })
                });
                let moving = num("cad").is_some_and(|c| c > 20.0) || device_moving;
                if moving {
                    r.pedalling += 1;
                    if power.is_none_or(|p| p < 1.0) {
                        r.pedalling_no_power += 1;
                    }
                }
                if let Some(p) = power {
                    r.power_sum += p;
                    r.power_max = r.power_max.max(p);
                }
                if fields.contains_key("cmd") {
                    r.cmds += 1;
                }
            }
            (_, _) if (msg.contains("error") || msg.contains("failed") || cat == "panic") && other_errors.len() < 20 => {
                other_errors.push(format!("{stamp} [{cat}] {msg}"));
            }
            _ => {}
        }
    }

    let mut out = String::new();
    let _ = writeln!(out, "Log runs {} → {}", first.as_deref().unwrap_or("?"), last);
    if devs.is_empty() {
        let _ = writeln!(out, "No sensors or trainers were connected.");
    }
    for (id, d) in &devs {
        let _ = writeln!(out, "Device {id} \"{}\": connected {}×, dropped {}×, {} messages", d.name, d.connects, d.drops, d.rx);
        if let (Some(a), Some(b)) = (d.first_rx, d.last_rx) {
            if b > a {
                let _ = writeln!(out, "  data for {:.0} s at {:.1} messages/s; longest silence {:.1} s (from {})", b - a, d.rx as f64 / (b - a), d.longest_gap.0, clock(d.longest_gap.1));
            }
        }
        if !d.kinds.is_empty() {
            let kinds: Vec<String> = d.kinds.iter().map(|(k, n)| format!("{k}×{n}")).collect();
            let _ = writeln!(out, "  message types: {}", kinds.join(", "));
        }
        for e in d.errors.iter().take(10) {
            let _ = writeln!(out, "  ! {e}");
        }
    }
    for (i, r) in rides.iter().enumerate() {
        let avg = if r.seconds > 0 { r.power_sum / r.seconds as f64 } else { 0.0 };
        let _ = writeln!(out, "Ride {}: {} s, pedalling {} s, of which {} s with no power reading; average {:.0} W, max {:.0} W; {} trainer commands", i + 1, r.seconds, r.pedalling, r.pedalling_no_power, avg, r.power_max, r.cmds);
    }
    for e in &other_errors {
        let _ = writeln!(out, "! {e}");
    }
    out
}

/// `12:34:56.789` → seconds since midnight.
fn seconds(stamp: &str) -> Option<f64> {
    let mut parts = stamp.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let s: f64 = parts.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

fn clock(t: f64) -> String {
    let t = t % 86400.0;
    format!("{:02}:{:02}:{:02}", (t / 3600.0) as u32, (t / 60.0) as u32 % 60, t as u32 % 60)
}

/// `key=value` pairs in a log message (values run to the next space; `name="…"` may hold spaces).
fn parse_fields(msg: &str) -> HashMap<&str, &str> {
    let mut out = HashMap::new();
    let mut rest = msg;
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].rsplit(' ').next().unwrap_or("");
        let after = &rest[eq + 1..];
        let (value, next) = if let Some(quoted) = after.strip_prefix('"') {
            match quoted.find('"') {
                Some(end) => (&quoted[..end], &quoted[end + 1..]),
                None => (quoted, ""),
            }
        } else {
            match after.find(' ') {
                Some(end) => (&after[..end], &after[end..]),
                None => (after, ""),
            }
        };
        if !key.is_empty() {
            out.insert(key, value);
        }
        rest = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_counts_drops_gaps_and_missing_power() {
        let log = "\
10:00:00.000 [dev] dev=ble:1 name=\"Tacx Vortex 123\" status=Connecting
10:00:01.000 [dev] dev=ble:1 status=Connected
10:00:01.250 [ble:rx] dev=ble:1 chr=fec2 hex=a4094e05 power=0 cad=-
10:00:01.500 [ble:rx] dev=ble:1 chr=fec2 hex=a4094e05 power=0 cad=-
10:00:09.500 [ble:rx] dev=ble:1 chr=2a63 hex=00 power=180 cad=-
10:00:10.000 [dev] dev=ble:1 status=Searching
10:00:12.000 [ble:tx] dev=ble:1 chr=fec3 hex=00 failed: not connected
10:00:13.000 [ride] start route_km=15.4 mode=sim
10:00:14.000 [ride] t=1 power=- cad=- spd=8.1 grade=4.2 cmd=sim:4.2% devices=ble:1:Connected:power=-,cad=-,spd=4.10,hr=-
10:00:15.000 [ride] t=2 power=210 cad=86 spd=8.2 grade=4.2 cmd=sim:4.2%
10:00:16.000 [ride] t=3 power=0 cad=0 spd=3.0 grade=4.2 devices=ble:1:Connected:power=0,cad=0,spd=0.00,hr=-
";
        let s = summarise(log);
        assert!(s.contains("\"Tacx Vortex 123\": connected 1×, dropped 1×, 3 messages"), "{s}");
        assert!(s.contains("longest silence 8.0 s (from 10:00:01)"), "{s}");
        assert!(s.contains("chr=2a63") || s.contains("2a63×1"), "{s}");
        assert!(s.contains("Ride 1: 3 s, pedalling 2 s, of which 1 s with no power reading"), "{s}");
        assert!(s.contains("2 trainer commands"), "{s}");
        assert!(s.contains("! 10:00:12.000 dev=ble:1 chr=fec3 hex=00 failed: not connected"), "{s}");
    }

    #[test]
    fn redact_removes_secrets_and_names() {
        let v = serde_json::json!({ "rider": { "name": "Graham", "ftp": 250 }, "strava": { "clientSecret": "abc", "clientId": "" }, "group": { "relayUrl": "ws://x" } });
        let r = redact(v);
        assert_eq!(r["rider"]["name"], "(removed)");
        assert_eq!(r["rider"]["ftp"], 250);
        assert_eq!(r["strava"]["clientSecret"], "(set)");
        assert_eq!(r["strava"]["clientId"], "");
        assert_eq!(r["group"]["relayUrl"], "(set)");
    }
}
