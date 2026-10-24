//! The weather where the route is, now: what Open-Meteo reports, and what it means for the
//! ride — the wind on the rider, and how wet, snowy, cloudy and hazy to draw the world.

use serde::{Deserialize, Serialize};

/// Current conditions at one place.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conditions {
    /// WMO weather code.
    pub code: u32,
    /// °C.
    pub temperature: f64,
    /// Share of the sky under cloud, 0–1.
    pub cloud_cover: f64,
    /// Rain (and showers), mm an hour.
    pub rain: f64,
    /// Snowfall, cm an hour.
    pub snowfall: f64,
    /// How far you can see, metres.
    pub visibility: f64,
    /// Wind 10 m above the ground (as weather reports give it), m/s.
    pub wind_speed: f64,
    pub wind_gusts: f64,
    /// Where the wind blows from, degrees clockwise from north.
    pub wind_from: f64,
    /// Snow lying on the ground, metres.
    pub snow_depth: f64,
    /// Whether the roads are wet: rain now or in the last few hours.
    pub wet: bool,
}

/// The Open-Meteo request for the conditions at a place, with the last three hours' rain.
pub fn conditions_url(lat: f64, lon: f64) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat:.4}&longitude={lon:.4}\
         &current=weather_code,temperature_2m,cloud_cover,rain,showers,snowfall,visibility,wind_speed_10m,wind_direction_10m,wind_gusts_10m,snow_depth\
         &hourly=precipitation&past_hours=3&forecast_hours=1&wind_speed_unit=ms&timezone=GMT"
    )
}

#[derive(Deserialize)]
struct Reply {
    current: Current,
    #[serde(default)]
    hourly: Option<Hourly>,
}

#[derive(Deserialize)]
struct Current {
    #[serde(default)]
    interval: Option<f64>,
    weather_code: Option<u32>,
    temperature_2m: Option<f64>,
    cloud_cover: Option<f64>,
    rain: Option<f64>,
    showers: Option<f64>,
    snowfall: Option<f64>,
    visibility: Option<f64>,
    wind_speed_10m: Option<f64>,
    wind_direction_10m: Option<f64>,
    wind_gusts_10m: Option<f64>,
    snow_depth: Option<f64>,
}

#[derive(Deserialize)]
struct Hourly {
    #[serde(default)]
    precipitation: Vec<Option<f64>>,
}

/// Reads an Open-Meteo reply (see `conditions_url`). Missing readings count as calm and dry.
pub fn parse_conditions(json: &str) -> Result<Conditions, String> {
    let reply: Reply = serde_json::from_str(json).map_err(|e| format!("Unexpected reply from the weather service: {e}"))?;
    let c = reply.current;
    // Rain and snowfall are totals over the reading's interval (15 minutes).
    let per_hour = 3600.0 / c.interval.filter(|i| *i > 0.0).unwrap_or(3600.0);
    let rain = (c.rain.unwrap_or(0.0) + c.showers.unwrap_or(0.0)) * per_hour;
    let snowfall = c.snowfall.unwrap_or(0.0) * per_hour;
    let code = c.weather_code.ok_or("The weather service gave no reading.")?;
    let recent: f64 = reply.hourly.map_or(0.0, |h| h.precipitation.iter().flatten().sum());
    let wind_speed = c.wind_speed_10m.unwrap_or(0.0).max(0.0);
    Ok(Conditions {
        code,
        temperature: c.temperature_2m.unwrap_or(12.0),
        cloud_cover: (c.cloud_cover.unwrap_or(0.0) / 100.0).clamp(0.0, 1.0),
        rain,
        snowfall,
        visibility: c.visibility.unwrap_or(30_000.0).max(20.0),
        wind_speed,
        wind_gusts: c.wind_gusts_10m.unwrap_or(wind_speed).max(wind_speed),
        wind_from: c.wind_direction_10m.unwrap_or(0.0).rem_euclid(360.0),
        snow_depth: c.snow_depth.unwrap_or(0.0).max(0.0),
        wet: rain > 0.05 || recent > 0.3 || (51..=67).contains(&code) || (80..=82).contains(&code),
    })
}

/// How much of the 10 m wind reaches a rider: wind slows near the ground (over open
/// country, about two-thirds is left at the height of a rider's body), and hedges, trees
/// and buildings take more.
pub const RIDER_HEIGHT_SHARE: f64 = 0.6;

/// The headwind a rider feels, m/s (negative is a tailwind), riding in the direction
/// `(east, north)` (a unit vector) in a wind of `speed` m/s at 10 m from `from_deg`.
/// Crosswinds don't slow a rider much and are left out.
pub fn headwind(speed: f64, from_deg: f64, east: f64, north: f64) -> f64 {
    let from = from_deg.to_radians();
    speed * RIDER_HEIGHT_SHARE * (from.sin() * east + from.cos() * north)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_open_meteo() {
        let json = r#"{"current":{"interval":900,"weather_code":61,"temperature_2m":8.1,"cloud_cover":100,"rain":0.5,"showers":0.0,
            "snowfall":0.0,"visibility":12000.0,"wind_speed_10m":6.0,"wind_direction_10m":-90,"wind_gusts_10m":11.0,"snow_depth":0.0},
            "hourly":{"precipitation":[0.0,0.4,null,1.0]}}"#;
        let c = parse_conditions(json).unwrap();
        assert_eq!((c.code, c.cloud_cover, c.rain, c.wind_from, c.wet), (61, 1.0, 2.0, 270.0, true));
        let dry = parse_conditions(r#"{"current":{"weather_code":1,"wind_speed_10m":2.0}}"#).unwrap();
        assert!(!dry.wet && dry.wind_gusts == 2.0 && dry.visibility > 10_000.0);
        assert!(parse_conditions(r#"{"current":{}}"#).is_err());
        assert!(parse_conditions("rate limited").is_err());
    }

    #[test]
    fn wind_on_the_rider() {
        // Riding north into a northerly is a headwind; south, a tailwind; east, a crosswind.
        assert!((headwind(10.0, 0.0, 0.0, 1.0) - 10.0 * RIDER_HEIGHT_SHARE).abs() < 1e-9);
        assert!((headwind(10.0, 0.0, 0.0, -1.0) + 10.0 * RIDER_HEIGHT_SHARE).abs() < 1e-9);
        assert!(headwind(10.0, 0.0, 1.0, 0.0).abs() < 1e-9);
        // A westerly (from 270°) on someone riding west.
        assert!((headwind(10.0, 270.0, -1.0, 0.0) - 10.0 * RIDER_HEIGHT_SHARE).abs() < 1e-9);
    }
}
