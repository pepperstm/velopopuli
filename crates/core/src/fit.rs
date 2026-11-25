//! Minimal FIT activity encoder. FIT is what Strava, Garmin Connect and most
//! other training platforms ingest most reliably.

#[derive(Debug, Clone)]
pub struct FitSample {
    /// Unix ms.
    pub time: u64,
    pub lat: f64,
    pub lon: f64,
    pub ele: f64,
    pub dist: f64,
    pub speed: f64,
    pub power: f64,
    pub hr: Option<f64>,
    pub cadence: Option<f64>,
    /// Watts the trainer was asked to hold; None when it wasn't holding a target (sim mode).
    pub target_power: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct FitSession {
    pub start_time: u64,
    pub end_time: u64,
    pub timer_s: f64,
    pub distance_m: f64,
    pub ascent_m: f64,
    pub descent_m: f64,
    pub avg_power: f64,
    pub max_power: f64,
    pub np: f64,
    pub avg_hr: f64,
    pub max_hr: f64,
    pub avg_cadence: f64,
    pub max_cadence: f64,
    pub max_speed: f64,
    pub kj: f64,
    /// Local time minus UTC, in seconds, for the activity's local timestamp.
    pub utc_offset_s: i64,
    /// The ride's laps, in order. Empty writes the whole ride as one lap.
    pub laps: Vec<FitLap>,
    /// The workout ridden, written with its steps so a lap can say which step it was.
    pub workout: Option<FitWorkout>,
}

/// Why a lap ended: FIT's `lap_trigger`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LapTrigger {
    /// The rider pressed Lap.
    Manual = 0,
    /// A workout step ended.
    Time = 1,
    /// The rider crossed the line of a loop.
    PositionLap = 4,
    /// The ride ended.
    SessionEnd = 7,
}

/// One lap: a run of samples and what was ridden in it.
#[derive(Debug, Clone, PartialEq)]
pub struct FitLap {
    /// Index of the lap's first sample, and one past its last.
    pub samples: std::ops::Range<usize>,
    pub timer_s: f64,
    pub distance_m: f64,
    pub ascent_m: f64,
    pub descent_m: f64,
    pub trigger: LapTrigger,
    /// The workout step ridden in this lap (index into `FitWorkout::steps`).
    pub workout_step: Option<usize>,
}

/// A workout, as FIT's `workout` and `workout_step` messages.
#[derive(Debug, Clone, PartialEq)]
pub struct FitWorkout {
    pub name: String,
    pub steps: Vec<FitWorkoutStep>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FitWorkoutStep {
    pub name: String,
    pub duration_s: f64,
    /// Lowest and highest target, as a percentage of FTP; None for a step with no target.
    pub power_pct: Option<(f64, f64)>,
}

#[derive(Clone, Copy, PartialEq)]
enum T {
    Enum,
    U8,
    U16,
    S32,
    U32,
    U32z,
}

impl T {
    /// (FIT base type id, size in bytes, "invalid" marker value)
    fn spec(self) -> (u8, usize, u32) {
        match self {
            T::Enum => (0x00, 1, 0xff),
            T::U8 => (0x02, 1, 0xff),
            T::U16 => (0x84, 2, 0xffff),
            T::S32 => (0x85, 4, 0x7fff_ffff),
            T::U32 => (0x86, 4, 0xffff_ffff),
            T::U32z => (0x8c, 4, 0),
        }
    }
}

type Field = (u8, T, Option<f64>);

/// Seconds between the Unix epoch and the FIT epoch (1989-12-31T00:00:00Z).
const FIT_EPOCH: i64 = 631_065_600;
const SEMICIRCLES: f64 = 2_147_483_648.0 / 180.0;

const CRC_TABLE: [u16; 16] = [0x0000, 0xcc01, 0xd801, 0x1400, 0xf001, 0x3c00, 0x2800, 0xe401, 0xa001, 0x6c00, 0x7800, 0xb401, 0x5000, 0x9c01, 0x8801, 0x4400];

pub fn fit_crc(bytes: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in bytes {
        let mut tmp = CRC_TABLE[(crc & 0xf) as usize];
        crc = ((crc >> 4) & 0x0fff) ^ tmp ^ CRC_TABLE[(b & 0xf) as usize];
        tmp = CRC_TABLE[(crc & 0xf) as usize];
        crc = ((crc >> 4) & 0x0fff) ^ tmp ^ CRC_TABLE[((b >> 4) & 0xf) as usize];
    }
    crc
}

/// A field written as raw bytes (strings, byte arrays): (number, base type, bytes).
type Raw<'a> = (u8, u8, &'a [u8]);
/// A developer field: (field number, developer data index, bytes).
type Dev<'a> = (u8, u8, &'a [u8]);
/// What a definition message says: global number, field (number, base type, size)s, dev field (number, size, index)s.
type Layout = (u16, Vec<(u8, u8, u8)>, Vec<(u8, u8, u8)>);

/// Identifies VeloPopuli's developer fields (developer_data_id.application_id): a random
/// UUID, d15c7f0c-de7b-4272-8c63-37ff17f724f6, in byte order. Never change it — readers
/// match on it to know which app's `target_power` they're looking at.
const APP_ID: [u8; 16] = [0xd1, 0x5c, 0x7f, 0x0c, 0xde, 0x7b, 0x42, 0x72, 0x8c, 0x63, 0x37, 0xff, 0x17, 0xf7, 0x24, 0xf6];

/// NUL-padded fixed-size string, cut at a character boundary if it's too long.
fn padded<const N: usize>(s: &str) -> [u8; N] {
    let mut end = s.len().min(N - 1);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = [0; N];
    out[..end].copy_from_slice(&s.as_bytes()[..end]);
    out
}

#[derive(Default)]
struct Writer {
    bytes: Vec<u8>,
    /// Layout last defined for each local message type.
    defined: [Option<Layout>; 8],
}

impl Writer {
    fn message(&mut self, local: u8, global: u16, fields: &[Field]) {
        self.message_ext(local, global, fields, &[], &[]);
    }

    /// `fields` come first, then `raw`, then the developer fields `dev`.
    fn message_ext(&mut self, local: u8, global: u16, fields: &[Field], raw: &[Raw], dev: &[Dev]) {
        let mut layout: Vec<(u8, u8, u8)> = fields.iter().map(|f| (f.0, f.1.spec().0, f.1.spec().1 as u8)).collect();
        layout.extend(raw.iter().map(|r| (r.0, r.1, r.2.len() as u8)));
        let dev_layout: Vec<(u8, u8, u8)> = dev.iter().map(|d| (d.0, d.2.len() as u8, d.1)).collect();
        let layout = (global, layout, dev_layout);
        if self.defined[local as usize].as_ref() != Some(&layout) {
            // Bit 0x20 of the header says the definition also lists developer fields.
            self.bytes.extend_from_slice(&[0x40 | if dev.is_empty() { 0 } else { 0x20 } | local, 0, 0]);
            self.bytes.extend_from_slice(&global.to_le_bytes());
            self.bytes.push(layout.1.len() as u8);
            for (num, base, size) in &layout.1 {
                self.bytes.extend_from_slice(&[*num, *size, *base]);
            }
            if !dev.is_empty() {
                self.bytes.push(dev.len() as u8);
                for (num, size, index) in &layout.2 {
                    self.bytes.extend_from_slice(&[*num, *size, *index]);
                }
            }
            self.defined[local as usize] = Some(layout);
        }
        self.bytes.push(local);
        for (_, ty, value) in fields {
            let (_, size, invalid) = ty.spec();
            let raw: u32 = match value {
                Some(v) if v.is_finite() => {
                    if *ty == T::S32 {
                        (v.round() as i32) as u32
                    } else {
                        v.round().clamp(0.0, u32::MAX as f64) as u32
                    }
                }
                _ => invalid,
            };
            self.bytes.extend_from_slice(&raw.to_le_bytes()[..size]);
        }
        for (_, _, b) in raw {
            self.bytes.extend_from_slice(b);
        }
        for (_, _, b) in dev {
            self.bytes.extend_from_slice(b);
        }
    }
}

fn ts(ms: u64) -> f64 {
    ((ms as f64 / 1000.0).round() as i64 - FIT_EPOCH) as f64
}

fn cap(v: f64, max: f64) -> Option<f64> {
    Some(v.clamp(0.0, max))
}

fn positive(v: f64, max: f64) -> Option<f64> {
    (v > 0.0).then(|| v.min(max))
}

/// What a lap's samples add up to.
struct LapStats {
    avg_power: f64,
    max_power: f64,
    np: Option<f64>,
    avg_hr: f64,
    max_hr: f64,
    avg_cadence: f64,
    max_cadence: f64,
    max_speed: f64,
}

fn lap_stats(samples: &[FitSample]) -> LapStats {
    let n = samples.len().max(1) as f64;
    let mean = |v: Vec<f64>| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
    let max = |v: &mut dyn Iterator<Item = f64>| v.fold(0.0_f64, f64::max);
    let hr: Vec<f64> = samples.iter().filter_map(|p| p.hr).filter(|h| *h > 0.0).collect();
    let cad: Vec<f64> = samples.iter().filter_map(|p| p.cadence).filter(|c| *c > 0.0).collect();
    // Normalised power needs 30 s of riding: the 4th-power mean of the 30 s rolling average.
    let np = (samples.len() >= 30).then(|| {
        let rolling: Vec<f64> = samples.windows(30).map(|w| w.iter().map(|p| p.power).sum::<f64>() / 30.0).collect();
        (rolling.iter().map(|m| m.powi(4)).sum::<f64>() / rolling.len() as f64).powf(0.25)
    });
    LapStats {
        avg_power: samples.iter().map(|p| p.power).sum::<f64>() / n,
        max_power: max(&mut samples.iter().map(|p| p.power)),
        np,
        avg_hr: mean(hr.clone()),
        max_hr: max(&mut hr.into_iter()),
        avg_cadence: mean(cad.clone()),
        max_cadence: max(&mut cad.into_iter()),
        max_speed: max(&mut samples.iter().map(|p| p.speed)),
    }
}

pub fn encode_fit(samples: &[FitSample], s: &FitSession) -> Vec<u8> {
    let mut w = Writer::default();
    let start = ts(s.start_time);
    let end = ts(s.end_time);
    let elapsed_ms = (end - start) * 1000.0;

    // file_id: type=activity, manufacturer=development
    w.message(0, 0, &[(0, T::Enum, Some(4.0)), (1, T::U16, Some(255.0)), (2, T::U16, Some(1.0)), (3, T::U32z, Some(start % 4_294_967_294.0 + 1.0)), (4, T::U32, Some(start))]);
    // developer_data_id + field_description: declares the per-record `target_power` developer
    // field (index 0, field 0). Readers that don't know it skip it by the sizes in the definition.
    w.message_ext(5, 207, &[(3, T::U8, Some(0.0))], &[(1, 0x0d, &APP_ID)], &[]);
    w.message_ext(
        6,
        206,
        &[(0, T::U8, Some(0.0)), (1, T::U8, Some(0.0)), (2, T::U8, Some(0x84 as f64)), (14, T::U16, Some(20.0))],
        &[(3, 0x07, &padded::<16>("target_power")), (8, 0x07, &padded::<8>("watts"))],
        &[],
    );
    if let Some(wk) = &s.workout {
        // workout: sport=cycling, and its steps, so each lap can point at the step it was.
        w.message_ext(7, 26, &[(4, T::Enum, Some(2.0)), (6, T::U16, Some(wk.steps.len() as f64))], &[(8, 0x07, &padded::<32>(&wk.name))], &[]);
        for (i, step) in wk.steps.iter().enumerate() {
            // Power targets above 1000 would be watts; up to 1000 they're a percentage of FTP.
            let (target_type, low, high) = match step.power_pct {
                Some((low, high)) => (4.0, Some(low.clamp(0.0, 1000.0)), Some(high.clamp(0.0, 1000.0))),
                None => (2.0, None, None),
            };
            w.message_ext(
                7,
                27,
                &[
                    (254, T::U16, Some(i as f64)),
                    (1, T::Enum, Some(0.0)),
                    (2, T::U32, Some(step.duration_s * 1000.0)),
                    (3, T::Enum, Some(target_type)),
                    (4, T::U32, low.map(|_| 0.0)),
                    (5, T::U32, low),
                    (6, T::U32, high),
                ],
                &[(0, 0x07, &padded::<32>(&step.name))],
                &[],
            );
        }
    }
    // event: timer start
    w.message(1, 21, &[(253, T::U32, Some(start)), (0, T::Enum, Some(0.0)), (1, T::Enum, Some(0.0))]);
    for p in samples {
        // 0xffff marks "no target", like the invalid value of any uint16.
        let target = p.target_power.map_or(0xffff, |v| v.round().clamp(0.0, 65534.0) as u16).to_le_bytes();
        w.message_ext(
            2,
            20,
            &[
                (253, T::U32, Some(ts(p.time))),
                (0, T::S32, Some(p.lat * SEMICIRCLES)),
                (1, T::S32, Some(p.lon * SEMICIRCLES)),
                (2, T::U16, cap((p.ele + 500.0) * 5.0, 65534.0)),
                (3, T::U8, p.hr.map(|v| v.clamp(0.0, 254.0))),
                (4, T::U8, p.cadence.map(|v| v.clamp(0.0, 254.0))),
                (5, T::U32, Some(p.dist * 100.0)),
                (6, T::U16, cap(p.speed * 1000.0, 65534.0)),
                (7, T::U16, cap(p.power, 65534.0)),
            ],
            &[],
            &[(0, 0, &target)],
        );
    }
    // event: timer stop_all
    w.message(1, 21, &[(253, T::U32, Some(end)), (0, T::Enum, Some(0.0)), (1, T::Enum, Some(4.0))]);
    let whole = [FitLap { samples: 0..samples.len(), timer_s: s.timer_s, distance_m: s.distance_m, ascent_m: s.ascent_m, descent_m: s.descent_m, trigger: LapTrigger::SessionEnd, workout_step: None }];
    let laps: &[FitLap] = if s.laps.is_empty() { &whole } else { &s.laps };
    for (i, lap) in laps.iter().enumerate() {
        let range = lap.samples.start.min(samples.len())..lap.samples.end.min(samples.len());
        let in_lap = &samples[range.clone()];
        let stats = lap_stats(in_lap);
        // A lap runs from its first sample to the next lap's first, so the laps add up to the ride.
        let lap_start = in_lap.first().map_or(start, |p| ts(p.time));
        let lap_end = samples.get(range.end).filter(|_| i + 1 < laps.len()).or(in_lap.last()).map_or(end, |p| ts(p.time));
        let (first, last) = (in_lap.first(), in_lap.last());
        w.message(
            3,
            19,
            &[
                (254, T::U16, Some(i as f64)),
                (253, T::U32, Some(lap_end)),
                (0, T::Enum, Some(9.0)),
                (1, T::Enum, Some(1.0)),
                (2, T::U32, Some(lap_start)),
                (3, T::S32, first.map(|p| p.lat * SEMICIRCLES)),
                (4, T::S32, first.map(|p| p.lon * SEMICIRCLES)),
                (5, T::S32, last.map(|p| p.lat * SEMICIRCLES)),
                (6, T::S32, last.map(|p| p.lon * SEMICIRCLES)),
                (7, T::U32, Some((lap_end - lap_start) * 1000.0)),
                (8, T::U32, Some(lap.timer_s * 1000.0)),
                (9, T::U32, Some(lap.distance_m * 100.0)),
                (13, T::U16, if lap.timer_s > 0.0 { cap(lap.distance_m / lap.timer_s * 1000.0, 65534.0) } else { None }),
                (14, T::U16, cap(stats.max_speed * 1000.0, 65534.0)),
                (15, T::U8, positive(stats.avg_hr, 254.0)),
                (16, T::U8, positive(stats.max_hr, 254.0)),
                (17, T::U8, positive(stats.avg_cadence, 254.0)),
                (18, T::U8, positive(stats.max_cadence, 254.0)),
                (19, T::U16, cap(stats.avg_power, 65534.0)),
                (20, T::U16, cap(stats.max_power, 65534.0)),
                (21, T::U16, cap(lap.ascent_m, 65534.0)),
                (22, T::U16, cap(lap.descent_m, 65534.0)),
                (24, T::Enum, Some(lap.trigger as u8 as f64)),
                (25, T::Enum, Some(2.0)),
                (33, T::U16, stats.np.and_then(|np| cap(np, 65534.0))),
                (39, T::Enum, Some(58.0)),
                (71, T::U16, lap.workout_step.map(|i| i as f64)),
            ],
        );
    }
    // session: sport=cycling, sub_sport=virtual_activity (Strava files it as a Virtual Ride)
    w.message(
        4,
        18,
        &[
            (254, T::U16, Some(0.0)),
            (253, T::U32, Some(end)),
            (0, T::Enum, Some(8.0)),
            (1, T::Enum, Some(1.0)),
            (2, T::U32, Some(start)),
            (5, T::Enum, Some(2.0)),
            (6, T::Enum, Some(58.0)),
            (7, T::U32, Some(elapsed_ms)),
            (8, T::U32, Some(s.timer_s * 1000.0)),
            (9, T::U32, Some(s.distance_m * 100.0)),
            (11, T::U16, cap(s.kj, 65534.0)),
            (14, T::U16, if s.timer_s > 0.0 { cap(s.distance_m / s.timer_s * 1000.0, 65534.0) } else { None }),
            (15, T::U16, cap(s.max_speed * 1000.0, 65534.0)),
            (16, T::U8, positive(s.avg_hr, 254.0)),
            (17, T::U8, positive(s.max_hr, 254.0)),
            (18, T::U8, positive(s.avg_cadence, 254.0)),
            (19, T::U8, positive(s.max_cadence, 254.0)),
            (20, T::U16, cap(s.avg_power, 65534.0)),
            (21, T::U16, cap(s.max_power, 65534.0)),
            (22, T::U16, cap(s.ascent_m, 65534.0)),
            (23, T::U16, cap(s.descent_m, 65534.0)),
            (25, T::U16, Some(0.0)),
            (26, T::U16, Some(laps.len() as f64)),
            (34, T::U16, cap(s.np, 65534.0)),
        ],
    );
    // activity
    w.message(
        5,
        34,
        &[
            (253, T::U32, Some(end)),
            (0, T::U32, Some(s.timer_s * 1000.0)),
            (1, T::U16, Some(1.0)),
            (2, T::Enum, Some(0.0)),
            (3, T::Enum, Some(26.0)),
            (4, T::Enum, Some(1.0)),
            (5, T::U32, Some(end + s.utc_offset_s as f64)),
        ],
    );

    let data = w.bytes;
    let mut out = Vec::with_capacity(data.len() + 16);
    out.extend_from_slice(&[14, 0x10]);
    out.extend_from_slice(&2132u16.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(b".FIT");
    let header_crc = fit_crc(&out);
    out.extend_from_slice(&header_crc.to_le_bytes());
    out.extend_from_slice(&data);
    let crc = fit_crc(&out);
    out.extend_from_slice(&crc.to_le_bytes());
    out
}
