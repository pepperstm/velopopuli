# Devices

Trainers, power meters, heart-rate straps, cadence and speed sensors, and handlebar controllers, over Bluetooth and ANT+.

- **Protocols** (encoding and decoding bytes) are in [`crates/core/src/devices`](../../crates/core/src/devices), with no I/O, and tested against hand-built packets in `crates/core/tests/devices.rs`.
- **Transports** (the radio, the USB stick) are in [`src-tauri/src/devices`](../../src-tauri/src/devices).

## The hub

Every connected device reports `Readings` (power, cadence, heart rate, wheel speed, any of them optional) to one registry, `Devices`, built on `Hub` ([`devices/mod.rs`](../../crates/core/src/devices/mod.rs)). The ride asks it for one merged `Telemetry` every tick.

**Most direct wins.** For each metric the freshest reading (under 4 s old) from the highest-ranked device counts:

| Metric | First choice | Then | Then |
|---|---|---|---|
| Power | Power meter | Trainer | |
| Cadence | Cadence sensor | Power meter | Trainer |
| Heart rate | Heart-rate strap | Trainer (relayed) | |

So a crank power meter beats the trainer's own estimate, and a chest strap beats a heart rate the trainer passes on. With **Estimate power from a speed sensor** on and nothing measuring power, wheel speed becomes power through a fluid-trainer curve (see [Physics](physics-and-trainer-control.md#power-without-a-power-meter)). Cadence and speed sensors send cumulative revolution counts with event times. `RevCounter` turns them into rpm, coping with wrap-round and repeated events. Wheel speed assumes a 700×25c wheel (2.105 m) unless the rider sets another size.

Commands go to **the first connected trainer that accepts them** (`Hub::trainer`).

Each device is a `DeviceInfo`: `id`, `name`, `transport` (Ble, Ant, Sim), `kind` (Trainer, Power, Hr, Cadence, Speed, Controller), `status` (Connecting, Connected, Searching, Disconnected) and whether it's `controllable`. The page gets the list with live readings through `devices:changed`.

## Bluetooth

[`src-tauri/src/devices/ble.rs`](../../src-tauri/src/devices/ble.rs), on [btleplug](https://github.com/deviceplug/btleplug): CoreBluetooth on macOS, WinRT on Windows, BlueZ on Linux.

### Searching

`scan_start` scans for devices advertising a cycling service (FTMS 0x1826, Cycling Power 0x1818, Heart Rate 0x180D, Speed and Cadence 0x1816), Tacx's FE-C service, or OpenBikeControl, and streams what it hears to the page as `ble:candidates`. Connecting stops the search.

### Connecting

`open_link` connects, discovers the services, works out what the device is, and subscribes:

| Found | Kind | Used for |
|---|---|---|
| FTMS Indoor Bike Data (0x2AD2), or Tacx FE-C | Trainer | Power, cadence, speed; commands |
| Cycling Power Measurement (0x2A63) | Power | Power, and cadence from crank revolutions |
| Heart Rate Measurement (0x2A37) | Hr | Heart rate |
| CSC Measurement (0x2A5B) | Cadence or Speed | Crank or wheel revolutions |
| OpenBikeControl buttons, and nothing else | Controller | Button presses |

Buttons on a trainer or sensor are used too; only a device with nothing else is a controller. A trainer or sensor whose buttons fail to subscribe is kept for its measurements.

### Trainer control

- **FTMS** (the standard): subscribe to the control point (0x2AD9, which answers by indication), then *Request Control* (0x00) and *Start* (0x07; some trainers reject it when already started, which is fine). Then:
  - *Set Target Power* (0x05, watts as `i16`);
  - *Set Indoor Bike Simulation* (0x11: wind 0, gradient in 0.01% as `i16` clamped to ±40%, Crr in 0.0001, Cw in 0.01 kg/m).
  
  If taking control fails (some spin bikes are read-only FTMS), the device is still used for its power and cadence.
- **Tacx FE-C over Bluetooth** (older Tacx trainers without FTMS): ANT+ FE-C pages, each wrapped as a whole ANT acknowledged-data message, written to Tacx's own service (6e40fec1-…). It's only used when there's no FTMS. Pages: 49 (target power, in 0.25 W), 50 (wind resistance), 51 (gradient and Crr), 55 (rider and bike mass).

With detailed logging on (the default), every write and every notification goes to the diagnostics log with its bytes (`ble:tx`, `ble:rx`). A failed write is logged either way.

### Staying connected

Each connection is a tokio task (`pump`). The link is checked every 2 s. When it drops (a sensor sleeping, a trainer power-cycled) the task waits 3 s and reconnects, with 20 s per attempt, until the rider removes the device. The status is *Searching* in the meantime.

### Remembered sensors

The first time a sensor connects it's remembered in `devices.json` in the data folder (`{"ble":[{"id","name"}],"ant":false}`). At launch, `look_for_known` scans for up to 3 minutes and connects each remembered sensor as soon as it's heard, retrying a failed one after 15 s. The Devices page shows which are still being looked for (`looking_for`), and **Forget** (`device_forget`) stops it. Bluetooth ids are per computer: on macOS they're CoreBluetooth's own UUIDs, not the device's address.

### Linux: a quiet radio to connect

BlueZ often aborts a connection made while the radio is scanning ("le-connection-abort-by-local", "Service discovery timed out"). So on Linux (`QUIET_TO_CONNECT`), every connection goes through `Radio::connect`, which stops the scan first and starts it again once no connection is under way, if a search or the look for remembered sensors still wants it. CoreBluetooth and WinRT connect fine while scanning, so elsewhere the scan carries on.

## ANT+

[`src-tauri/src/devices/ant.rs`](../../src-tauri/src/devices/ant.rs): the USB stick driven directly through libusb ([rusb](https://github.com/a1ien/rusb), vendored), with no vendor SDK. It works with Dynastream/Garmin sticks (USB vendor 0x0FCF, products 0x1008, 0x1009 and 0x1004).

- **Setup:** reset the stick, set the ANT+ network key, then open one **wildcard channel per profile** on 57 (2457 MHz). The first sensor of each kind the stick hears becomes a device.

| Profile | Device type | Kind |
|---|---|---|
| FE-C (smart trainer) | 17 | Trainer |
| Bicycle power | 11 | Power |
| Heart rate | 120 | Hr |
| Speed and cadence | 121 | Cadence |
| Cadence | 122 | Cadence |
| Speed | 123 | Speed |

- **Framing** ([`devices/ant.rs`](../../crates/core/src/devices/ant.rs)): `SYNC, LENGTH, ID, PAYLOAD, XOR checksum`. `AntParser` reassembles messages from USB reads.
- **Data pages** ([`devices/profiles.rs`](../../crates/core/src/devices/profiles.rs)): 8 bytes each. `PageDecoder` keeps the state each needs: power from the torque pages' event counts, revolutions for cadence and speed.
- **Commands** to an FE-C trainer go as acknowledged messages, one at a time: the next is sent once the stick reports the last one's outcome, or after 2 s (`pump_ack`).
- **On Linux** the stick needs permission to be opened: the packages install a udev rule ([`resources/99-ant-usb.rules`](../../resources/99-ant-usb.rules)). **On Windows** it needs the WinUSB driver (installed with Zadig). macOS needs nothing.

Once ANT+ has been started (`ant_start`), it's remembered and started again at launch.

## Handlebar controllers

[OpenBikeControl](https://openbikecontrol.org) is an open protocol for handlebar controllers ([`devices/obc.rs`](../../crates/core/src/devices/obc.rs)). The button characteristic carries `[0x01, id, state, id, state, …]`. On connecting, the app writes its name and the button ids it uses to the app-info characteristic.

Each press becomes one `Action`: shift harder or easier, harder or easier (the ERG target or the workout's intensity), skip step, pause, resume, or camera. Where a controller sends more than one id for a press, the ride's state decides which counts (`Context`): whether virtual gears are on (with the bike's own gears, shift buttons do nothing), and whether the trainer is holding a power target (gears change nothing then, so difficulty counts). The action reaches the page as `controller:action`, which runs it like the matching key.

Bluetooth remotes and keyboards need nothing special: they send keys. On the ride screen:

| Key | Action |
|---|---|
| `]` `=` `Page Down` `→` | Shift harder (virtual gears; the rider can change these) |
| `[` `-` `Page Up` `←` | Shift easier |
| `↑` `↓` | A workout's intensity ±5%; in ERG, the target ±10 W; with the demo rider, its effort ±10 W |
| `Space` | Pause or resume |
| `C` | Change camera |

## The demo rider

[`src-tauri/src/devices/sim.rs`](../../src-tauri/src/devices/sim.rs): a simulated rider and smart trainer, for trying the app or developing it without hardware. It's a controllable trainer (`sim:demo`) reporting four times a second:

- **Power:** 190 W on the flat, pushing harder uphill and easing off downhill (from 0.4× to 1.45×), with a little wobble. In ERG it holds the target.
- **Cadence:** about 88 rpm, lower uphill.
- **Heart rate:** drifting towards `90 + 0.36 × power`.

The smoke harness rides with it.

## Adding a device type

1. Put the decoding (and any encoding) in `crates/core/src/devices`, with a test built from real captured bytes. A diagnostics report with detailed logging on has them (`ble:rx` lines).
2. Recognise it in `open_link` (Bluetooth) or add a profile to `ANT_PROFILES`.
3. If it's a new kind of reading, add it to `Readings` and give it a rank in `priority`.
4. Mark it as not tried with real hardware in the README's status table until someone has.
