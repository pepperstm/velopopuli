# FAQ

**Is it really free?**
Yes — free to use, open source (MIT), no subscription, no account.

**Which trainers work?**
Bluetooth FTMS trainers (most made since about 2018), Tacx trainers using FE-C over Bluetooth, and any ANT+ FE-C trainer through a USB stick. Power meters, heart-rate straps and speed/cadence sensors too. Real-world testing is just beginning — see [[Connecting your trainer]].

**Do I need a smart trainer?**
No. With a power meter you ride normally (the trainer just doesn't change resistance). With only a speed sensor on a basic trainer, VeloPopuli can estimate power. With nothing at all, try the demo rider.

**Why is my speed different from my trainer's display?**
Your speed comes from your power, weight and the virtual road, as it would outdoors — not from the flywheel.

**Why does the first ride on a route take a while to start?**
It downloads that route's terrain and map data once. Later rides start immediately, and work offline.

**Why does the road surface or width look wrong somewhere?**
They come from OpenStreetMap tags. If a stretch is mis-tagged, you can fix it on [openstreetmap.org](https://www.openstreetmap.org) and everyone benefits. If the tags couldn't be fetched, the app guesses from the road class and tries again a day later.

**Can I ride with friends?**
Yes — see [[Group rides and self-hosting]]. For now one of you runs the small relay server.

**Does it work with Apple Health or my watch?**
Not at the moment. Rides are saved as standard FIT files, which most platforms import, and upload to Strava directly. A watch that broadcasts heart rate over Bluetooth works as a heart-rate sensor.

**How do I report a bug?**
[[Reporting problems]] — the app builds the report for you.
