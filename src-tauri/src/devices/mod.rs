//! Connected sensors: the shared registry plus one module per transport.

pub mod ant;
pub mod ble;
pub mod sim;

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use velopopuli_core::devices::obc;
use velopopuli_core::devices::{DeviceInfo, DeviceStatus, Hub, Readings, Telemetry, TrainerCommand};

use crate::{diag, now_ms};

/// How the registry reaches the task or thread that owns a device.
pub struct Handle {
    pub control: Option<Box<dyn Fn(TrainerCommand) + Send + Sync>>,
    pub stop: Box<dyn Fn() + Send + Sync>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AntStatus {
    /// "off", "opening", "scanning" or "error"
    pub state: &'static str,
    pub message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicesSnapshot {
    pub devices: Vec<DeviceInfo>,
    pub ant: AntStatus,
    /// Remembered Bluetooth sensors not connected yet, while the app looks for them.
    pub looking_for: Vec<KnownBle>,
}

/// A Bluetooth sensor connected before, to connect again at launch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnownBle {
    /// The operating system's id for the peripheral (without the `ble:` prefix).
    pub id: String,
    pub name: String,
}

/// The sensors to reconnect at launch, kept in the store as `devices`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Remembered {
    #[serde(default)]
    pub ble: Vec<KnownBle>,
    /// The ANT+ stick was on: start it again. ANT+ pairs whichever sensors are near, so there
    /// is no list of them to keep.
    #[serde(default)]
    pub ant: bool,
}

impl Remembered {
    /// Adds a sensor, or renames it if it is already there.
    pub fn add_ble(&mut self, id: &str, name: &str) {
        match self.ble.iter_mut().find(|k| k.id == id) {
            Some(k) => k.name = name.to_string(),
            None => self.ble.push(KnownBle { id: id.to_string(), name: name.to_string() }),
        }
    }

    pub fn forget_ble(&mut self, id: &str) -> bool {
        let before = self.ble.len();
        self.ble.retain(|k| k.id != id);
        self.ble.len() != before
    }
}

const REMEMBERED: &str = "devices";

pub struct Devices {
    app: AppHandle,
    hub: Mutex<Hub>,
    handles: Mutex<HashMap<String, Handle>>,
    ant_status: Mutex<AntStatus>,
    looking_for: Mutex<Vec<KnownBle>>,
}

impl Devices {
    pub fn new(app: AppHandle) -> Self {
        Self { app, hub: Mutex::default(), handles: Mutex::default(), ant_status: Mutex::new(AntStatus { state: "off", message: String::new() }), looking_for: Mutex::default() }
    }

    pub fn snapshot(&self) -> DevicesSnapshot {
        DevicesSnapshot { devices: self.hub.lock().unwrap().snapshot(now_ms()), ant: self.ant_status.lock().unwrap().clone(), looking_for: self.looking_for.lock().unwrap().clone() }
    }

    /// The sensors to reconnect at launch.
    pub fn remembered(&self) -> Remembered {
        self.app.state::<crate::AppState>().store.get(REMEMBERED).unwrap_or_default()
    }

    /// Changes the remembered sensors and saves them.
    pub fn change_remembered(&self, f: impl FnOnce(&mut Remembered)) {
        let store = &self.app.state::<crate::AppState>().store;
        let mut r: Remembered = store.get(REMEMBERED).unwrap_or_default();
        let before = r.clone();
        f(&mut r);
        if r != before {
            if let Err(e) = store.set(REMEMBERED, &r) {
                diag::event("dev", format!("could not save remembered devices: {e}"));
            }
        }
    }

    /// Which remembered sensors are being looked for, shown on the Devices page.
    pub fn set_looking_for(&self, pending: Vec<KnownBle>) {
        let mut current = self.looking_for.lock().unwrap();
        if *current != pending {
            *current = pending;
            drop(current);
            self.changed();
        }
    }

    /// Tells the UI the device list or a status changed.
    pub fn changed(&self) {
        let _ = self.app.emit("devices:changed", self.snapshot());
    }

    pub fn add(&self, info: DeviceInfo, handle: Handle) -> bool {
        let id = info.id.clone();
        diag::event("dev", format!("dev={id} name={:?} transport={:?} kind={:?} status={:?} added", info.name, info.transport, info.kind, info.status));
        if !self.hub.lock().unwrap().add(info) {
            return false;
        }
        self.handles.lock().unwrap().insert(id, handle);
        self.changed();
        true
    }

    pub fn contains(&self, id: &str) -> bool {
        self.hub.lock().unwrap().contains(id)
    }

    /// Stops the device's task and forgets it.
    pub fn remove(&self, id: &str) {
        // Take the handle out before calling it: `stop` may call back into the registry.
        let handle = self.handles.lock().unwrap().remove(id);
        diag::event("dev", format!("dev={id} removed"));
        if let Some(h) = handle {
            (h.stop)();
        }
        if self.hub.lock().unwrap().remove(id) {
            self.changed();
        }
    }

    pub fn report(&self, id: &str, readings: Readings) {
        self.hub.lock().unwrap().report(id, readings, now_ms());
    }

    pub fn set_status(&self, id: &str, status: DeviceStatus) {
        if self.hub.lock().unwrap().set_status(id, status) {
            diag::event("dev", format!("dev={id} status={status:?}"));
            self.changed();
        }
    }

    /// Updates what a device turned out to be once its services are known.
    pub fn update(&self, id: &str, f: impl FnOnce(&mut DeviceInfo)) {
        if let Some(d) = self.hub.lock().unwrap().get_mut(id) {
            f(d);
            diag::event("dev", format!("dev={id} kind={:?} controllable={} status={:?}", d.kind, d.controllable, d.status));
        }
        self.changed();
    }

    pub fn telemetry(&self, use_virtual_power: bool) -> Telemetry {
        self.hub.lock().unwrap().telemetry(use_virtual_power, now_ms())
    }

    pub fn is_empty(&self) -> bool {
        self.hub.lock().unwrap().is_empty()
    }

    pub fn has_trainer(&self) -> bool {
        self.hub.lock().unwrap().trainer().is_some()
    }

    /// Sends a command to the connected controllable trainer, if there is one.
    pub fn command_trainer(&self, cmd: TrainerCommand) {
        let Some(id) = self.hub.lock().unwrap().trainer().map(str::to_string) else {
            diag::raw("cmd", format!("no controllable trainer connected; not sent: {cmd:?}"));
            return;
        };
        if let Some(control) = self.handles.lock().unwrap().get(&id).and_then(|h| h.control.as_ref()) {
            control(cmd);
        }
    }

    /// What decides between the actions of a controller press; `None` outside a ride, where presses do nothing.
    pub fn controller_context(&self) -> Option<obc::Context> {
        self.app.state::<crate::AppState>().current_session().ok().map(|s| s.controller_context())
    }

    /// Hands a controller press to the ride screen, which runs it as it would the matching key.
    pub fn controller_action(&self, id: &str, action: obc::Action) {
        diag::raw("dev", format!("dev={id} press {action:?}"));
        let _ = self.app.emit("controller:action", action);
    }

    pub fn set_ant_status(&self, state: &'static str, message: impl Into<String>) {
        let message = message.into();
        diag::event("ant", format!("stick state={state} {message}"));
        *self.ant_status.lock().unwrap() = AntStatus { state, message };
        self.changed();
    }

    pub fn ant_state(&self) -> &'static str {
        self.ant_status.lock().unwrap().state
    }

    pub fn app(&self) -> &AppHandle {
        &self.app
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembered_sensors_are_kept_once_and_forgotten() {
        let mut r = Remembered::default();
        r.add_ble("A1", "Trainer");
        r.add_ble("B2", "HRM");
        r.add_ble("A1", "Tacx Neo");
        assert_eq!(r.ble.iter().map(|k| (k.id.as_str(), k.name.as_str())).collect::<Vec<_>>(), [("A1", "Tacx Neo"), ("B2", "HRM")]);
        assert!(r.forget_ble("A1") && !r.forget_ble("A1"));
        assert_eq!(r.ble.len(), 1);
        // Older stores have no list yet.
        let old: Remembered = serde_json::from_str("{}").unwrap();
        assert_eq!(old, Remembered::default());
    }
}
