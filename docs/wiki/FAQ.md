# FAQ

**Is it really free?**
Yes — free to use, open source (GPLv3), no subscription, no account. The licence also means anything built from VeloPopuli has to stay open source.

**Which trainers work?**
Most smart trainers made since about 2018, including Tacx ones, over Bluetooth or through an ANT+ USB stick. Power meters, heart-rate straps and speed and cadence sensors work too. Only a few have been tried so far: see [[Connecting your trainer]].

**Do I need a smart trainer?**
No. With a power meter you ride normally (the trainer just doesn't change resistance). With only a speed sensor on a basic trainer, VeloPopuli can estimate power. With nothing at all, try the demo rider.

**Why is my speed different from my trainer's display?**
Your speed comes from your power, weight and the virtual road, as it would outdoors — not from the flywheel.

**Why does the first ride on a route take a while to start?**
It downloads that route's terrain and map data once. Later rides start immediately, and work offline.

**Why does the road surface or width look wrong somewhere?**
They come from OpenStreetMap tags. If a stretch is mis-tagged, you can fix it on [openstreetmap.org](https://www.openstreetmap.org) and everyone benefits. If the tags couldn't be fetched, the app guesses from the road class and tries again a day later. A dirt track where you expected a road means the map has no road along your route there — often a route drawn freehand rather than along the roads; re-plan it along the road (Find a route does) or add the missing road to OpenStreetMap.

**Where do I find routes?**
Import a GPX from anywhere, plan one on the map, or pick from your Strava routes, signposted cycle routes, mountain-bike trails and famous climbs under **Find a route**. See [[Routes and the map]].

**Do I have to pair my trainer every time?**
No. Pair it once and it connects by itself next time you open the app, as soon as it wakes up.

**Why wasn't my ride saved?**
Rides under a minute aren't saved. The app warns you if you press **End ride** before then.

**Can I ride with friends?**
Yes — see [[Group rides and self-hosting]]. For now one of you runs the small relay server.

**Does it work with Apple Health or my watch?**
Not at the moment. Rides are saved as standard FIT files, which most platforms import, and go to [[Strava]] or [[Dreeve]] directly. A watch that broadcasts heart rate over Bluetooth works as a heart-rate sensor.

**How do I report a bug?**
[[Reporting problems]] — the app builds the report for you.
