//! Imagery tiles for the ground: downloaded when a chunk first needs them, kept on disk
//! for later rides, and decoded into a small in-memory cache that neighbouring chunks
//! share. The choice of tiles and putting them together is `world::imagery` in the core.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use tokio::sync::Semaphore;
use velopopuli_core::gpx::Route;
use velopopuli_core::world::{chunk_image, far_colours, providers, tile_url, ChunkImage, Provider, Tile};

/// Decoded tiles kept in memory (192 KB each).
const MEMORY_TILES: usize = 64;
/// Downloads at once, to be kind to the servers.
const DOWNLOADS: usize = 6;
const TIMEOUT: Duration = Duration::from_secs(15);
/// How long "this server has no tile here" is believed before asking again.
const MISSING_FOR: Duration = Duration::from_secs(30 * 86_400);
/// The disk cache is trimmed, oldest tiles first, to `KEEP_BYTES` once it passes `MAX_BYTES`.
const MAX_BYTES: u64 = 2 << 30;
const KEEP_BYTES: u64 = 3 << 29;

type Key = (usize, u8, u32, u32);
/// Decoded tiles by key (`None` where a provider has none), and the order they came in.
type Memory = (HashMap<Key, Option<Arc<Tile>>>, VecDeque<Key>);

pub struct Imagery {
    http: reqwest::Client,
    dir: PathBuf,
    route: Arc<Route>,
    providers: Vec<Provider>,
    memory: Mutex<Memory>,
    downloads: Semaphore,
}

impl Imagery {
    /// `data_dir` is the app's data folder; `custom_url` the rider's own provider, if any.
    pub fn new(http: reqwest::Client, data_dir: &Path, route: Arc<Route>, custom_url: &str) -> Arc<Self> {
        let dir = data_dir.join("imagery");
        let trim = dir.clone();
        std::thread::spawn(move || trim_cache(&trim));
        Arc::new(Self { http, dir, route, providers: providers(custom_url), memory: Mutex::new((HashMap::new(), VecDeque::new())), downloads: Semaphore::new(DOWNLOADS) })
    }

    /// The ground image for one chunk, or `None` if no imagery could be had.
    pub async fn chunk(self: &Arc<Self>, cx: i32, cz: i32, size: usize) -> Option<ChunkImage> {
        self.assemble(move |me, fetch| chunk_image(&me.route, cx, cz, size, &me.providers, fetch)).await
    }

    /// Colours for the distant landscape's points, each standing for `cell` metres square.
    pub async fn far(self: &Arc<Self>, points: Vec<(f64, f64)>, cell: f64) -> Option<Vec<[f32; 3]>> {
        self.assemble(move |me, fetch| far_colours(&me.route, &points, cell, &me.providers, fetch)).await
    }

    /// Runs `make` over the tiles it asks for, in rounds: each finds which tiles it lacks, and
    /// they are all fetched at once before the next. The first round takes every tile as there,
    /// so it asks only for each point's own provider; later ones fill in where one was missing.
    async fn assemble<T: Send + 'static>(self: &Arc<Self>, make: impl Fn(&Self, &mut dyn FnMut(usize, u8, u32, u32) -> Option<Arc<Tile>>) -> Option<T> + Send + Clone + 'static) -> Option<T> {
        let mut have: HashMap<Key, Option<Arc<Tile>>> = HashMap::new();
        for round in 0..4 {
            let (me, known, make) = (self.clone(), have.clone(), make.clone());
            let (out, wanted) = tauri::async_runtime::spawn_blocking(move || {
                let mut wanted = Vec::new();
                let blank = Arc::new(Tile { rgb: vec![0; 256 * 256 * 3] });
                let out = make(&me, &mut |p, z, x, y| match known.get(&(p, z, x, y)) {
                    Some(t) => t.clone(),
                    None => {
                        wanted.push((p, z, x, y));
                        (round == 0).then(|| blank.clone())
                    }
                });
                (out, wanted)
            })
            .await
            .ok()?;
            if wanted.is_empty() {
                return out;
            }
            let got = futures::future::join_all(wanted.iter().map(|&k| self.tile(k))).await;
            have.extend(wanted.into_iter().zip(got));
        }
        None
    }

    async fn tile(&self, key: Key) -> Option<Arc<Tile>> {
        if let Some(t) = self.memory.lock().unwrap().0.get(&key) {
            return t.clone();
        }
        let t = self.load(key).await;
        // A failed download isn't remembered, so it is tried again later; a missing tile is.
        if let Some(t) = &t {
            self.remember(key, t.clone());
        }
        t.flatten()
    }

    fn remember(&self, key: Key, tile: Option<Arc<Tile>>) {
        let mut m = self.memory.lock().unwrap();
        if m.0.insert(key, tile).is_none() {
            m.1.push_back(key);
        }
        while m.1.len() > MEMORY_TILES {
            if let Some(old) = m.1.pop_front() {
                m.0.remove(&old);
            }
        }
    }

    /// `Some(None)`: the provider has no tile there. `None`: couldn't find out (offline, slow).
    async fn load(&self, (p, z, x, y): Key) -> Option<Option<Arc<Tile>>> {
        let provider = &self.providers[p];
        let path = self.dir.join(&provider.id).join(z.to_string()).join(x.to_string()).join(y.to_string());
        let missing = path.with_extension("none");
        if std::fs::metadata(&missing).and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().unwrap_or_default() < MISSING_FOR) {
            return Some(None);
        }
        // Reading and decoding are blocking work, kept off the async threads.
        let cached = path.clone();
        if let Ok(Some(t)) = tauri::async_runtime::spawn_blocking(move || std::fs::read(&cached).ok().and_then(|b| decode(&b))).await {
            return Some(Some(Arc::new(t)));
        }
        let _permit = self.downloads.acquire().await.ok()?;
        let reply = self.http.get(tile_url(provider, z, x, y)).timeout(TIMEOUT).send().await.ok()?;
        let status = reply.status();
        if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return None;
        }
        let bytes = if status.is_success() { reply.bytes().await.ok()? } else { Default::default() };
        tauri::async_runtime::spawn_blocking(move || {
            let _ = std::fs::create_dir_all(path.parent()?);
            match decode(&bytes) {
                Some(t) => {
                    let _ = std::fs::write(&path, &bytes);
                    Some(Some(Arc::new(t)))
                }
                None => {
                    let _ = std::fs::write(&missing, b"");
                    Some(None)
                }
            }
        })
        .await
        .ok()?
    }
}

/// A JPEG or PNG tile, as 256 × 256 RGB; `None` if it isn't one or is blank.
fn decode(bytes: &[u8]) -> Option<Tile> {
    if bytes.starts_with(&[0xff, 0xd8]) {
        use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};
        let mut d = zune_jpeg::JpegDecoder::new_with_options(bytes, DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB));
        let rgb = d.decode().ok()?;
        let info = d.info()?;
        return (info.width == 256 && info.height == 256).then(|| Tile::new(rgb)).flatten();
    }
    if bytes.starts_with(b"\x89PNG") {
        let mut d = png::Decoder::new(bytes);
        d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = d.read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut buf).ok()?;
        if frame.width != 256 || frame.height != 256 {
            return None;
        }
        let channels = frame.color_type.samples();
        let rgb = buf[..frame.buffer_size()].chunks_exact(channels).flat_map(|px| if channels >= 3 { [px[0], px[1], px[2]] } else { [px[0]; 3] }).collect();
        return Tile::new(rgb);
    }
    None
}

/// Keeps the disk cache under `MAX_BYTES` by removing the tiles least recently written.
fn trim_cache(dir: &Path) {
    fn walk(dir: &Path, out: &mut Vec<(SystemTime, u64, PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() {
                walk(&e.path(), out);
            } else {
                out.push((meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len(), e.path()));
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, &mut files);
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= MAX_BYTES {
        return;
    }
    files.sort_by_key(|f| f.0);
    for (_, len, path) in files {
        if total <= KEEP_BYTES {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_jpeg_and_png_tiles() {
        // A 256 × 256 PNG with a gradient, and one of a single colour.
        let png = |flat: bool| {
            let mut out = Vec::new();
            let mut e = png::Encoder::new(&mut out, 256, 256);
            e.set_color(png::ColorType::Rgb);
            let mut w = e.write_header().unwrap();
            let data: Vec<u8> = (0..256 * 256 * 3).map(|i| if flat { 90 } else { (i / 3 % 256) as u8 }).collect();
            w.write_image_data(&data).unwrap();
            drop(w);
            out
        };
        assert!(decode(&png(false)).is_some());
        assert!(decode(&png(true)).is_none());
        assert!(decode(b"<html>404</html>").is_none());
    }

    #[test]
    fn trims_the_oldest_tiles() {
        let dir = std::env::temp_dir().join(format!("vp-imagery-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/1/2")).unwrap();
        std::fs::write(dir.join("a/1/2/3"), b"x").unwrap();
        trim_cache(&dir);
        // Far under the limit: nothing goes.
        assert!(dir.join("a/1/2/3").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fetches a real chunk over the network: Bern, on Swiss imagery.
    #[tokio::test]
    #[ignore]
    async fn fetches_a_real_chunk() {
        let raw: Vec<_> = (0..20).map(|k| velopopuli_core::gpx::RawPoint { lat: 46.948 + k as f64 * 1e-4, lon: 7.447, ele: Some(540.0) }).collect();
        let route = Arc::new(velopopuli_core::gpx::build_route("Bern", &raw).unwrap());
        let http = reqwest::Client::builder().user_agent("VeloPopuli/0.3 (open-source indoor cycling app)").build().unwrap();
        let dir = std::env::temp_dir().join(format!("vp-imagery-net-{}", std::process::id()));
        let imagery = Imagery::new(http, &dir, route, "");
        let img = imagery.chunk(0, 0, velopopuli_core::world::NEAR_PX).await.expect("an image");
        assert_eq!(img.credits, vec!["SWISSIMAGE © swisstopo".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
