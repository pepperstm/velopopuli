# Reporting problems

Something not working — a trainer dropping out, power reading zero, resistance not changing? The app keeps a diagnostics log, and turning it into a report takes a few seconds.

1. Ride until the problem has happened (a few minutes is plenty).
2. On the ride summary press **Report a problem…** (also under **Devices → Trouble connecting?** and **Settings → Diagnostics**).
3. Say briefly what happened and how the device was connected (Bluetooth or ANT+).
4. Press **Report on GitHub…**. The app saves the report, opens a new GitHub issue with the summary filled in, and shows the file — drag it onto the issue and press **Create**. (**Just save the report…** if you'd rather send it another way.)

## What's in a report

One text file covering the last couple of days:

- a **summary**: per device, how often it connected and dropped out, how much data it sent and its longest silence; per ride, how many seconds you pedalled with no power reading;
- every Bluetooth scan, the services and features each device offered, connections, drop-outs (and why) and reconnects;
- ANT+ stick and channel events;
- the raw data each sensor sent and every command sent to the trainer;
- a line each second of the ride (power, cadence, speed, the gradient sent, and what each device said);
- your settings, with account details and your name removed.

It contains no account details, tokens or locations. GitHub issues are public, so only send what you're happy to share.

The raw logs are in the app's data folder under `logs/` (**Settings → Diagnostics → Open logs folder**); the last 12 runs are kept. **Detailed diagnostics** (the raw data, about 3 MB an hour) can be switched off in Settings.
