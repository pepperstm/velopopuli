# Releasing

Releases are built by CI from a version tag. Nothing is built or uploaded by hand.

## Versions

Versions are `MAJOR.MINOR.PATCH` (0.3.10, say). A suffix such as `0.4.0-beta.1` makes a pre-release; a plain version becomes the latest release.

The version is written in these places, which must all agree:

| File | Field |
|---|---|
| `package.json` | `version` |
| `package-lock.json` | the two `version` fields at the top |
| `src-tauri/tauri.conf.json` | `version` (the installers' version, and what the Arch job reads) |
| `src-tauri/Cargo.toml`, `crates/core/Cargo.toml`, `crates/relay/Cargo.toml` | `version` |
| `Cargo.lock` | the three workspace crates (`cargo check` updates them) |
| `packaging/arch/PKGBUILD` | `pkgver` |
| `HANDOFF.md` | the heading of §2 |

## Making a release

1. **Branch** `release/vX.Y.Z` from an up-to-date `main`.
2. **Bump the version** everywhere above.
3. **Write the release notes** in [`packaging/RELEASE_NOTES.md`](../../packaging/RELEASE_NOTES.md): rewrite *What's new* for this version in plain words, thanking contributors and reporters by their GitHub names. The rest (downloads, first launch on each system) changes only when that does. This file is the release's description on GitHub.
4. **Open a pull request**, let CI pass, and squash-merge it ("Release vX.Y.Z (#N)").
5. **Tag** the merge commit and push the tag:

   ```bash
   git switch main && git pull
   git tag -a vX.Y.Z -m "VeloPopuli X.Y.Z"
   git push origin vX.Y.Z
   ```

6. CI builds everything again from the tag, and its **Publish release** job makes the GitHub Release (below). It takes about 15 minutes. If a macOS job is cancelled because no runner picked it up, re-run the failed jobs: `gh run rerun <run id> --failed`.
7. **Check the release page**: every file is there, the notes read right, and the download links on the website point at the new version.

### What the release contains

The `release` job in [`build.yml`](../../.github/workflows/build.yml) collects the `app` and `arch` jobs' artifacts:

| File | For |
|---|---|
| `VeloPopuli_X.Y.Z_universal.dmg` | macOS, Apple silicon and Intel |
| `VeloPopuli_X.Y.Z_x64_en-US.msi`, `VeloPopuli_X.Y.Z_x64-setup.exe` | Windows |
| `VeloPopuli_X.Y.Z_amd64.deb`, `VeloPopuli_X.Y.Z_amd64.AppImage` | Linux |
| `VeloPopuli_X.Y.Z_amd64.AppImage.zsync` | Updating the AppImage (below) |
| `velopopuli-bin-X.Y.Z-1-x86_64.pkg.tar.zst`, `PKGBUILD` | Arch Linux |
| `velopopuli-relay-macos-arm64`, `-linux-x86_64`, `-windows-x86_64.exe` | The group-ride relay |
| `SHA256SUMS.txt` | Checksums of all of the above |

### Updating the AppImage

The AppImage carries update information, so [AppImageUpdate](https://github.com/AppImageCommunity/AppImageUpdate) and tools like it can update an installed copy from the latest release, downloading only what changed. After `tauri build`, the Linux job runs [`packaging/appimage-update-info.sh`](../../packaging/appimage-update-info.sh), which unpacks the AppImage, packs it again with [appimagetool](https://github.com/AppImage/appimagetool) and `-u "gh-releases-zsync|pepperstm|velopopuli|latest|VeloPopuli_*_amd64.AppImage.zsync"`, and checks the result. The `.zsync` file it makes is published with the release. `latest` means pre-releases are never offered as updates.

VeloPopuli is also listed in the [AppImage catalog](https://appimage.github.io), which reads the newest release from GitHub itself.

### Signing

- **macOS** builds are ad-hoc signed (`signingIdentity: "-"`, hardened runtime off), not notarised. The first launch needs *Open Anyway* in System Settings → Privacy & Security. A Developer ID certificate and notarisation would remove that.
- **Windows** builds are unsigned, so SmartScreen warns on first run.
- **Linux:** the binary is built on Ubuntu 22.04 (glibc 2.35), so it runs on that and anything newer, Arch included.

The release notes explain each first launch to riders.

## After a release

### The wiki

The user's manual is the [GitHub wiki](https://github.com/pepperstm/velopopuli/wiki). Its pages are kept in this repository under [`docs/wiki`](../wiki), so they're reviewed with the code they describe, and copied to the wiki repository after the pull request merges:

```bash
git clone https://github.com/pepperstm/velopopuli.wiki.git /tmp/wiki
cp docs/wiki/*.md /tmp/wiki/
cd /tmp/wiki && git add -A && git commit -m "…" && git push
```

Pictures are linked from `main` (`https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/…`), so push the wiki only after the change that adds them has merged.

The wiki is written for riders: what to press and what happens, with no code. How things work inside belongs in these developer pages.

### The website

[velopopuli.com](https://velopopuli.com) is a static site kept outside this repository, uploaded by the maintainer. After a release its download links and version numbers are updated, and any new feature added to the feature list and the FAQ.

### The README

The README's **status table** says what has been tried with real hardware and what hasn't. Keep it honest: say what was checked and how, and mark anything only run in the smoke harness or with the demo rider as such.

## Hotfixes

A fix for a release goes to `main` like any change, then out as the next patch version (0.3.11 after 0.3.10). There are no release branches to maintain.
