//! Satellite and aerial imagery for the ground: which source to use where, and turning
//! web-map tiles (256 px squares in Web Mercator) into one image per terrain chunk.
//!
//! National orthophotos with open licences where there are any, and Sentinel-2 cloudless
//! (10 m, CC BY 4.0) everywhere else. A rider can add their own provider instead, with a
//! URL template that holds their key. Downloading and caching tiles is the app's job; this
//! only says which tiles it needs and puts them together.

use std::collections::HashMap;
use std::sync::Arc;

use super::CHUNK;
use crate::countries::country;
use crate::gpx::Route;

/// A web-map tile service.
#[derive(Debug, Clone, PartialEq)]
pub struct Provider {
    /// Short name, used for the disk cache.
    pub id: String,
    /// URL with `{z}`, `{x}` and `{y}` for the tile.
    pub url: String,
    /// Deepest zoom level it serves.
    pub max_zoom: u8,
    /// Credit shown while riding over it.
    pub credit: String,
    /// The country it covers, or `None` for everywhere.
    pub country: Option<&'static str>,
}

struct Builtin {
    id: &'static str,
    url: &'static str,
    max_zoom: u8,
    credit: &'static str,
    country: Option<&'static str>,
}

/// Open sources, national ones first. The last covers the world and is the fallback.
const BUILTIN: [Builtin; 6] = [
    Builtin {
        id: "swisstopo",
        url: "https://wmts.geo.admin.ch/1.0.0/ch.swisstopo.swissimage/default/current/3857/{z}/{x}/{y}.jpeg",
        max_zoom: 19,
        credit: "SWISSIMAGE © swisstopo",
        country: Some("CH"),
    },
    Builtin {
        id: "pdok",
        url: "https://service.pdok.nl/hwh/luchtfotorgb/wmts/v1_0/Actueel_orthoHR/EPSG:3857/{z}/{x}/{y}.jpeg",
        max_zoom: 19,
        credit: "Luchtfoto © Beeldmateriaal Nederland, CC BY 4.0",
        country: Some("NL"),
    },
    Builtin {
        id: "pnoa",
        url: "https://www.ign.es/wmts/pnoa-ma?request=GetTile&service=WMTS&VERSION=1.0.0&Layer=OI.OrthoimageCoverage&Style=default&Format=image/jpeg&TileMatrixSet=GoogleMapsCompatible&TileMatrix={z}&TileRow={y}&TileCol={x}",
        max_zoom: 19,
        credit: "PNOA © Instituto Geográfico Nacional de España, CC BY 4.0",
        country: Some("ES"),
    },
    Builtin {
        id: "ign",
        url: "https://data.geopf.fr/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0&LAYER=ORTHOIMAGERY.ORTHOPHOTOS&STYLE=normal&TILEMATRIXSET=PM&TILEMATRIX={z}&TILEROW={y}&TILECOL={x}&FORMAT=image/jpeg",
        max_zoom: 19,
        credit: "BD ORTHO © IGN, Licence Ouverte 2.0",
        country: Some("FR"),
    },
    Builtin {
        id: "usgs",
        url: "https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer/tile/{z}/{y}/{x}",
        max_zoom: 16,
        credit: "USGS The National Map: orthoimagery (public domain)",
        country: Some("US"),
    },
    Builtin {
        id: "s2cloudless-2017",
        url: "https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless-2017_3857/default/g/{z}/{y}/{x}.jpg",
        // 10 m data: deeper levels are only enlarged.
        max_zoom: 14,
        credit: "Sentinel-2 cloudless 2017 by EOX IT Services GmbH (contains modified Copernicus Sentinel data 2017), CC BY 4.0",
        country: None,
    },
];

/// The providers for a ride: the open ones, or the rider's own (with the worldwide open
/// one behind it, for wherever it has no tiles).
pub fn providers(custom_url: &str) -> Vec<Provider> {
    let own = |b: &Builtin| Provider { id: b.id.into(), url: b.url.into(), max_zoom: b.max_zoom, credit: b.credit.into(), country: b.country };
    let custom = custom_url.trim();
    if custom.contains("{z}") && custom.contains("{x}") && custom.contains("{y}") {
        let host = custom.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or("custom");
        let custom = Provider { id: format!("custom-{:016x}", fnv(custom)), url: custom.into(), max_zoom: 19, credit: format!("Imagery from {host}"), country: None };
        vec![custom, own(&BUILTIN[BUILTIN.len() - 1])]
    } else {
        BUILTIN.iter().map(own).collect()
    }
}

/// A stable hash, so a custom provider's cache folder doesn't show its key.
fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

/// The URL of one tile.
pub fn tile_url(p: &Provider, z: u8, x: u32, y: u32) -> String {
    p.url.replace("{z}", &z.to_string()).replace("{x}", &x.to_string()).replace("{y}", &y.to_string())
}

/// Which provider to use at a point: the first that covers its country.
pub fn provider_at(providers: &[Provider], lat: f64, lon: f64) -> usize {
    let here = country(lat, lon);
    providers.iter().position(|p| p.country.is_none() || p.country == here).unwrap_or(providers.len() - 1)
}

const TILE_PX: usize = 256;
/// Earth's circumference at the equator, in Web Mercator metres.
const EQUATOR_M: f64 = 40_075_016.686;

/// A point's position in pixels on the whole map at zoom `z`.
fn map_px(lat: f64, lon: f64, z: u8) -> (f64, f64) {
    let size = (TILE_PX << z) as f64;
    let lat = lat.clamp(-85.05, 85.05).to_radians();
    ((lon + 180.0) / 360.0 * size, (1.0 - lat.tan().asinh() / std::f64::consts::PI) / 2.0 * size)
}

/// The shallowest zoom whose pixels are no bigger than `metres` at latitude `lat`, up to `max`.
pub fn zoom_for(metres: f64, lat: f64, max: u8) -> u8 {
    let at_zero = EQUATOR_M * lat.to_radians().cos() / TILE_PX as f64;
    ((at_zero / metres).log2().ceil().max(0.0) as u8).min(max)
}

/// One decoded tile: 256 × 256 RGB, sRGB.
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub rgb: Vec<u8>,
}

impl Tile {
    /// `None` unless it is a full 256 × 256 RGB tile with something on it: some services
    /// answer outside their coverage with a tile of one flat colour.
    pub fn new(rgb: Vec<u8>) -> Option<Tile> {
        if rgb.len() != TILE_PX * TILE_PX * 3 {
            return None;
        }
        let (mut lo, mut hi) = ([255u8; 3], [0u8; 3]);
        // A sparse sample is enough to tell a picture from a blank.
        for px in rgb.as_chunks::<3>().0.iter().step_by(97) {
            for k in 0..3 {
                lo[k] = lo[k].min(px[k]);
                hi[k] = hi[k].max(px[k]);
            }
        }
        (0..3).any(|k| hi[k] - lo[k] > 6).then_some(Tile { rgb })
    }
}

/// A terrain chunk's ground image: `size` × `size` RGBA, sRGB, rows running along +z.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkImage {
    pub size: usize,
    pub rgba: Vec<u8>,
    /// Credits for the providers it came from.
    pub credits: Vec<String>,
}

/// Sizes a chunk image comes in: fine near the rider, coarse further out.
pub const NEAR_PX: usize = 256;
pub const FAR_PX: usize = 128;

/// Reads colours off the tiles of several providers at once, each at its own zoom, fetching
/// every tile once. Where a provider lacks a tile the last provider stands in.
struct Sampler<F> {
    zooms: Vec<u8>,
    tiles: HashMap<(usize, u32, u32), Option<Arc<Tile>>>,
    fetch: F,
    used: Vec<bool>,
}

impl<F: FnMut(usize, u8, u32, u32) -> Option<Arc<Tile>>> Sampler<F> {
    /// For pixels of `metres` at latitude `lat`.
    fn new(providers: &[Provider], metres: f64, lat: f64, fetch: F) -> Self {
        Self { zooms: providers.iter().map(|p| zoom_for(metres, lat, p.max_zoom)).collect(), tiles: HashMap::new(), fetch, used: vec![false; providers.len()] }
    }

    /// Bilinear sample of provider `p` at a point; `None` if a tile is missing.
    fn one(&mut self, p: usize, lat: f64, lon: f64) -> Option<[f64; 3]> {
        let z = self.zooms[p];
        let (gx, gy) = map_px(lat, lon, z);
        let (fx, fy) = (gx - 0.5, gy - 0.5);
        let (ix, iy) = (fx.floor() as i64, fy.floor() as i64);
        let (tx, ty) = (fx - ix as f64, fy - iy as f64);
        let mut out = [0.0; 3];
        for (dx, dy, w) in [(0, 0, (1.0 - tx) * (1.0 - ty)), (1, 0, tx * (1.0 - ty)), (0, 1, (1.0 - tx) * ty), (1, 1, tx * ty)] {
            let (px, py) = ((ix + dx).max(0) as u64, (iy + dy).max(0) as u64);
            let (x, y) = ((px / TILE_PX as u64) as u32, (py / TILE_PX as u64) as u32);
            let fetch = &mut self.fetch;
            let t = self.tiles.entry((p, x, y)).or_insert_with(|| fetch(p, z, x, y)).clone()?;
            let at = ((py as usize % TILE_PX) * TILE_PX + px as usize % TILE_PX) * 3;
            for (o, &c) in out.iter_mut().zip(&t.rgb[at..at + 3]) {
                *o += c as f64 * w;
            }
        }
        Some(out)
    }

    /// Provider `p`'s colour at a point, or the fallback's (sRGB, 0–255).
    fn colour(&mut self, p: usize, lat: f64, lon: f64) -> Option<[f64; 3]> {
        let fallback = self.zooms.len() - 1;
        for p in [p, fallback] {
            if let Some(c) = self.one(p, lat, lon) {
                self.used[p] = true;
                return Some(c);
            }
        }
        None
    }

    fn credits(&self, providers: &[Provider]) -> Vec<String> {
        providers.iter().zip(&self.used).filter(|(_, &u)| u).map(|(p, _)| p.credit.clone()).collect()
    }
}

/// Builds the image for chunk `(cx, cz)` at `size` pixels a side. `fetch` gets a tile
/// (provider index, zoom, x, y), or `None` if there isn't one. Where a provider has no tile
/// the last provider fills in; `None` if that has nothing either.
pub fn chunk_image(route: &Route, cx: i32, cz: i32, size: usize, providers: &[Provider], fetch: impl FnMut(usize, u8, u32, u32) -> Option<Arc<Tile>>) -> Option<ChunkImage> {
    let (x0, z0) = (cx as f64 * CHUNK, cz as f64 * CHUNK);
    let pixel = CHUNK / size as f64;
    // Which provider serves each pixel, decided on a coarse grid: a country lookup per pixel
    // would be slow, and borders are only known to the nearest kilometre anyway.
    const GRID: usize = 16;
    let which: Vec<usize> = (0..GRID * GRID)
        .map(|k| {
            let (lat, lon) = route.to_lat_lon(x0 + ((k % GRID) as f64 + 0.5) * CHUNK / GRID as f64, z0 + ((k / GRID) as f64 + 0.5) * CHUNK / GRID as f64);
            provider_at(providers, lat, lon)
        })
        .collect();
    let (lat_mid, _) = route.to_lat_lon(x0 + CHUNK / 2.0, z0 + CHUNK / 2.0);
    let mut sampler = Sampler::new(providers, pixel, lat_mid, fetch);
    let mut rgba = vec![255u8; size * size * 4];
    for j in 0..size {
        for i in 0..size {
            let (lat, lon) = route.to_lat_lon(x0 + (i as f64 + 0.5) * pixel, z0 + (j as f64 + 0.5) * pixel);
            let c = sampler.colour(which[(j * GRID / size) * GRID + i * GRID / size], lat, lon)?;
            let at = (j * size + i) * 4;
            for k in 0..3 {
                rgba[at + k] = c[k].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Some(ChunkImage { size, rgba, credits: sampler.credits(providers) })
}

/// Colours for points of the distant landscape, `(x, z)` each standing for a square of
/// `cell` metres: linear RGB, ready for vertex colours. `None` if any point has no imagery.
pub fn far_colours(route: &Route, points: &[(f64, f64)], cell: f64, providers: &[Provider], fetch: impl FnMut(usize, u8, u32, u32) -> Option<Arc<Tile>>) -> Option<Vec<[f32; 3]>> {
    let mid = points.get(points.len() / 2)?;
    let mut sampler = Sampler::new(providers, cell, route.to_lat_lon(mid.0, mid.1).0, fetch);
    points
        .iter()
        .map(|&(x, z)| {
            let (lat, lon) = route.to_lat_lon(x, z);
            let c = sampler.colour(provider_at(providers, lat, lon), lat, lon)?;
            Some(c.map(|v| (v / 255.0).powf(2.2) as f32))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpx::{build_route, RawPoint, Route};

    /// A short route centred on `(lat, lon)`.
    fn route_at(lat: f64, lon: f64) -> Route {
        let raw: Vec<RawPoint> = (0..20).map(|k| RawPoint { lat: lat + (k as f64 - 9.5) * 1e-4, lon, ele: Some(500.0) }).collect();
        build_route("test", &raw).unwrap()
    }

    #[test]
    fn picks_national_imagery_where_there_is_some() {
        let p = providers("");
        let id = |lat, lon| p[provider_at(&p, lat, lon)].id.as_str();
        assert_eq!(id(46.948, 7.447), "swisstopo");
        assert_eq!(id(52.37, 4.89), "pdok");
        assert_eq!(id(40.42, -3.70), "pnoa");
        assert_eq!(id(45.92, 6.87), "ign"); // Chamonix
        assert_eq!(id(39.74, -104.99), "usgs");
        assert_eq!(id(44.17, 8.34), "s2cloudless-2017"); // Italy has no open national imagery
        assert_eq!(id(45.0, -30.0), "s2cloudless-2017"); // nor does the sea
    }

    #[test]
    fn a_riders_own_provider_comes_first() {
        let p = providers("https://api.example.com/v4/sat/{z}/{x}/{y}.jpg?access_token=SECRET");
        assert_eq!(p.len(), 2);
        assert_eq!(provider_at(&p, 46.948, 7.447), 0);
        assert_eq!(p[0].credit, "Imagery from api.example.com");
        assert!(!p[0].id.contains("SECRET"));
        assert_eq!(tile_url(&p[0], 17, 1, 2), "https://api.example.com/v4/sat/17/1/2.jpg?access_token=SECRET");
        // Without all three placeholders it isn't a tile URL: the open sources stay.
        assert_eq!(providers("https://example.com/tiles").len(), BUILTIN.len());
    }

    #[test]
    fn zoom_follows_the_pixel_size() {
        // A 240 m chunk at 256 px is about 0.94 m a pixel: zoom 17 at mid latitudes.
        assert_eq!(zoom_for(CHUNK / NEAR_PX as f64, 45.0, 19), 17);
        assert_eq!(zoom_for(CHUNK / FAR_PX as f64, 45.0, 19), 16);
        assert_eq!(zoom_for(CHUNK / NEAR_PX as f64, 45.0, 14), 14);
        // Pixels shrink towards the poles, so the same detail needs less zoom.
        assert!(zoom_for(1.0, 65.0, 19) < zoom_for(1.0, 0.0, 19));
        // Tile maths: the map's centre, and a known tile (Paris at zoom 17).
        let (x, y) = map_px(0.0, 0.0, 1);
        assert!((x - 256.0).abs() < 1e-9 && (y - 256.0).abs() < 1e-9);
        let (x, y) = map_px(48.85, 2.35, 17);
        assert_eq!(((x / 256.0) as u32, (y / 256.0) as u32), (66391, 45096));
    }

    #[test]
    fn a_flat_tile_is_no_tile() {
        assert!(Tile::new(vec![200; 256 * 256 * 3]).is_none());
        assert!(Tile::new(vec![200; 100]).is_none());
        let mut rgb = vec![200; 256 * 256 * 3];
        rgb.iter_mut().step_by(7).for_each(|v| *v = 40);
        assert!(Tile::new(rgb).is_some());
    }

    /// A tile whose pixels encode where they are, so the result can be checked.
    fn gradient(x: u32, y: u32) -> Arc<Tile> {
        let mut rgb = Vec::with_capacity(256 * 256 * 3);
        for j in 0..256u32 {
            for i in 0..256u32 {
                rgb.extend([(i % 256) as u8, (j % 256) as u8, ((x + y) % 2 * 200) as u8]);
            }
        }
        Arc::new(Tile::new(rgb).unwrap())
    }

    #[test]
    fn builds_a_chunk_from_tiles_and_falls_back() {
        // Bern: Swiss imagery at zoom 17.
        let route = route_at(46.948, 7.447);
        let p = providers("");
        let mut asked = Vec::new();
        let img = chunk_image(&route, 0, 0, NEAR_PX, &p, |k, z, x, y| {
            asked.push((p[k].id.clone(), z));
            Some(gradient(x, y))
        })
        .unwrap();
        assert_eq!(img.rgba.len(), NEAR_PX * NEAR_PX * 4);
        assert_eq!(img.credits, vec!["SWISSIMAGE © swisstopo".to_string()]);
        assert!(asked.iter().all(|(id, z)| id == "swisstopo" && *z == 17));
        // A 240 m chunk spans one or two 200 m tiles each way: a handful of tiles, each fetched once.
        assert!((1..=9).contains(&asked.len()), "{}", asked.len());
        // Pixels step evenly across the tile: one chunk pixel is about 0.94 / 1.25 m ≈ 1.3 tile pixels in red.
        let red = |i: usize, j: usize| img.rgba[(j * NEAR_PX + i) * 4] as i32;
        let step = red(11, 5) - red(10, 5);
        assert!((1..=2).contains(&step), "{step}");
        assert_eq!(img.rgba[3], 255);

        // With no Swiss tiles, Sentinel fills in.
        let img = chunk_image(&route, 0, 0, FAR_PX, &p, |k, _, x, y| (p[k].id != "swisstopo").then(|| gradient(x, y))).unwrap();
        assert_eq!(img.credits.len(), 1);
        assert!(img.credits[0].starts_with("Sentinel-2"));
        // With nothing at all, no image: the ground keeps its own colours.
        assert!(chunk_image(&route, 0, 0, FAR_PX, &p, |_, _, _, _| None).is_none());
    }
}

#[cfg(test)]
mod far_tests {
    use super::*;
    use crate::gpx::{build_route, RawPoint};

    #[test]
    fn colours_the_distance_from_coarse_tiles() {
        let raw: Vec<RawPoint> = (0..20).map(|k| RawPoint { lat: 45.09 + k as f64 * 1e-4, lon: 6.07, ele: Some(1500.0) }).collect();
        let route = build_route("Alpe", &raw).unwrap();
        let p = providers("");
        let points: Vec<(f64, f64)> = (0..10).flat_map(|j| (0..10).map(move |i| (i as f64 * 150.0, j as f64 * 150.0))).collect();
        let mut zooms = Vec::new();
        let grey = Arc::new(Tile { rgb: (0..256 * 256 * 3).map(|i| if i % 2 == 0 { 100 } else { 140 }).collect() });
        let c = far_colours(&route, &points, 150.0, &p, |k, z, _, _| {
            zooms.push((p[k].id.clone(), z));
            Some(grey.clone())
        })
        .unwrap();
        assert_eq!(c.len(), 100);
        // French imagery, coarse: 150 m cells need zoom 10 at this latitude.
        assert!(zooms.iter().all(|(id, z)| id == "ign" && *z == 10), "{zooms:?}");
        assert!(zooms.len() <= 4);
        // sRGB 100–140 averaged, as linear.
        assert!(c.iter().all(|v| v[0] > 0.1 && v[0] < 0.3));
    }
}
