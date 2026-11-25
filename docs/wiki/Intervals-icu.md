# Intervals.icu

> **New in 0.4.0.** The Intervals.icu connection is in the [release candidates](https://github.com/pepperstm/velopopuli/releases) only for now, not in a stable release.

[Intervals.icu](https://intervals.icu) is a training log and planner. VeloPopuli sends your rides straight to it, and rides the workouts planned on your Intervals.icu calendar.

## Connecting

1. On intervals.icu, open **Settings** and scroll down to **Developer Settings**.
2. Copy your **athlete ID** (it starts with `i`, such as `i12345`) and create an **API key**.
3. In VeloPopuli → **Settings → Intervals.icu**, enter both and press **Save and test**. The test tells you if the ID or the key is wrong, and shows your name once it works.

The key is kept in the system keychain (macOS, Windows) or a file only you can read (Linux). **Disconnect** keeps the ID and key, so connecting again is one click.

## Uploading rides

- Rides upload with one click on the summary or history screen, or automatically (the toggle on the Intervals.icu card). Rides under 15 minutes ask first; see [[Strava]] for how to change that.
- The whole `.FIT` file goes across: power, heart rate, cadence, the route, every lap (each workout step is a lap), and in ERG and workouts the power the trainer was asked to hold, which Intervals.icu can chart next to the power you rode.
- Once it's uploaded, the button becomes **View on Intervals.icu**.
- Intervals.icu pairs a ride with the workout planned for that day by itself.

## Riding your planned workouts

With Intervals.icu connected, the **Workouts** screen shows **Planned on Intervals.icu**: the cycling workouts on your calendar for the next two weeks, with the day, length and training load.

- **Add to my workouts** puts one in your workout library. Then pick it when you start a ride, like any other workout.
- Targets come as a share of your FTP, so they follow the FTP set in VeloPopuli (**Settings → Rider**).
- If your plan changes on Intervals.icu, press **Added · update** to fetch the workout again. It replaces the copy in your library.
- Only workouts with steps are listed. A workout that's just a note has nothing for the trainer to hold.
