//! Where the lowland tropics are, and the Americas: what decides palms and rain trees, and
//! the tropical roadside, and yellow road paint. Outside both, nothing changes.

use super::height::smoothstep;

/// Latitudes (degrees from the equator) over which the tropics fade out.
const TROPIC_LAT: (f64, f64) = (21.0, 25.0);
/// Heights (metres) over which the lowlands give way to highland country.
const LOWLAND_M: (f64, f64) = (900.0, 1500.0);
/// Longitudes of the Americas, west to east.
const AMERICAS_LON: (f64, f64) = (-170.0, -30.0);

/// How tropical a place is, from 0 (not at all) to 1: in the lowland tropics, blending out
/// towards the tropics' edge and up into the highlands. `lat_abs` is degrees from the equator.
pub fn tropical_weight(lat_abs: f64, height: f64) -> f64 {
    (1.0 - smoothstep(TROPIC_LAT.0, TROPIC_LAT.1, lat_abs)) * (1.0 - smoothstep(LOWLAND_M.0, LOWLAND_M.1, height))
}

/// Whether a longitude is in the Americas, where roads are painted with yellow centre lines.
pub fn in_americas(lon: f64) -> bool {
    (AMERICAS_LON.0..=AMERICAS_LON.1).contains(&lon)
}
