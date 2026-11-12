# VeloPopuli for developers

How VeloPopuli is built: every part of the app, what it does, and why it is the way it is. It's for anyone changing the code. The user's manual is the [wiki](https://github.com/pepperstm/velopopuli/wiki).

These pages describe how things work. Two other files cover the project itself:

- [HANDOFF.md](../../HANDOFF.md): where the project stands, what has and hasn't been tried with real hardware, the next priorities, the decisions behind things and the known traps.
- [CONTRIBUTING.md](../../CONTRIBUTING.md): how a change is proposed, written, reviewed and merged.

## Chapters

1. **[Architecture](architecture.md)**: the three layers, how they talk, and what happens from pressing *Ride* to the summary.
2. **[Building and testing](building-and-testing.md)**: tools, commands, the test suites, the smoke harness and CI.
3. **[Routes](routes.md)**: GPX in, a rideable route out. Resampling, smoothing, gradients, climbs, loops, and where routes come from.
4. **[Physics and trainer control](physics-and-trainer-control.md)**: speed from power, road surfaces, what the trainer is told and when, and virtual gears.
5. **[The ride](the-ride.md)**: the ride engine, rivals and ghosts, laps, timed climbs and segments, recording, and what happens at the end: bests, XP, badges, fitness and workouts.
6. **[The world](the-world.md)**: building the 3D landscape from map and elevation data: terrain, the route's road, trails, bridges and tunnels, buildings, woods, fields, barriers, landmarks and the velodrome.
7. **[Drawing](drawing.md)**: how the page draws the world with three.js, how data reaches it, the rider and cameras, and the native renderer.
8. **[Devices](devices.md)**: Bluetooth and ANT+, trainers and sensors, the sensor hub, remembered sensors, controllers and the demo rider.
9. **[Connectors and other services](connectors-and-services.md)**: Strava and Dreeve, group rides and the relay, fan and lights, street photos, place search and route planning.
10. **[Data and diagnostics](data-and-diagnostics.md)**: the data folder, store keys, caches and their versions, secrets, logs and reports.
11. **[Releasing](releasing.md)**: versions, CI, releases, the wiki and the website.

Adding a service for uploads or ghosts has its own guide: [Adding a connector](../ADDING_A_CONNECTOR.md).

## The code at a glance

| Folder | What's in it | Language |
|---|---|---|
| [`crates/core`](../../crates/core) | Everything that computes: routes, physics, the ride, workouts, fitness, FIT files, sensor protocols, the whole 3D world. No I/O, so all of it is unit-tested. | Rust |
| [`crates/relay`](../../crates/relay) | The group-ride relay server. | Rust |
| [`src-tauri`](../../src-tauri) | The app: windows, commands, the ride loop, Bluetooth and ANT+, downloads and caching, connectors, the native renderer. | Rust (Tauri 2) |
| [`web`](../../web) | The screens (React) and the 3D drawing (three.js). No ride logic. | TypeScript |
| [`docs`](..) | These pages, the wiki source (`docs/wiki`), README pictures (`docs/images`). | Markdown |
| [`packaging`](../../packaging) | Release notes and the Arch Linux package. | |
| [`.github`](../../.github) | CI, issue forms, the pull-request template. | YAML |

About three quarters of the code is Rust. The rule that shapes everything: **Rust decides, the page draws.** Anything that computes something or talks to hardware or the network is in Rust. The page shows what Rust tells it and sends back what the rider pressed.
