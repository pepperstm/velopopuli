# Riding

![On the climb](https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/ride-climb.jpg)

## The HUD

Power (with W/kg and zone), speed, heart rate and cadence on the left, with what's coming up (villages, summits, cafés) below. The clock is at the top, the gradient top right, the riders around you and a live map on the right, and two elevation profiles — the next stretch and the whole route — along the bottom. On a climb a banner counts down to the summit; on a Strava segment, a live gap to your best.

## Speed and resistance

- **Gradient mode** (default): the trainer simulates the real slope. **Feel** scales it (100% = the real gradient), and **Settings → Trainer & physics** caps the steepest pitch and softens descents, so a 15% ramp is rideable on any trainer.
- **ERG mode**: the trainer holds a fixed power while the route scrolls by.
- **Gears**: shift your bike as normal — or, if you prefer, set up [[Virtual gears]] and shift with keys or a remote.
- **Speed is simulated from your power** — with your weight, the bike, the slope, rolling resistance and air (thinner at altitude). The trainer's difficulty only changes how it feels, never your speed.
- **You feel the road surface.** Concrete, gravel, cobbles and earth roll heavier than tarmac: you slow down on screen, and the trainer gets harder by the same force. At 200 W on the flat (84 kg, road tyres) that's about 34 km/h on tarmac, 33.5 on concrete, 31 on gravel and 29 on cobbles or earth. Graded mountain-bike trails roll heavier the harder they are — S0 ×4 your tarmac figure, S1 ×5, S2 ×6.5, S3 ×8, S4 and above ×10 — for roots, rocks and steps the model can't otherwise see. They also have a speed limit, standing in for braking on technical ground the way the descent speed limit does for corners: 40 km/h on S0, 30 on S1, 22 on S2, 15 on S3, 10 on S4 and above. Set your tyres' rolling resistance on tarmac, or turn this off, under **Settings → Trainer & physics → Feel the road surface**. It does nothing in ERG mode.

## Cameras

`C` cycles: behind, high behind, alongside and overhead.

![Overhead camera](https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/ride-hairpins.jpg)

## Racing

- **Pace riders** hold a steady W/kg to chase or drop (set them in Settings).
- **Your ghost** — your best ride on the route.
- **Other people's rides** as ghosts — from a GPX or FIT file, or from your Strava.
- **Climbs** are detected and categorised (4 → HC) and timed; **Strava segments** along the route are timed too.

## Time of day and weather

Dawn, day, evening, dusk or night (with a lamp on the bike), or the real clock; clear, cloudy, rain, fog or snow — or the live weather at the route. **Settings → Virtual world**.

| | |
|---|---|
| ![Dusk in the rain](https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/ride-dusk-rain.jpg) | ![Night](https://raw.githubusercontent.com/pepperstm/velopopuli/main/docs/images/ride-night.jpg) |

## Native renderer (macOS, experimental)

**Settings → Virtual world → Native renderer** draws the world with Metal directly instead of in the app's web page, for lower overhead.
