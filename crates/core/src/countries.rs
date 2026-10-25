//! Which country a point is in, from simplified Natural Earth outlines (public domain,
//! built by `tools/countries.py`). Coarse — borders are good to a kilometre or so where
//! it matters (countries with national imagery) and a few kilometres elsewhere — but
//! enough to pick an imagery source or a regional style.

use std::sync::OnceLock;

struct Ring {
    code: [u8; 2],
    /// From the sharper outlines: a point outside it is only taken to be in it when very close.
    detailed: bool,
    /// `[south, west, north, east]`, to skip most rings without testing every edge.
    bounds: [f64; 4],
    /// `[lat, lon]` points; the last joins back to the first.
    points: Vec<[f64; 2]>,
}

fn rings() -> &'static [Ring] {
    static RINGS: OnceLock<Vec<Ring>> = OnceLock::new();
    RINGS.get_or_init(|| {
        include_str!("countries.txt")
            .lines()
            .filter_map(|line| {
                let (code, rest) = line.split_once(' ')?;
                let points: Vec<[f64; 2]> = rest
                    .split(' ')
                    .filter_map(|p| {
                        let (lat, lon) = p.split_once(',')?;
                        Some([lat.parse().ok()?, lon.parse().ok()?])
                    })
                    .collect();
                let bounds = points.iter().fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]);
                let (code, detailed) = code.strip_suffix('+').map_or((code, false), |c| (c, true));
                let code = code.as_bytes();
                (code.len() == 2 && code.iter().all(u8::is_ascii_uppercase) && points.len() >= 3).then(|| Ring { code: [code[0], code[1]], detailed, bounds, points })
            })
            .collect()
    })
}

fn inside(points: &[[f64; 2]], lat: f64, lon: f64) -> bool {
    let mut odd = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let ([ai, oi], [aj, oj]) = (points[i], points[j]);
        if (ai > lat) != (aj > lat) && lon < oi + (lat - ai) / (aj - ai) * (oj - oi) {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// How far outside every outline a point can be and still count as in the nearest
/// country, in degrees (about 15 km): simplified coasts cut corners, and a coarse
/// outline doesn't quite meet a detailed neighbour's.
const NEAR_DEG: f64 = 0.15;
/// The same for a detailed outline (about a kilometre, its simplification): a point
/// further outside one really is in the neighbouring country.
const NEAR_DETAILED_DEG: f64 = 0.01;

/// Squared distance in degrees from `(lat, lon)` to a ring's edges, longitude scaled by `cos`.
fn edge_distance2(points: &[[f64; 2]], lat: f64, lon: f64, cos: f64) -> f64 {
    let mut best = f64::INFINITY;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let (a, b) = (points[j], points[i]);
        let (ax, ay, bx, by) = ((a[1] - lon) * cos, a[0] - lat, (b[1] - lon) * cos, b[0] - lat);
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 { (-(ax * dx + ay * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        best = best.min((ax + dx * t).powi(2) + (ay + dy * t).powi(2));
        j = i;
    }
    best
}

fn code(r: &'static Ring) -> &'static str {
    // Codes are ASCII letters, checked when the outlines are read.
    std::str::from_utf8(&r.code).unwrap_or("")
}

/// The ISO 3166-1 alpha-2 code of the country at `(lat, lon)`: the outline it is in, or else
/// the nearest within about 15 km. `None` out at sea or in a microstate too small to be outlined.
pub fn country(lat: f64, lon: f64) -> Option<&'static str> {
    let rings = rings();
    if let Some(r) = rings.iter().find(|r| lat >= r.bounds[0] && lat <= r.bounds[2] && lon >= r.bounds[1] && lon <= r.bounds[3] && inside(&r.points, lat, lon)) {
        return Some(code(r));
    }
    let cos = lat.to_radians().cos().max(0.01);
    let reach = NEAR_DEG / cos;
    rings
        .iter()
        .filter(|r| lat >= r.bounds[0] - NEAR_DEG && lat <= r.bounds[2] + NEAR_DEG && lon >= r.bounds[1] - reach && lon <= r.bounds[3] + reach)
        .map(|r| (r, edge_distance2(&r.points, lat, lon, cos)))
        .filter(|(r, d)| *d < if r.detailed { NEAR_DETAILED_DEG * NEAR_DETAILED_DEG } else { NEAR_DEG * NEAR_DEG })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(r, _)| code(r))
}

#[cfg(test)]
mod tests {
    use super::country;

    #[test]
    fn finds_countries_and_their_borders() {
        assert_eq!(country(48.857, 2.352), Some("FR")); // Paris
        assert_eq!(country(46.948, 7.447), Some("CH")); // Bern
        assert_eq!(country(46.204, 6.143), Some("CH")); // Geneva, nearly surrounded by France
        assert_eq!(country(46.193, 6.236), Some("FR")); // Annemasse, on Geneva's doorstep
        assert_eq!(country(47.56, 7.59), Some("CH")); // Basel
        assert_eq!(country(47.615, 7.66), Some("DE")); // Lörrach, a few kilometres over the border
        assert_eq!(country(43.70, 7.27), Some("FR")); // Nice, on the coast
        assert_eq!(country(52.37, 4.89), Some("NL")); // Amsterdam
        assert_eq!(country(51.22, 4.40), Some("BE")); // Antwerp
        assert_eq!(country(40.42, -3.70), Some("ES")); // Madrid
        assert_eq!(country(39.57, 2.65), Some("ES")); // Palma, Mallorca
        assert_eq!(country(44.17, 8.34), Some("IT")); // Finale Ligure
        assert_eq!(country(39.74, -104.99), Some("US")); // Denver
        assert_eq!(country(45.50, -73.57), Some("CA")); // Montreal
        assert_eq!(country(54.6, -2.5), Some("GB"));
        assert_eq!(country(45.0, -30.0), None); // mid-Atlantic
    }
}
