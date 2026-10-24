//! How buildings look where they stand: a regional style (wall and roof colours, roof
//! pitch and shape, eaves), adjusted for the kind of building and overridden by whatever
//! OpenStreetMap says of it (`roof:shape`, `roof:colour`, `building:material`, …).

use serde::{Deserialize, Serialize};

use super::height::hash2;
use crate::countries::country;

/// What OpenStreetMap says about a building beyond its outline. All optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildingTags {
    /// `building=*`: "house", "barn", "apartments", … ("yes" or empty when unknown).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub kind: String,
    /// `roof:shape`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub roof_shape: String,
    /// `building:colour` and `roof:colour`, as sRGB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_colour: Option<[u8; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roof_colour: Option<[u8; 3]>,
    /// `building:material` and `roof:material`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub wall_material: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub roof_material: String,
}

/// Broad building traditions, picked by country (and within a few countries, by area).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Mediterranean,
    Alpine,
    British,
    NorthernEurope,
    Nordic,
    EasternEurope,
    NorthAmerica,
    Arid,
    Other,
}

/// The region at a point.
pub fn region_at(lat: f64, lon: f64) -> Region {
    use Region::*;
    let Some(c) = country(lat, lon) else { return Other };
    match c {
        "CH" | "AT" | "LI" | "SI" => Alpine,
        // The Alps reach into these countries' corners.
        "IT" if lat >= 45.9 => Alpine,
        "FR" if lat > 44.3 && lat < 46.5 && lon > 5.6 => Alpine,
        "DE" if lat < 47.75 && lon > 9.6 => Alpine,
        "IT" | "ES" | "PT" | "GR" | "HR" | "MT" | "CY" | "MC" | "SM" | "VA" | "AL" | "ME" | "BA" | "MK" | "TR" => Mediterranean,
        "FR" if lat < 44.3 => Mediterranean,
        "GB" | "IE" | "IM" => British,
        "NO" | "SE" | "FI" | "IS" | "EE" => Nordic,
        "DE" | "NL" | "BE" | "LU" | "DK" | "FR" | "PL" | "CZ" | "SK" | "LV" | "LT" => NorthernEurope,
        "HU" | "RO" | "BG" | "RS" | "UA" | "BY" | "MD" | "RU" | "GE" | "AM" | "AZ" | "KZ" => EasternEurope,
        // The American south-west builds in stucco and tile.
        "US" if lat < 37.5 && lon < -103.0 => Mediterranean,
        "US" | "CA" => NorthAmerica,
        "MA" | "DZ" | "TN" | "LY" | "EG" | "SA" | "AE" | "OM" | "QA" | "KW" | "BH" | "JO" | "IL" | "PS" | "LB" | "SY" | "IQ" | "IR" | "YE" | "MR" | "ML" | "NE" | "TD" | "SD" => Arid,
        _ => Other,
    }
}

/// A region's way of building.
struct Style {
    walls: &'static [[u8; 3]],
    roofs: &'static [[u8; 3]],
    /// Roof pitch of houses, degrees.
    pitch: f64,
    /// How far the roof reaches past the walls, metres.
    eave: f64,
    /// Share of pitched roofs that are hipped rather than gabled.
    hipped: f64,
    /// Share of houses with flat roofs.
    flat: f64,
    /// Timber above the ground floor, as on Alpine chalets.
    upper: Option<&'static [[u8; 3]]>,
    /// Barns and outbuildings: their walls.
    barn: &'static [[u8; 3]],
}

const MEDITERRANEAN: Style = Style {
    walls: &[[236, 219, 180], [230, 200, 150], [214, 170, 120], [240, 232, 214], [222, 180, 150], [245, 240, 230], [210, 190, 160]],
    roofs: &[[178, 92, 58], [160, 80, 52], [190, 112, 72], [150, 90, 70]],
    pitch: 18.0,
    eave: 0.4,
    hipped: 0.5,
    flat: 0.15,
    upper: None,
    barn: &[[190, 170, 140], [170, 150, 120]],
};
const ALPINE: Style = Style {
    walls: &[[240, 236, 226], [232, 226, 214], [226, 218, 200]],
    roofs: &[[80, 70, 62], [95, 85, 78], [70, 70, 72], [120, 82, 60]],
    pitch: 22.0,
    eave: 1.2,
    hipped: 0.1,
    flat: 0.0,
    upper: Some(&[[130, 85, 50], [110, 72, 44], [150, 100, 60]]),
    barn: &[[110, 75, 48], [95, 65, 42]],
};
const BRITISH: Style = Style {
    walls: &[[150, 75, 55], [165, 92, 66], [135, 72, 56], [190, 175, 150], [225, 215, 190]],
    roofs: &[[70, 72, 78], [85, 85, 92], [150, 80, 60]],
    pitch: 40.0,
    eave: 0.25,
    hipped: 0.25,
    flat: 0.03,
    upper: None,
    barn: &[[120, 100, 80], [150, 130, 100]],
};
const NORTHERN: Style = Style {
    walls: &[[160, 80, 60], [175, 110, 80], [230, 225, 210], [215, 205, 185], [200, 195, 185]],
    roofs: &[[165, 70, 50], [140, 60, 45], [60, 60, 65]],
    pitch: 45.0,
    eave: 0.4,
    hipped: 0.2,
    flat: 0.05,
    upper: None,
    barn: &[[125, 95, 70], [150, 80, 60]],
};
const NORDIC: Style = Style {
    walls: &[[140, 40, 35], [140, 40, 35], [225, 190, 110], [235, 235, 230], [190, 195, 195], [200, 160, 90]],
    roofs: &[[45, 45, 48], [120, 45, 40], [90, 92, 96]],
    pitch: 35.0,
    eave: 0.5,
    hipped: 0.05,
    flat: 0.03,
    upper: None,
    barn: &[[140, 40, 35], [120, 36, 32]],
};
const EASTERN: Style = Style {
    walls: &[[225, 215, 190], [210, 200, 180], [200, 205, 190], [170, 100, 75], [220, 200, 170]],
    roofs: &[[80, 110, 90], [120, 125, 130], [150, 70, 55], [110, 85, 70]],
    pitch: 35.0,
    eave: 0.4,
    hipped: 0.4,
    flat: 0.05,
    upper: None,
    barn: &[[120, 100, 80], [100, 90, 80]],
};
const NORTH_AMERICA: Style = Style {
    walls: &[[235, 235, 230], [180, 185, 185], [215, 205, 180], [170, 190, 205], [185, 195, 170], [160, 90, 70]],
    roofs: &[[70, 70, 72], [95, 90, 85], [110, 100, 90]],
    pitch: 28.0,
    eave: 0.4,
    hipped: 0.35,
    flat: 0.05,
    upper: None,
    barn: &[[150, 45, 40], [130, 120, 110]],
};
const ARID: Style = Style {
    walls: &[[220, 200, 160], [205, 180, 140], [235, 225, 200], [240, 238, 232]],
    roofs: &[[200, 185, 155], [215, 205, 185]],
    pitch: 15.0,
    eave: 0.2,
    hipped: 0.3,
    flat: 0.9,
    upper: None,
    barn: &[[200, 180, 145]],
};
const OTHER: Style = Style {
    walls: &[[219, 209, 189], [204, 189, 168], [230, 224, 214], [189, 179, 168], [214, 194, 158], [179, 158, 143]],
    roofs: &[[140, 69, 51], [92, 92, 102], [115, 84, 71]],
    pitch: 30.0,
    eave: 0.4,
    hipped: 0.3,
    flat: 0.2,
    upper: None,
    barn: &[[130, 105, 80]],
};

fn style(region: Region) -> &'static Style {
    match region {
        Region::Mediterranean => &MEDITERRANEAN,
        Region::Alpine => &ALPINE,
        Region::British => &BRITISH,
        Region::NorthernEurope => &NORTHERN,
        Region::Nordic => &NORDIC,
        Region::EasternEurope => &EASTERN,
        Region::NorthAmerica => &NORTH_AMERICA,
        Region::Arid => &ARID,
        Region::Other => &OTHER,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoofShape {
    Flat,
    Gabled,
    Hipped,
    Pyramidal,
    /// One slope, high along one long side.
    Skillion,
}

/// Everything about a building's appearance, decided once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// sRGB, 0–1.
    pub wall: [f64; 3],
    /// Walls above the ground floor, if they differ (timber on a chalet).
    pub upper: Option<[f64; 3]>,
    pub roof: [f64; 3],
    pub shape: RoofShape,
    /// Roof pitch, degrees.
    pub pitch: f64,
    /// Roof overhang past the walls, metres.
    pub eave: f64,
    pub windows: bool,
}

/// Broad kinds of building, from `building=*` (or the size of an unnamed one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    House,
    Small,
    Barn,
    Industrial,
    Apartments,
    Church,
    Greenhouse,
    Garage,
}

fn kind_of(tag: &str, area: f64) -> Kind {
    match tag {
        "house" | "detached" | "semidetached_house" | "bungalow" | "terrace" | "chalet" | "farm" | "cabin" | "villa" | "dormitory" => Kind::House,
        "shed" | "hut" | "kiosk" | "toilets" | "service" | "transformer_tower" | "bunker" => Kind::Small,
        "garage" | "garages" | "carport" => Kind::Garage,
        "barn" | "farm_auxiliary" | "stable" | "cowshed" | "sty" | "silo" | "riding_hall" => Kind::Barn,
        "industrial" | "warehouse" | "manufacture" | "factory" | "retail" | "supermarket" | "storage_tank" | "hangar" | "sports_hall" | "parking" => Kind::Industrial,
        "apartments" | "residential" | "hotel" | "office" | "commercial" | "hospital" | "university" | "college" | "school" | "public" | "civic" | "government" | "train_station" => {
            if area < 250.0 {
                Kind::House
            } else {
                Kind::Apartments
            }
        }
        "church" | "cathedral" | "chapel" | "mosque" | "temple" | "synagogue" | "religious" | "monastery" => Kind::Church,
        "greenhouse" | "glasshouse" => Kind::Greenhouse,
        // Unknown: judge by size.
        _ => match area {
            a if a < 30.0 => Kind::Small,
            a if a < 300.0 => Kind::House,
            a if a < 1500.0 => Kind::Apartments,
            _ => Kind::Industrial,
        },
    }
}

fn srgb(c: [u8; 3]) -> [f64; 3] {
    c.map(|v| v as f64 / 255.0)
}

fn pick(list: &[[u8; 3]], seed: i32, salt: i32) -> [f64; 3] {
    srgb(list[(hash2(seed, salt) * list.len() as f64) as usize % list.len()])
}

fn wall_material(m: &str) -> Option<&'static [[u8; 3]]> {
    Some(match m {
        "brick" => &[[160, 85, 60], [150, 78, 58], [175, 100, 70]],
        "stone" | "sandstone" | "limestone" | "granite" => &[[185, 175, 155], [165, 158, 140], [200, 190, 165]],
        "wood" | "timber_framing" => &[[140, 100, 65], [120, 85, 55]],
        "concrete" | "cement_block" => &[[190, 190, 185], [175, 175, 172]],
        "glass" | "mirror" => &[[150, 175, 190], [130, 160, 180]],
        "metal" | "steel" | "aluminium" => &[[170, 175, 180], [150, 155, 160]],
        "plaster" | "render" | "stucco" => return None,
        _ => return None,
    })
}

fn roof_material(m: &str) -> Option<[u8; 3]> {
    Some(match m {
        "roof_tiles" | "tile" | "tiles" | "clay" => [165, 80, 55],
        "slate" => [70, 72, 78],
        "metal" | "tin" | "copper_patina" => [140, 145, 150],
        "copper" => [110, 150, 130],
        "concrete" | "gravel" => [150, 150, 148],
        "thatch" => [160, 140, 90],
        "wood" | "shingle" => [120, 90, 60],
        "glass" => [180, 200, 205],
        "grass" | "plants" => [100, 130, 70],
        "asphalt" | "tar_paper" | "bitumen" | "eternit" => [72, 72, 74],
        "stone" => [120, 115, 110],
        _ => return None,
    })
}

fn shape_from_tag(s: &str) -> Option<RoofShape> {
    Some(match s {
        "flat" => RoofShape::Flat,
        "gabled" | "gambrel" | "mansard" | "saltbox" | "round" => RoofShape::Gabled,
        "hipped" | "half-hipped" | "side_hipped" => RoofShape::Hipped,
        "pyramidal" | "dome" | "onion" | "cone" => RoofShape::Pyramidal,
        "skillion" | "lean_to" => RoofShape::Skillion,
        _ => return None,
    })
}

/// How a building looks: by region, by kind, and by its own tags. `area` is its footprint in
/// m², `seed` varies the choices from one building to the next.
pub fn look(region: Region, tags: &BuildingTags, area: f64, seed: i32) -> Look {
    let s = style(region);
    let kind = kind_of(tags.kind.as_str(), area);
    let roll = |salt: i32| hash2(seed, salt);
    let mut l = Look {
        wall: pick(s.walls, seed, 1),
        upper: s.upper.map(|u| pick(u, seed, 3)),
        roof: pick(s.roofs, seed, 2),
        shape: if roll(4) < s.flat {
            RoofShape::Flat
        } else if roll(5) < s.hipped {
            RoofShape::Hipped
        } else {
            RoofShape::Gabled
        },
        pitch: s.pitch * (0.9 + roll(6) * 0.2),
        eave: s.eave,
        windows: true,
    };
    match kind {
        Kind::House => {}
        Kind::Small => {
            l.upper = None;
            l.eave = l.eave.min(0.3);
            l.windows = false;
        }
        Kind::Garage => {
            (l.shape, l.upper, l.eave, l.windows) = (if region == Region::Alpine { RoofShape::Gabled } else { RoofShape::Flat }, None, 0.2, false);
            l.wall = srgb([200, 198, 192]);
        }
        Kind::Barn => {
            l.wall = pick(s.barn, seed, 7);
            l.upper = None;
            l.shape = RoofShape::Gabled;
            l.pitch = (s.pitch + 8.0).min(45.0);
            l.windows = false;
            l.eave = l.eave.max(0.5);
        }
        Kind::Industrial => {
            l.wall = pick(&[[200, 200, 195], [180, 185, 190], [210, 205, 195], [170, 176, 180]], seed, 8);
            l.roof = pick(&[[150, 152, 155], [130, 134, 138], [160, 160, 156]], seed, 9);
            l.upper = None;
            l.shape = if roll(10) < 0.5 { RoofShape::Flat } else { RoofShape::Gabled };
            l.pitch = 8.0;
            l.eave = 0.3;
            l.windows = matches!(tags.kind.as_str(), "retail" | "supermarket" | "sports_hall");
        }
        Kind::Apartments => {
            l.upper = None;
            // Blocks are flat-roofed, except where tiled roofs are the custom whatever the size.
            l.shape = match region {
                Region::Mediterranean | Region::Alpine | Region::NorthernEurope | Region::EasternEurope if roll(11) < 0.6 => RoofShape::Hipped,
                _ => RoofShape::Flat,
            };
            l.pitch = l.pitch.min(25.0);
            l.eave = l.eave.min(0.6);
        }
        Kind::Church => {
            l.wall = pick(&[[200, 190, 170], [235, 230, 220], [185, 170, 150]], seed, 12);
            l.upper = None;
            l.shape = RoofShape::Gabled;
            l.pitch = (s.pitch + 12.0).min(50.0);
            l.eave = 0.4;
        }
        Kind::Greenhouse => {
            l.wall = srgb([200, 222, 215]);
            l.roof = srgb([210, 228, 222]);
            l.upper = None;
            (l.shape, l.pitch, l.eave, l.windows) = (RoofShape::Gabled, 25.0, 0.0, false);
        }
    }
    // What the map says wins.
    if let Some(list) = wall_material(&tags.wall_material) {
        l.wall = pick(list, seed, 13);
        l.upper = None;
    }
    if let Some(c) = roof_material(&tags.roof_material) {
        l.roof = srgb(c);
    }
    if let Some(c) = tags.wall_colour {
        l.wall = srgb(c);
        l.upper = None;
    }
    if let Some(c) = tags.roof_colour {
        l.roof = srgb(c);
    }
    if let Some(shape) = shape_from_tag(&tags.roof_shape) {
        l.shape = shape;
    }
    l
}

/// An OpenStreetMap colour value (`#rrggbb`, `#rgb` or a common name) as sRGB.
pub fn parse_colour(v: &str) -> Option<[u8; 3]> {
    let v = v.trim().to_ascii_lowercase();
    if let Some(hex) = v.strip_prefix('#') {
        let n = u32::from_str_radix(hex, 16).ok()?;
        return match hex.len() {
            6 => Some([(n >> 16) as u8, (n >> 8) as u8, n as u8]),
            3 => Some([((n >> 8) & 15) as u8 * 17, ((n >> 4) & 15) as u8 * 17, (n & 15) as u8 * 17]),
            _ => None,
        };
    }
    Some(match v.replace(['_', ' ', '-'], "").as_str() {
        "white" => [240, 240, 236],
        "black" => [40, 40, 42],
        "grey" | "gray" => [140, 140, 140],
        "lightgrey" | "lightgray" | "silver" => [195, 195, 195],
        "darkgrey" | "darkgray" => [90, 90, 92],
        "red" => [170, 60, 50],
        "darkred" | "maroon" => [120, 40, 35],
        "brown" => [125, 85, 60],
        "beige" => [225, 210, 180],
        "cream" | "ivory" => [240, 232, 210],
        "yellow" => [230, 205, 110],
        "orange" => [215, 140, 70],
        "pink" => [225, 170, 165],
        "salmon" => [225, 150, 125],
        "green" => [100, 140, 90],
        "darkgreen" => [60, 95, 65],
        "olive" => [128, 128, 70],
        "blue" => [90, 120, 170],
        "lightblue" => [165, 195, 220],
        "navy" | "darkblue" => [50, 60, 100],
        "tan" => [210, 180, 140],
        "sand" | "sandybrown" => [215, 190, 145],
        "terracotta" => [185, 95, 65],
        "ochre" => [205, 150, 70],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_by_place() {
        assert_eq!(region_at(44.17, 8.34), Region::Mediterranean); // Finale Ligure
        assert_eq!(region_at(46.55, 11.9), Region::Alpine); // Dolomites
        assert_eq!(region_at(45.09, 6.07), Region::Alpine); // Alpe d'Huez
        assert_eq!(region_at(43.6, 3.9), Region::Mediterranean); // Montpellier
        assert_eq!(region_at(48.85, 2.35), Region::NorthernEurope); // Paris
        assert_eq!(region_at(47.0, 8.0), Region::Alpine);
        assert_eq!(region_at(51.25, -0.33), Region::British); // Box Hill
        assert_eq!(region_at(60.39, 5.32), Region::Nordic); // Bergen
        assert_eq!(region_at(39.74, -104.99), Region::NorthAmerica); // Denver
        assert_eq!(region_at(34.05, -118.25), Region::Mediterranean); // Los Angeles
        assert_eq!(region_at(31.63, -8.0), Region::Arid); // Marrakesh
        assert_eq!(region_at(45.0, -30.0), Region::Other);
    }

    #[test]
    fn colours_from_tags() {
        assert_eq!(parse_colour("#ff8000"), Some([255, 128, 0]));
        assert_eq!(parse_colour("#F80"), Some([255, 136, 0]));
        assert_eq!(parse_colour(" Light Grey "), Some([195, 195, 195]));
        assert_eq!(parse_colour("dark_red"), Some([120, 40, 35]));
        assert_eq!(parse_colour("sparkly"), None);
    }

    #[test]
    fn looks_follow_region_kind_and_tags() {
        let tags = |kind: &str| BuildingTags { kind: kind.into(), ..Default::default() };
        // Alpine chalets: timber upper floor, deep eaves.
        let chalet = look(Region::Alpine, &tags("house"), 120.0, 7);
        assert!(chalet.upper.is_some() && chalet.eave > 1.0 && chalet.shape != RoofShape::Flat);
        // British houses are steeper than Mediterranean ones.
        assert!(look(Region::British, &tags("house"), 100.0, 3).pitch > look(Region::Mediterranean, &tags("house"), 100.0, 3).pitch + 10.0);
        // Barns: no windows, gabled; a Nordic barn is falu red.
        let barn = look(Region::Nordic, &tags("barn"), 300.0, 5);
        assert!(!barn.windows && barn.shape == RoofShape::Gabled && barn.wall[0] > barn.wall[1] * 2.0);
        // Arid roofs are nearly all flat.
        assert!((0..50).filter(|&s| look(Region::Arid, &tags("house"), 100.0, s).shape == RoofShape::Flat).count() > 35);
        // The map's own tags win.
        let tagged = BuildingTags { kind: "house".into(), roof_shape: "pyramidal".into(), roof_colour: Some([10, 20, 30]), wall_material: "brick".into(), ..Default::default() };
        let l = look(Region::Alpine, &tagged, 100.0, 1);
        assert_eq!(l.shape, RoofShape::Pyramidal);
        assert_eq!(l.roof, srgb([10, 20, 30]));
        assert!(l.upper.is_none() && l.wall[0] > 0.55 && l.wall[2] < 0.3);
    }
}
