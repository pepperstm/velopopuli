# Physics and trainer control

Two separate things happen every moment of a ride:

1. **The simulation** works out how fast the rider goes from their power, as outdoors ([`physics.rs`](../../crates/core/src/physics.rs)).
2. **The trainer** is told how hard to make the pedals feel: a gradient to simulate, or a power to hold.

The trainer's own speed reading is never used for the rider's speed. Every trainer therefore gives the same speed for the same effort, and the trainer's difficulty setting changes only how it feels.

## Speed from power

`step_speed(v, power, grade, elevation, params, dt)` integrates in steps of at most 0.1 s:

```
drive  = P × 0.975 / max(v, 1.5)                       drivetrain losses; the floor caps the force from a standstill
resist = m·g·(sin θ + Crr·cos θ) + ½·ρ(h)·CdA·v²       slope, rolling, air
v     += (drive − resist) / (m + 1.4) × dt              1.4 kg stands in for the wheels' rotating mass
v      = clamp(v, 0, max_speed)
```

- `m` is rider plus bike (defaults 75 + 9 kg), `θ` the slope angle from the gradient, `g` 9.80665.
- **Air density** falls with height: `ρ(h) = 1.225 × e^(−h / 8500)`. Riding at 2,000 m is noticeably faster for the same power.
- **Defaults** (`PhysicsSettings`): CdA 0.32 m², Crr 0.004 (road tyres on tarmac), top speed 80 km/h. The top speed stands in for braking on descents, since corners aren't modelled.
- The bike is held still until the rider pedals (under 0.3 m/s and 5 W), even if the start points downhill. A tick longer than 0.25 s (after a stall or sleep) is cut short so the rider doesn't jump down the road.

At 200 W on the flat with the defaults that's about 34 km/h.

### The road surface

With **Feel the road surface** on (`road_surfaces`, the default), the rider's Crr is multiplied according to where they are ([`ride.rs`](../../crates/core/src/ride.rs), `crr_at`):

| Where | Multiplier | Speed limit |
|---|---|---|
| Tarmac | 1.0 | |
| Concrete | 1.25 | |
| Gravel | 2.5 | |
| Cobbles, dirt | 3.5 | |
| Mountain-bike trail S0 | 4.0 | 40 km/h |
| S1 | 5.0 | 30 km/h |
| S2 | 6.5 | 22 km/h |
| S3 | 8.0 | 15 km/h |
| S4 and up | 10.0 | 10 km/h |
| A ford | 6.0 at least | 12 km/h, for 6 m either side |

A graded trail uses its trail figure instead of its surface. In a ford, the larger of the ford's and the trail's applies. The trail limits stand in for braking on roots, rocks and steps, which the model can't see, just as the descent limit stands in for braking into corners. The surface factors are in `Surface::rolling_factor`, the trail ones in `Trail::rolling_factor` and `Trail::max_speed` ([`world/surface.rs`](../../crates/core/src/world/surface.rs)), and the ford's in [`world/fords.rs`](../../crates/core/src/world/fords.rs). At 200 W that comes to about 34 km/h on tarmac, 33.5 on concrete, 31 on gravel and 29 on cobbles or earth.

Where each sample is (surface, trail grade, ford) is decided when the world is built (see [The world](the-world.md#the-routes-own-road)) and handed to the engine with `set_surfaces`, `set_trails` and `set_fords`.

## What the trainer is told

The ride loop sends one `TrainerCommand` a second, or within 250 ms of a gear change (but never more than four times a second):

| Situation | Command |
|---|---|
| A workout step, with **Hold workout targets** on | `TargetPower(target × FTP × intensity)` |
| ERG mode, no workout | `TargetPower(erg_watts)` |
| Otherwise | `Simulation { grade, crr, cw }` |

At ride start the trainer also gets `UserConfig { rider_kg, bike_kg }`. How each command becomes bytes for each kind of trainer is in [Devices](devices.md#trainer-control).

### The gradient sent

`RideEngine::trainer_grade` takes the road's gradient and applies the rider's settings (`TrainerSettings::trainer_grade`):

```
g = road_grade × difficulty         "Feel": 0.75 by default
if g < 0: g × downhill              descents softened: 0.5 by default
g = clamp(g, −max_grade, max_grade) 15% by default (0 turns the cap off)
```

Then it **adds the surface's extra rolling resistance as gradient**: `+ (Crr_here − Crr_base)`. On a slope the extra force is `m·g·ΔCrr` either way, and every trainer follows gradient while some ignore the Crr they're sent. So cobbles feel heavy on any trainer. Paused or finished, the gradient is 0.

`crr` sent is the rider's base Crr. `cw` is the wind coefficient `½·ρ(h)·CdA` in kg/m, the form trainers expect in simulation mode.

### Power without a power meter

**Estimate power from a speed sensor** (`virtual_power`) turns wheel speed into watts with a typical fluid-trainer curve (Kurt Kinetic's): `P = 5.24482 × mph + 0.019168 × mph³`. It's only used when nothing measures power ([`devices/mod.rs`](../../crates/core/src/devices/mod.rs)).

## Virtual gears

With *My bike's gears* (the default) nothing changes: the trainer feels the flywheel speed, so the bike's own gears work as on the road. **Virtual gears** ([`gearing.rs`](../../crates/core/src/gearing.rs)) are for leaving the bike in one gear and shifting with keys, buttons or a controller.

### The idea

The rider's speed still comes from their power, so a gear can't make them faster. It changes how the power is delivered: more force at a lower cadence in a harder gear. With the bike in real ratio `r_real` and the rider in virtual ratio `r_virtual`, let `k = r_virtual / r_real`:

- The flywheel turns at the speed the **real** gear gives for the rider's cadence: `v_trainer = v / k`.
- To feel the virtual gear, the trainer must push back with the road force scaled by the ratio: `F_trainer = k × F_road(v)`.

A trainer in simulation mode works out its force as `m·g·(grade + crr) + cw·v_trainer²` at its own speed. So the app sends the gradient that makes that come out right (`effective_grade`):

```
m·g·(g_eff + crr) + cw·v_t² = k·(m·g·(g + crr) + cw·v²)
⇒ g_eff = k·(g + crr) + (k·cw·v² − cw·v_t²) / (m·g) − crr
```

In the real gear `k = 1`, and the gradient is unchanged. The result is clamped to −25%…+40%, beyond which trainers ignore or misbehave. It goes through the ordinary simulation command, so it works on every trainer the app supports.

Consequences, all as on a real bike: at low speed on the flat there is little force to scale, so gears hardly matter. On a descent steep enough to carry you, no gear gives anything to push against. Very hard gears at low speed can ask for more than a wheel-on trainer can give.

**Simple steps** (`feel: "steps"`) is a fallback for trainers that behave oddly: a fixed gradient (`step`, 1% by default) per gear either side of the real one.

### Gear sets

- **Presets** (`PRESETS`): 24 evenly spaced ratios from 0.75 to 4.8, road compact 50/34 × 11–34, semi-compact 52/36 × 11–30, standard 53/39 × 11–28, gravel 40 × 10–44 and MTB 32 × 10–52.
- **Custom:** the rider's chainrings and cogs. Every combination is sorted easiest first; any within 2.5% of the one before is dropped, so shifting steps through the whole range one gear at a time, like sequential shifting.
- **Start gear:** the one set, or else the one nearest the real ratio.
- **Cadence** with no cadence sensor is worked out from speed, gear and wheel size (`cadence`), and shown as estimated.

Shifting is `ride_shift` (keys, on-screen buttons) or a controller press (see [Devices](devices.md#handlebar-controllers)). Gears do nothing in ERG or in a workout held in ERG.
