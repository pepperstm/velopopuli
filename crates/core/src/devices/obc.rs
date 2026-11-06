//! OpenBikeControl (openbikecontrol.org): an open protocol for handlebar
//! controllers. The Bluetooth service carries `[0x01, id, state, id, state…]`
//! button messages; the app tells the controller who it is and which ids it uses.

use serde::Serialize;

pub const SVC_OBC: u128 = 0xd273f680_d548_419d_b9d1_fa0472345229;
pub const CHR_OBC_BUTTONS: u128 = 0xd273f681_d548_419d_b9d1_fa0472345229;
pub const CHR_OBC_APP_INFO: u128 = 0xd273f683_d548_419d_b9d1_fa0472345229;

const MSG_BUTTONS: u8 = 0x01;
const MSG_APP_INFO: u8 = 0x04;

pub const SHIFT_UP: u8 = 0x01;
pub const SHIFT_DOWN: u8 = 0x02;
pub const HARDER: u8 = 0x30;
pub const EASIER: u8 = 0x31;
pub const SKIP_INTERVAL: u8 = 0x32;
pub const PAUSE: u8 = 0x33;
pub const RESUME: u8 = 0x34;
pub const CAMERA: u8 = 0x40;

/// The button ids VeloPopuli acts on, sent to the controller after connecting.
pub const SUPPORTED: [u8; 8] = [SHIFT_UP, SHIFT_DOWN, HARDER, EASIER, SKIP_INTERVAL, PAUSE, RESUME, CAMERA];

/// What a controller press does on the ride screen; the same things the keys do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    ShiftHarder,
    ShiftEasier,
    /// ERG target up, or the workout harder.
    Harder,
    Easier,
    SkipStep,
    Pause,
    Resume,
    Camera,
}

/// What decides between ids sent for the same press.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Context {
    /// Virtual gears are on (with real gears, shift buttons do nothing).
    pub virtual_gears: bool,
    /// The trainer holds a power target, so gears change nothing and difficulty is what counts.
    pub erg: bool,
}

/// `(id, state)` pairs of a button state message; `None` for any other message.
pub fn parse_buttons(v: &[u8]) -> Option<Vec<(u8, u8)>> {
    if *v.first()? != MSG_BUTTONS {
        return None;
    }
    Some(v[1..].chunks_exact(2).map(|p| (p[0], p[1])).collect())
}

/// The App Information message: app id, version and the button ids used.
pub fn app_info(app_id: &str, version: &str) -> Vec<u8> {
    // The spec allows 32 bytes for each string.
    let (id, version) = (&app_id.as_bytes()[..app_id.len().min(32)], &version.as_bytes()[..version.len().min(32)]);
    let mut out = vec![MSG_APP_INFO, 0x01, id.len() as u8];
    out.extend_from_slice(id);
    out.push(version.len() as u8);
    out.extend_from_slice(version);
    out.push(SUPPORTED.len() as u8);
    out.extend_from_slice(&SUPPORTED);
    out
}

/// Turns button messages into presses. Messages only list what changed, so the
/// state of every id is kept. One physical button may send several ids at once
/// (shift up, increase difficulty and select, so every app finds one it knows):
/// all ids a message newly presses are one press, and one action is picked.
#[derive(Default)]
pub struct Buttons {
    held: Vec<u8>,
}

impl Buttons {
    /// The action for a button message, if it pressed anything VeloPopuli uses.
    pub fn update(&mut self, v: &[u8], ctx: Context) -> Option<Action> {
        let mut pressed = Vec::new();
        for (id, state) in parse_buttons(v)? {
            let was_held = self.held.contains(&id);
            if state == 0 {
                self.held.retain(|h| *h != id);
            } else if !was_held {
                self.held.push(id);
                pressed.push(id);
            }
        }
        pick(&pressed, ctx)
    }
}

fn first(pressed: &[u8], choices: &[(u8, Action)]) -> Option<Action> {
    choices.iter().find(|(id, _)| pressed.contains(id)).map(|(_, a)| *a)
}

/// The one action for the ids pressed together: shifting first, then difficulty,
/// then the rest; in ERG, difficulty before shifting.
pub fn pick(pressed: &[u8], ctx: Context) -> Option<Action> {
    let shift = if ctx.virtual_gears { first(pressed, &[(SHIFT_UP, Action::ShiftHarder), (SHIFT_DOWN, Action::ShiftEasier)]) } else { None };
    let difficulty = first(pressed, &[(HARDER, Action::Harder), (EASIER, Action::Easier)]);
    let (a, b) = if ctx.erg { (difficulty, shift) } else { (shift, difficulty) };
    a.or(b).or_else(|| first(pressed, &[(SKIP_INTERVAL, Action::SkipStep), (PAUSE, Action::Pause), (RESUME, Action::Resume), (CAMERA, Action::Camera)]))
}
