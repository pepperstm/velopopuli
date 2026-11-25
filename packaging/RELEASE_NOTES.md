**VeloPopuli: the people's indoor ride.** Ride real GPX routes in 3D on your smart trainer. Free and open source. [velopopuli.com](https://velopopuli.com) · [Guides](https://github.com/pepperstm/velopopuli/wiki)

> This is an early release. The 3D world, physics, workouts and progress tracking are well tested; real trainers and sensors are only now being tried, and the Windows and Linux builds have had little use. If something doesn't work, the app's **Report a problem → Report on GitHub** builds a diagnostics report for you. Please send one.

## What's new in 0.4.0

- **Overlay mode.** Press **O** or **Overlay** during a ride and the app shrinks to a see-through panel that floats over everything: power, speed, heart rate, cadence, the workout, the map and the gear buttons, on top of a video or a stream in another window. Drag it anywhere, and change how see-through it is. Press **O** again to go back.
- **A new look.** The app now has the glassy style of the website, called **Glass**. The flat look from before is still there as **Simple**, under **Settings → Rider → Theme**.
- **Sound.** A fanfare at the top of every climb and a grander one at the finish, bells for a personal best or a lap, three beeps before each workout step changes, and a soft warning if a sensor drops out. All made by the app, with no sound files. Set the volume or turn them off under **Settings → Sound**.
- **A bigger route finder.** Six routing styles (road bike, quiet roads, gravel, touring, safest, mountain bike), set for each part of the route. Drag the route to add a waypoint, undo and redo, reverse, close the loop or go out and back. **Make a loop** plans a round trip of the distance and direction you choose. A breakdown shows the surfaces and road types, the climbs, and how long the ride should take you.
- **Laps.** Press **Lap** (or `L`, or Lap on a handlebar controller) to mark a lap without stopping, and see its time and average power. Each workout step and each time round a loop is a lap too, so training sites show every interval on its own.
- **Target power in your rides.** In ERG and workouts, the `.FIT` file now records the power the trainer was asked to hold, so sites like Intervals.icu and GoldenCheetah can show it next to the power you rode. Thanks to david-uhlig for adding it.
- **Steep climbs.** On a very steep climb at low power, the rider now crawls up slowly instead of stalling at 0 km/h. Thanks to david-uhlig for reporting it.
- **AppImage updates.** On Linux, the AppImage can be updated with AppImageUpdate, which downloads only what has changed.
- **The group-ride relay on Docker.** Host group rides on a home server, a NAS or a Raspberry Pi with one command: `docker run -d --restart unless-stopped -p 47800:47800 ghcr.io/pepperstm/velopopuli-relay` (on a release candidate, add `:next` to the end). See [Group rides and self-hosting](https://github.com/pepperstm/velopopuli/wiki/Group-rides-and-self-hosting#running-a-relay-with-docker) in the wiki.

## Download

| System | File | Notes |
|---|---|---|
| **macOS** (Apple Silicon and Intel, 10.15+) | `VeloPopuli_*_universal.dmg` | Drag to Applications. Not notarised yet: the first time, open **System Settings → Privacy & Security → Open Anyway** (or run `xattr -dr com.apple.quarantine /Applications/VeloPopuli.app`). |
| **Windows** 10/11 (64-bit) | `VeloPopuli_*_x64-setup.exe` or `VeloPopuli_*_x64_en-US.msi` | Not code-signed yet: if SmartScreen warns, choose **More info → Run anyway**. ANT+ sticks need the WinUSB driver ([Zadig](https://zadig.akeo.ie)). |
| **Ubuntu / Debian** (and Mint, Pop!_OS…; 64-bit) | `VeloPopuli_*_amd64.deb` | `sudo apt install ./VeloPopuli_*_amd64.deb`. It installs the ANT+ stick permission rule too. |
| **Arch Linux** (and Manjaro, EndeavourOS…) | `velopopuli-bin-*-x86_64.pkg.tar.zst` | `sudo pacman -U velopopuli-bin-*.pkg.tar.zst`. The `PKGBUILD` is attached too. Install `bluez` for Bluetooth. |
| **Any other Linux** (64-bit) | `VeloPopuli_*_amd64.AppImage` | `chmod +x` and run. Later versions can be installed with AppImageUpdate, which downloads only what has changed. |

**Group-ride relay** (optional, for hosting rides with friends): `velopopuli-relay-*` for macOS, Windows and Linux; see [Group rides and self-hosting](https://github.com/pepperstm/velopopuli/wiki/Group-rides-and-self-hosting).

Setting up: [Installing](https://github.com/pepperstm/velopopuli/wiki/Installing) · [Connecting your trainer](https://github.com/pepperstm/velopopuli/wiki/Connecting-your-trainer) · [Your first ride](https://github.com/pepperstm/velopopuli/wiki/Your-first-ride)
