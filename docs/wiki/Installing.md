# Installing

Download the installer for your system from the [latest release](https://github.com/pepperstm/velopopuli/releases). (Every change is also built automatically — the newest builds are under the repository's [Actions tab](https://github.com/pepperstm/velopopuli/actions).)

## macOS

1. Open `VeloPopuli_<version>_universal.dmg` and drag **VeloPopuli** into **Applications**. It runs on Apple Silicon and Intel Macs, macOS 10.15 or later.
2. The app isn't notarised by Apple yet (that needs a paid developer account), so the first time you open it macOS refuses. Either:
   - open **System Settings → Privacy & Security**, scroll to *"VeloPopuli" was blocked* and click **Open Anyway**, or
   - run `xattr -dr com.apple.quarantine /Applications/VeloPopuli.app` once in Terminal.
3. The first time you search for a Bluetooth sensor, macOS asks to let VeloPopuli use Bluetooth — allow it.

## Windows

Run the `.msi` or the `-setup.exe`. Windows SmartScreen may warn that the app is unrecognised (it isn't code-signed yet): choose **More info → Run anyway**. WebView2 is already part of Windows 10 and 11.

For an ANT+ stick, Windows needs the generic WinUSB driver — see [[Connecting your trainer]].

## Linux

- **Ubuntu, Debian, Mint, Pop!_OS…:** `sudo apt install ./VeloPopuli_<version>_amd64.deb`
- **Arch, Manjaro, EndeavourOS…:** `sudo pacman -U velopopuli-bin-<version>-x86_64.pkg.tar.zst` (the `PKGBUILD` is attached to the release too)
- **Anything else:** make the `.AppImage` executable and run it.
 Bluetooth needs BlueZ running. The `.deb` installs the rule that lets you use an ANT+ stick without root; with the AppImage, add it yourself (see [[Connecting your trainer]]).

> Windows and Linux builds are produced and tested automatically, but have had little real-world use yet. Reports are very welcome — see [[Reporting problems]].

## Where your data lives

Routes, rides, settings and logs are kept in the app's data folder (on macOS `~/Library/Application Support/org.velopopuli.VeloPopuli`). Account tokens (Strava, Home Assistant) go in the system keychain on macOS and Windows, and in files readable only by you on Linux. Rides are saved as standard `.FIT` files.
