**VeloPopuli — the people's indoor ride.** Ride real GPX routes in 3D on your smart trainer. Free and open source. [velopopuli.com](https://velopopuli.com) · [Guides](https://github.com/pepperstm/velopopuli/wiki)

> This is an early release. The 3D world, physics, workouts and progress tracking are well tested; real trainers and sensors are only now being tried, and the Windows and Linux builds have had little use. If something doesn't work, the app's **Report a problem → Report on GitHub** builds a diagnostics report for you — please send one.

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
