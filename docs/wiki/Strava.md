# Strava

Strava gives every app its own key, and an open-source app can't ship a secret, so you create a free personal key once (about a minute):

1. Go to <https://www.strava.com/settings/api> and create an application (any name and website).
2. Set **Authorization Callback Domain** to `localhost`.
3. In Velo Populi → **Settings → Strava**, paste the Client ID and Client Secret and press **Connect**, then approve in the browser.

Then:

- Rides upload as **Virtual Rides** with the route map — one click on the summary, or automatically (Settings).
- **Segments** along a route are timed with a live gap to your best.
- **Your activities** can be raced as ghosts.

The key and tokens are kept in the system keychain (macOS, Windows) or a file only you can read (Linux).

> The Strava connection has not yet been tried against the live service — please report how it goes.
