# Fan and lights

Velo Populi can drive a fan that speeds up as you go faster (or work harder), and a light coloured by your power zone. **Settings → Fan & lights**:

| Choice | What you enter |
|---|---|
| **Home Assistant** | Its address, a long-lived access token (kept in the keychain), the fan entity (e.g. `fan.desk_fan` or a switch) and optionally a light entity |
| **Shelly** or **Tasmota** smart plug | The plug's address on your network (on/off by level) |
| **Webhook** | Any URL, with `{level}`, `{power}` and `{zone}` filled in |

The fan follows **speed**, **power** or **heart rate**: off below one value and full at another (10 → 35 km/h by default). **Test** checks the connection.

> Tested against simulated devices only so far.
