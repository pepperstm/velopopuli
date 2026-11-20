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

/// Identifies VeloPopuli's developer fields (developer_data_id.application_id).
const APP_ID: [u8; 16] = *b"VeloPopuli\0\0\0\0\0\0";

/// NUL-padded fixed-size string.
fn padded<const N: usize>(s: &str) -> [u8; N] {
    let mut out = [0; N];
    out[..s.len().min(N - 1)].copy_from_slice(&s.as_bytes()[..s.len().min(N - 1)]);
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
    let first = samples.first();
    // lap
    w.message(
        3,
        19,
        &[
            (254, T::U16, Some(0.0)),
            (253, T::U32, Some(end)),
            (0, T::Enum, Some(9.0)),
            (1, T::Enum, Some(1.0)),
            (2, T::U32, Some(start)),
            (3, T::S32, first.map(|p| p.lat * SEMICIRCLES)),
            (4, T::S32, first.map(|p| p.lon * SEMICIRCLES)),
            (7, T::U32, Some(elapsed_ms)),
            (8, T::U32, Some(s.timer_s * 1000.0)),
            (9, T::U32, Some(s.distance_m * 100.0)),
        ],
    );
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
            (26, T::U16, Some(1.0)),
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
