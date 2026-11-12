# Strava

Connect Strava to upload your rides, ride your Strava routes, race your Strava rides as ghosts, and be timed on Strava segments.

## Connecting

Strava gives each app its own key. An open-source app can't keep a key secret, so you make your own, once. It's free and takes about a minute.

1. Go to <https://www.strava.com/settings/api> and create an application. Any name and website will do.
2. Set **Authorization Callback Domain** to `localhost`.
3. In VeloPopuli, open **Settings → Strava**, paste the **Client ID** and **Client Secret**, and press **Connect**.
4. Your browser opens Strava. Approve the access it asks for, and you're connected.

Strava may ask for an app icon when you create the application. Any square picture will do.

## What you can do

- **Upload rides.** Press **Upload to Strava** on the ride summary, or later under **Progress → Rides**. Rides go up as **Virtual Rides**, with power, heart rate and the route map. Tick **Upload every ride automatically** on the Strava card to skip the button. Afterwards, **View on Strava** opens the activity.
- **Ride your Strava routes.** On **Find a route**, press **Show my Strava routes** to list the routes you've made or starred, then save the ones you want to your library. See [[Routes and the map]].
- **Race your rides as ghosts.** When setting up a ride, press **From Strava…** under *Ghosts to race* and pick a ride. The rides that follow the route are listed first.
- **Strava segments.** When setting up a ride, press **Find Strava segments**. You're timed on each one along the route, with a live gap to your best.

## Private routes

If you connected Strava before version 0.3.10, VeloPopuli can only see your public routes, and Find a route says so. Press **Disconnect**, then **Connect** again, and approve the extra access Strava asks for.

## Your details

Your key and Strava's sign-in tokens stay on your computer: in the system keychain on macOS and Windows, and in a file only you can read on Linux. **Disconnect** signs you out but keeps your key, so connecting again is one click.
