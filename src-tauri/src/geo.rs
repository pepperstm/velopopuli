//! Finding places and planning routes inside the app: place search through
//! OpenStreetMap's Nominatim, bike routing (with elevation) through BRouter, and
//! waymarked mountain-bike routes through the Overpass API.

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    name: String,
    detail: String,
    lat: f64,
    lon: f64,
}

pub async fn search(http: &reqwest::Client, query: &str) -> Result<Vec<Place>, String> {
    let url = reqwest::Url::parse_with_params("https://nominatim.openstreetmap.org/search", &[("q", query), ("format", "jsonv2"), ("limit", "8")]).map_err(|e| e.to_string())?;
    let res = http.get(url).send().await.map_err(|e| format!("Could not reach the place search: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("The place search answered {}.", res.status()));
    }
    let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    Ok(json
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let display = p["display_name"].as_str().unwrap_or("");
            let name = p["name"].as_str().filter(|n| !n.is_empty()).unwrap_or_else(|| display.split(',').next().unwrap_or(""));
            Some(Place {
                name: name.to_string(),
                detail: display.split(", ").skip(1).take(3).collect::<Vec<_>>().join(", "),
                lat: p["lat"].as_str()?.parse().ok()?,
                lon: p["lon"].as_str()?.parse().ok()?,
            })
        })
        .collect())
}

/// Routes through the given `[lat, lon]` waypoints and returns the GPX (with elevation).
pub async fn plan(http: &reqwest::Client, points: &[[f64; 2]], profile: &str) -> Result<String, String> {
    if points.len() < 2 {
        return Err("Add at least a start and a finish.".into());
    }
    if points.len() > 30 {
        return Err("Use at most 30 waypoints.".into());
    }
    let profile = match profile {
        "trekking" | "safety" | "fastbike" | "mtb" => profile,
        _ => "fastbike",
    };
    let lonlats = points.iter().map(|p| format!("{:.6},{:.6}", p[1], p[0])).collect::<Vec<_>>().join("|");
    let url = format!("https://brouter.de/brouter?lonlats={lonlats}&profile={profile}&alternativeidx=0&format=gpx");
    let res = http.get(url).timeout(std::time::Duration::from_secs(60)).send().await.map_err(|e| format!("Could not reach the route planner: {e}"))?;
    let status = res.status();
    let text = res.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() || !text.contains("<trkpt") {
        let reason = text.lines().next().unwrap_or("").chars().take(160).collect::<String>();
        return Err(format!("No route found between those points{}", if reason.is_empty() { ".".into() } else { format!(": {reason}") }));
    }
    Ok(text)
}

/// The waymarked mountain-bike routes within `[south, west, north, east]`.
pub async fn mtb_routes(http: &reqwest::Client, bounds: [f64; 4]) -> Result<Vec<velopopuli_core::waymarked::WaymarkedSummary>, String> {
    let [s, w, n, e] = bounds;
    // About 60 km across at most: wider areas hold too many routes for a public server.
    if (n - s).abs() > 0.6 || (e - w).abs() > 0.9 {
        return Err("Zoom in a little to search this area.".into());
    }
    let text = crate::mapdata::overpass_first(http, &velopopuli_core::waymarked::area_query(bounds), std::time::Duration::from_secs(30)).await?;
    velopopuli_core::waymarked::parse_area(&text)
}

/// One waymarked route's ways, joined into one line.
pub async fn mtb_route(http: &reqwest::Client, id: u64) -> Result<velopopuli_core::waymarked::WaymarkedLine, String> {
    let text = crate::mapdata::overpass_first(http, &velopopuli_core::waymarked::route_query(id), std::time::Duration::from_secs(45)).await?;
    velopopuli_core::waymarked::parse_route(&text)
}

/// Famous climbs, each as waypoints from the foot to the summit.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Classic {
    name: &'static str,
    region: &'static str,
    points: &'static [[f64; 2]],
}

pub const CLASSICS: &[Classic] = &[
    Classic { name: "Alpe d'Huez", region: "French Alps · from Bourg d'Oisans", points: &[[45.0556, 6.0303], [45.0917, 6.0706]] },
    Classic { name: "Mont Ventoux", region: "Provence · from Bédoin", points: &[[44.1247, 5.1797], [44.1741, 5.2786]] },
    Classic { name: "Passo dello Stelvio", region: "Italian Alps · from Prato", points: &[[46.6197, 10.5937], [46.5287, 10.4532]] },
    Classic { name: "Col du Galibier", region: "French Alps · from Valloire", points: &[[45.1652, 6.4286], [45.0641, 6.4078]] },
    Classic { name: "Col du Tourmalet", region: "Pyrenees · from Luz-Saint-Sauveur", points: &[[42.8722, -0.0036], [42.8977, 0.0647], [42.9086, 0.1453]] },
    Classic { name: "Passo del Mortirolo", region: "Italian Alps · from Mazzo", points: &[[46.2578, 10.2565], [46.2472, 10.2977]] },
    Classic { name: "Monte Zoncolan", region: "Carnic Alps · from Ovaro", points: &[[46.4830, 12.8648], [46.5007, 12.9271]] },
    Classic { name: "Box Hill", region: "Surrey · the zig-zag road", points: &[[51.2535, -0.3236], [51.2495, -0.3110]] },
];

#[cfg(test)]
mod tests {
    use velopopuli_core::gpx::{build_route, parse_gpx};

    /// Needs the network: `cargo test -p velopopuli -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn plans_every_classic_climb() {
        let http = reqwest::Client::builder().user_agent("VeloPopuli/0.2 (open-source indoor cycling app)").build().unwrap();
        let found = super::search(&http, "Bourg d'Oisans").await.unwrap();
        assert!(found.iter().any(|p| (p.lat - 45.05).abs() < 0.05), "{}", found.len());
        for c in super::CLASSICS {
            let gpx = super::plan(&http, c.points, "fastbike").await.unwrap();
            let route = build_route(c.name, &parse_gpx(&gpx).unwrap().1).unwrap();
            println!("{}: {:.1} km, {:.0} m up, {} climbs", c.name, route.total / 1000.0, route.ascent, route.climbs.len());
            assert!(route.ascent > 100.0 && route.climbs.len() >= 1, "{}", c.name);
        }
    }

    /// Needs the network: `cargo test -p velopopuli -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn finds_and_joins_waymarked_mtb_routes() {
        let http = reqwest::Client::builder().user_agent("VeloPopuli/0.3 (open-source indoor cycling app)").build().unwrap();
        // Finale Ligure: one of Europe's best-mapped mountain-bike areas.
        let found = super::mtb_routes(&http, [44.12, 8.22, 44.28, 8.42]).await.unwrap();
        println!("{} routes", found.len());
        assert!(!found.is_empty());
        for r in found.iter().take(5) {
            let line = super::mtb_route(&http, r.id).await.unwrap();
            let raw: Vec<_> = line.line.iter().map(|p| velopopuli_core::gpx::RawPoint { lat: p[0], lon: p[1], ele: None }).collect();
            let route = build_route(&line.name, &raw).unwrap();
            println!("{} ({}): {:.1} km, {} gaps, mapped {:?} km", line.name, r.id, route.total / 1000.0, line.gaps, r.distance.map(|d| d / 1000.0));
            assert!(route.total > 500.0);
        }
        assert!(super::mtb_routes(&http, [44.0, 8.0, 45.0, 9.0]).await.is_err(), "too wide an area is refused");
    }
}
