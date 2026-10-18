# Installing

Installers for every platform are built automatically on each change: open the repository's [Actions tab](https://github.com/pepperstm/velopopuli/actions), pick the latest green run and download the artifact for your system. (Proper releases with download links on [velopopuli.com](https://velopopuli.com) are coming.)

## macOS

1. Open `Velo Populi_<version>_universal.dmg` and drag **Velo Populi** into **Applications**. It runs on Apple Silicon and Intel Macs, macOS 10.15 or later.
2. The app isn't notarised by Apple yet (that needs a paid developer account), so the first time you open it macOS refuses. Either:
   - open **System Settings → Privacy & Security**, scroll to *"Velo Populi" was blocked* and click **Open Anyway**, or
   - run `xattr -dr com.apple.quarantine "/Applications/Velo Populi.app"` once in Terminal.
3. The first time you search for a Bluetooth sensor, macOS asks to let Velo Populi use Bluetooth — allow it.

## Windows

Run the `.msi` or the `-setup.exe`. Windows SmartScreen may warn that the app is unrecognised (it isn't code-signed yet): choose **More info → Run anyway**. WebView2 is already part of Windows 10 and 11.

For an ANT+ stick, Windows needs the generic WinUSB driver — see [[Connecting your trainer]].

## Linux

Install the `.deb` (Debian, Ubuntu, Mint…) or run the `.AppImage`. Bluetooth needs BlueZ running. The `.deb` installs the rule that lets you use an ANT+ stick without root; with the AppImage, add it yourself (see [[Connecting your trainer]]).

> Windows and Linux builds are produced and tested automatically, but have had little real-world use yet. Reports are very welcome — see [[Reporting problems]].

## Where your data lives

Routes, rides, settings and logs are kept in the app's data folder (on macOS `~/Library/Application Support/org.velopopuli.VeloPopuli`). Account tokens (Strava, Home Assistant) go in the system keychain on macOS and Windows, and in files readable only by you on Linux. Rides are saved as standard `.FIT` files.
