# Group rides and self-hosting

Ride a route together: everyone's rider appears in everyone's world.

## Riding together

1. One person starts a ride with **Ride with friends** ticked and shares the **room code**.
2. Everyone else goes to **Routes → Join a group ride** and enters the code. The host's route is downloaded automatically.

Group rides pass through a **relay**, a tiny server that forwards riders' positions within a room. There's no public relay yet, so for now one of you runs one.

## Running a relay

The relay is one small program. Download `velopopuli-relay` for Windows, macOS or Linux from the [latest release](https://github.com/pepperstm/velopopuli/releases) and run it on any computer the riders can reach. It listens on port 47800; give it an address to use another, such as `velopopuli-relay 0.0.0.0:9000`.

Then everyone sets **Settings → Group rides → Relay address** to `ws://your-host:47800`.

- It keeps nothing on disk and holds rooms only while riders are in them (up to 1,000 rooms of 50 riders).
- To reach it from the internet, forward the port on your router, or run it on a small cloud server.
- For a secure `wss://` address, put it behind a reverse proxy that handles TLS, such as Caddy (`reverse_proxy localhost:47800`). The app accepts `wss://` relay addresses.

## Running a relay with Docker

> **New in 0.4.0.** The image is published from the 0.4.0 release candidates on. Until the full 0.4.0 release, use the `next` tag, as below. Afterwards, leave the tag off to get the latest full release.
>
> The relay hasn't changed since 0.3.10, so riders on 0.3.10 can use it too, and riders on either version can share a room.

On a home server, a NAS (Unraid, TrueNAS, Synology), a Raspberry Pi or a cloud server with Docker, the relay is one command:

```bash
docker run -d --name velopopuli-relay --restart unless-stopped -p 47800:47800 ghcr.io/pepperstm/velopopuli-relay:next
```

It starts with the computer and restarts if it stops. The image is about 3 MB and runs on Intel/AMD and ARM (64-bit). To use another port, change the first number: `-p 9000:47800` makes the relay address `ws://your-host:9000`.

To update it, pull the new image and start it again:

```bash
docker pull ghcr.io/pepperstm/velopopuli-relay:next
docker rm -f velopopuli-relay
# then the docker run line above
```

| Tag | What you get |
|---|---|
| `latest` (or no tag) | The latest full release (from 0.4.0) |
| `next` | The newest build, release candidates included |
| `0.4.0` | That version only |

### With Docker Compose

```yaml
services:
  relay:
    image: ghcr.io/pepperstm/velopopuli-relay:next
    restart: unless-stopped
    ports:
      - "47800:47800"
```

### With a secure `wss://` address

Caddy gets a certificate for your domain by itself. Point a domain at the server, open ports 80 and 443, and use:

```yaml
services:
  relay:
    image: ghcr.io/pepperstm/velopopuli-relay:next
    restart: unless-stopped
  caddy:
    image: caddy:2
    restart: unless-stopped
    command: caddy reverse-proxy --from relay.example.com --to relay:47800
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - caddy-data:/data
volumes:
  caddy-data:
```

Then everyone sets the relay address to `wss://relay.example.com`.

## Coming later

Club servers that are easier to run, with private rooms, scheduled rides and shared routes.
