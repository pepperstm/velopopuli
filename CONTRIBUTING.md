# Contributing to VeloPopuli

Thank you for helping build the people's indoor ride. This is how work gets from an idea to the main branch. If you're new to the project, read [HANDOFF.md](HANDOFF.md) first — it explains where things stand and why they are the way they are.

## 1. Start with an issue

Every change starts as an issue, so effort isn't duplicated and the approach is agreed before code is written.

- **Bug** — something doesn't work. Attach a diagnostics report if it involves a trainer or sensor (in the app: *Report a problem → Report on GitHub* does it for you).
- **Hardware report** — your trainer or sensor worked, or didn't. These build the tested-hardware list.
- **Feature or improvement** — describe the rider's problem first, then the idea.

For anything bigger than a small fix, say in the issue how you plan to do it and wait for a 👍 from the maintainer. Small fixes and documentation can go straight to a pull request that references an issue.

Issues labelled `good first issue` are a gentle way in; `help wanted` are ones the maintainer would especially like taken on. Comment on an issue to claim it.

## 2. Set up

Fork the repository, then:

```bash
git clone https://github.com/<you>/velopopuli && cd velopopuli
npm install
npm run dev      # the app with hot reload
npm run check    # everything CI checks: formatting, types, lints, tests
```

Build requirements and the automated smoke harness are described in the wiki's [Building from source](https://github.com/pepperstm/velopopuli/wiki/Building-from-source).

## 3. Make the change

**Branch** from an up-to-date `main`, named for the change: `fix/vortex-power`, `feature/surface-rolling-resistance`, `docs/install-linux`.

**Keep it to one thing.** One logical change per pull request is much easier to review and to undo. Split refactors from behaviour changes.

**Where code goes**

- Anything that computes — routes, physics, protocols, terrain, map processing — goes in `crates/core`: pure Rust, no I/O, with tests.
- Hardware, files, network and the simulation loop go in `src-tauri`.
- The page (`web/`) only draws and handles input. It gets data from Rust commands, large data in compact binary form.
- Anything that appears in the 3D world should work in both renderers (three.js in `web/src/world/`, and the native one in `src-tauri/src/native/`), or say clearly in the PR that it doesn't yet.

**Style**

- Formatting is automatic: `npm run format` (rustfmt and Prettier with the repository's settings). CI rejects unformatted code.
- Match the code around you: naming, comment density, idioms. Comments explain *why* and anything non-obvious; they don't narrate the code.
- Names in user-facing text are plain English for riders, not developers: "Your trainer dropped out", not "BLE notification stream ended". British spelling.
- No new dependency without a reason given in the PR. Low overhead is a core goal: prefer the simple, cheap solution, and don't add work to the per-frame or 50 Hz paths without measuring it.
- Never commit secrets, API keys or personal data. Test data must be made up or openly licensed.

**Tests**

- Logic in `crates/core` needs unit tests; bugs get a test that fails before the fix.
- Visual changes: run the smoke harness (`VP_SMOKE_WINDOW=1` gives a full-window screenshot) and attach before/after screenshots to the PR.
- Hardware changes: say exactly what device you tested on and how, or that you couldn't. Don't claim something works that you haven't seen work.

**Documentation** — update in the same PR:

- the README (features, and the *Project status* table — keep it honest about what was and wasn't verified),
- the wiki source in `docs/wiki/` for anything a rider or developer would need to know (the maintainer publishes it to the wiki after merging),
- `HANDOFF.md` if you change something it describes (structure, formats, decisions, traps),
- cache format versions (`FORMAT`, `SCENERY_FORMAT`, `WAY_TAGS`) if you change what is cached.

**Commits**

- Summary line in the imperative, up to about 72 characters: *Keep the trainer connected when notifications pause*.
- A body that explains what changed and why, wrapped at about 76 characters. Mention how it was tested.
- Reference the issue: `Fixes #12` or `Refs #12`.
- Commit under your own name. You're responsible for every line you submit, however it was produced — understand it and test it.

## 4. Open a pull request

Push your branch to your fork and open a PR against `main`. The template asks for:

- what changed and why, and the issue it closes;
- how you tested it (tests, smoke runs, real hardware), with screenshots for anything visible;
- what you updated in the docs, or why nothing needed updating;
- anything unfinished or risky.

Open it as a **draft** early if you want feedback before it's finished.

## 5. Review and merge

- CI must pass: formatting, type checks, lints, tests, and installers building on Windows, macOS and Linux.
- The maintainer reviews every PR. Expect questions and requests for changes; push follow-up commits to the same branch (no need to force-push during review).
- PRs are **squash-merged**: the whole PR becomes one commit on `main`, titled with the PR title and described by its summary — so write those as you would a good commit message.
- After merging, the maintainer syncs the wiki and, when it's time, tags a release (see [HANDOFF.md](HANDOFF.md#7-releases)).

## Licence

VeloPopuli is licensed under the GNU GPL, version 3 or later. By contributing you agree that your contribution is licensed under the same terms. There is no contributor agreement to sign. Only add code or assets whose licence is compatible with GPLv3 (MIT, BSD, Apache-2.0, ISC, zlib, MPL-2.0, LGPL and other GPL-compatible licences are fine; say in the PR if you're unsure).

## Conduct

Be kind and assume good faith. See the [Code of Conduct](CODE_OF_CONDUCT.md).
