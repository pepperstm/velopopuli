# Dreeve

[Dreeve](https://github.com/dreeveapp/dreeve) (formerly *Statistics for Strava*) is a self-hosted dashboard for your sports data. VeloPopuli can send every ride's `.FIT` file straight to your own instance.

1. In Dreeve's admin panel open **Settings → Security → API access** and generate a key (it looks like `drv_…`).
2. Set it as `DREEVE_API_KEY` in Dreeve's `.env` and recreate the containers. Dreeve must run with `IMPORT_MODE=files`.
3. In VeloPopuli → **Settings → Dreeve**, enter your instance's address and the key and press **Save and test**. The test tells you if the key is wrong or uploads are switched off.

   The address must start with `https://`: VeloPopuli refuses plain `http://` so your key isn't sent unencrypted. Only `localhost` and `127.0.0.1` are allowed without it. If your Dreeve runs on your home network over plain http, put a reverse proxy with a certificate in front of it.

Then:

- Rides upload with one click on the summary or history screen, or automatically (toggle on the Dreeve card).
- Dreeve queues the file and imports it within a few minutes; the button shows what the server said. There is no page to open, so look in Dreeve's activity list.
- Dreeve reads everything from the FIT file (power, heart rate, cadence, the route), so nothing else is sent.
- **Your Dreeve rides** with a GPS track can be raced as ghosts: *From Dreeve…* when setting up a ride.

The key is kept in the system keychain (macOS, Windows) or a file only you can read (Linux).
