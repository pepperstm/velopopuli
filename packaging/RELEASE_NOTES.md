**VeloPopuli — the people's indoor ride.** Ride real GPX routes in 3D on your smart trainer. Free and open source. [velopopuli.com](https://velopopuli.com) · [Guides](https://github.com/pepperstm/velopopuli/wiki)

> This is an early release. The 3D world, physics, workouts and progress tracking are well tested; real trainers and sensors are only now being tried, and the Windows and Linux builds have had little use. If something doesn't work, the app's **Report a problem → Report on GitHub** builds a diagnostics report for you — please send one.

## What's new in 0.3.7

- **Roads cut into the hillside.** On a mountainside your road now sits on a level shelf, with a rocky cutting rising above it and a grassy embankment falling away below, instead of the slope simply blending into the road.
- **Side roads and railways too.** The other roads, tracks and railways around you are level across, cut into slopes and carried on embankments. Footpaths and driveways still follow the ground.
- **Field boundaries.** Open country is divided into fields: the hedges, walls and fences mapped near your route, and elsewhere fields in the style of the region — hedgerows with the odd tree in Britain, Ireland and north-west France, dry-stone walls on their hills and round the Mediterranean, timber fences on mountain pastures and in Scandinavia.
- **Holloways.** In hedge country, narrow lanes, yours included, run between high hedge banks, with gaps for junctions and gateways.
- The ride screenshots and the animation in the README are fresh from this version.

## Download

| System | File | Notes |
|---|---|---|
| **macOS** (Apple Silicon and Intel, 10.15+) | `VeloPopuli_*_universal.dmg` | Drag to Applications. Not notarised yet: the first time, open **System Settings → Privacy & Security → Open Anyway** (or run `xattr -dr com.apple.quarantine /Applications/VeloPopuli.app`). |
| **Windows** 10/11 (64-bit) | `VeloPopuli_*_x64-setup.exe` or `VeloPopuli_*_x64_en-US.msi` | Not code-signed yet: if SmartScreen warns, choose **More info → Run anyway**. ANT+ sticks need the WinUSB driver ([Zadig](https://zadig.akeo.ie)). |
| **Ubuntu / Debian** (and Mint, Pop!_OS…; 64-bit) | `VeloPopuli_*_amd64.deb` | `sudo apt install ./VeloPopuli_*_amd64.deb` — installs the ANT+ stick permission rule too. |
| **Arch Linux** (and Manjaro, EndeavourOS…) | `velopopuli-bin-*-x86_64.pkg.tar.zst` | `sudo pacman -U velopopuli-bin-*.pkg.tar.zst`. The `PKGBUILD` is attached too. Install `bluez` for Bluetooth. |
| **Any other Linux** (64-bit) | `VeloPopuli_*_amd64.AppImage` | `chmod +x` and run. |

**Group-ride relay** (optional, for hosting rides with friends): `velopopuli-relay-*` for macOS, Windows and Linux — see [Group rides and self-hosting](https://github.com/pepperstm/velopopuli/wiki/Group-rides-and-self-hosting).

Setting up: [Installing](https://github.com/pepperstm/velopopuli/wiki/Installing) · [Connecting your trainer](https://github.com/pepperstm/velopopuli/wiki/Connecting-your-trainer) · [Your first ride](https://github.com/pepperstm/velopopuli/wiki/Your-first-ride)
