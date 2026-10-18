# Group rides and self-hosting

Ride a route together: everyone's rider appears in everyone's world.

## Riding together

1. One person starts a ride with **Ride with friends** ticked and shares the **room code**.
2. Everyone else goes to **Routes → Join a group ride** and enters the code. The host's route is downloaded automatically.

Group rides pass through a **relay** — a tiny server that forwards riders' positions within a room. There's no public relay yet, so for now one of you runs one.

## Running a relay

The relay is a single small program (`crates/relay`); CI builds it for Windows, macOS and Linux (`velopopuli-relay`), or build it yourself:

```bash
cargo run --release -p velopopuli-relay            # listens on 0.0.0.0:47800
cargo run --release -p velopopuli-relay 0.0.0.0:9000
```

Then everyone sets **Settings → Group rides → Relay address** to `ws://your-host:47800`.

- It keeps nothing on disk and holds rooms only while riders are in them (up to 1,000 rooms of 50 riders).
- To reach it from the internet, forward the port on your router, or run it on a small cloud server.
- For a secure `wss://` address, put it behind a reverse proxy that handles TLS, such as Caddy (`reverse_proxy localhost:47800`) — the app accepts `wss://` relay addresses.

## Coming: self-hosted club servers

The plan is to make running your own server for a club or group of friends first-class: a Docker image with `wss://` out of the box, private rooms, a server list in the app, scheduled rides, shared routes and leaderboards, and simple admin. Friends lists and accounts would work without any central server — your identity lives on your device and clubs run their own servers.
