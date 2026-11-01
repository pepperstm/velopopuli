# velopopuli.com

The IONOS website source lives in `source/`. It was recovered from the original launch kit and the live homepage on 1 November 2026, preserving the v0.3.5 bleeding-edge feature section and route planner. Those historical feature claims are editorial content; they are not relabelled as new in every subsequent release.

`Update website` builds the latest published stable release into the homepage: current version, direct macOS/Windows/Linux installer links, full release notes (escaped text), and the GitHub release page. It refuses an incomplete installer set. Prereleases are excluded. The existing app `Build` workflow publishes with GITHUB_TOKEN, whose release event cannot start another workflow, so successful Build completion also triggers an update. Manual releases, release edits, website changes on main, and manual dispatch are covered. No personal GitHub token is needed.

## IONOS setup

The domain is connected to `/public/velopopuli` in IONOS Web Hosting Plus. Create an **SFTP-only** account restricted to that directory, then save these repository Actions secrets directly in GitHub Settings → Secrets and variables → Actions:

- `IONOS_HOST`: the personal server hostname shown by IONOS, not velopopuli.com.
- `IONOS_USER`: the SFTP username assigned by IONOS.
- `IONOS_PASSWORD`: that account's password. Enter it only into IONOS and GitHub's secret form, never into chat, source files, or logs.
- `IONOS_PATH`: `/` when the SFTP account is restricted to `/public/velopopuli`; otherwise the absolute path visible from the SFTP session (normally `/public/velopopuli`).
- `IONOS_KNOWN_HOSTS`: the server's OpenSSH known_hosts entry, verified against IONOS's published SSH fingerprint before storing it. Port is 22. Host-key verification is mandatory.

Run `Update website` from Actions after adding secrets. It uploads all managed website files over SFTP using temporary files and atomic rename, switches index.html last, preserves unmanaged server files, and checks the live release.json. The build artifact remains available even if secrets are missing. Existing hosting caches and custom configuration are not deleted. To roll back, restore the prior website commit and rerun; release metadata always tracks GitHub's latest stable release.

Build locally: `gh api repos/pepperstm/velopopuli/releases/latest > /tmp/release.json`, then `python3 website/scripts/build.py /tmp/release.json website/build` from the repository root. The PHP route API comes from the original local source because public HTTP cannot retrieve PHP source.
