# Riders from other apps

Ride the Herne Hill velodrome with riders of other indoor cycling apps.

VeloPopuli speaks [OpenBikeNet](https://openbikenet.cloud), an open protocol for sharing riders between indoor cycling apps. Each app keeps its own physics and world; only where its riders are is shared. The public server is free to use, and anyone can run their own.

## Turning it on

Go to **Settings → Other apps** and tick **Ride with riders from other apps**. It is off by default, so nothing leaves your computer until you turn it on.

Then start a ride on **Herne Hill Velodrome**, either way round. Riders from other apps appear on the same track with their app's name next to theirs. The HUD says how many are there.

The server address is the public one by default (`wss://openbikenet.cloud/v0`). If you run your own server, put its address there instead.

## What is sent

- Your rider name, position on the track, lap and speed.
- Power and cadence, only if **Share power and cadence** is on. It is off by default.
- Never your heart rate.

The server keeps no names or positions after a rider leaves; it stores only per-app usage counts. Names are shown as plain text, never as markup.

## Group rides

On Herne Hill, a group-ride room and OpenBikeNet are alternatives: while OpenBikeNet is on, *Ride with friends* is off there. Turn OpenBikeNet off in Settings to use a room.

## Running your own server

The reference server is open source: [OpenBikeNet/server](https://github.com/OpenBikeNet/server). With Docker, `make up` starts it on port 8090; put `ws://your-host:8090/v0` in the server address (or a `wss://` address behind a TLS proxy).

## Coming later

More shared routes, and the next stage of the protocol where the server calculates speed from power so races are fair.
