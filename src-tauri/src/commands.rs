//! The commands the UI can invoke. Each is a thin adapter onto the modules that do the work.

use std::path::{Path, PathBuf};

use base64::Engine;
use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use velopopuli_core::gamify::{Badge, BADGES};
use velopopuli_core::gpx::{build_route, demo_route, parse_gpx, Route};
use velopopuli_core::types::{RideSummary, RouteMeta, Settings, UploadRecord};
use velopopuli_core::world::GridMesh;

use crate::connectors::{self, ConnectorInfo, ConnectorStatus, Ctx, RideSearch, UploadRequest};
use crate::devices::DevicesSnapshot;
use crate::library::{self, GhostMeta, WorkoutMeta};
use crate::session::{load_route, local_day, RideInit, RideRequest, Session, DEMO_ROUTE_ID};
use crate::{geo, group, now_ms, photos, secrets, smart, AppState};
use velopopuli_core::fitness::{estimate_ftp, training_load, FtpEstimate, LoadDay, CURVE_DURATIONS};
use velopopuli_core::gamify::{streaks, Streaks};
use velopopuli_core::segments::{place_segment, Segment, SegmentSource};

type Res<T> = Result<T, String>;

fn blocking_err(e: tauri::Error) -> String {
    e.to_string()
}

// ── Storage ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn kv_get(state: State<AppState>, name: String) -> Option<Value> {
    state.store.get(&name)
}

#[tauri::command]
pub fn kv_set(state: State<AppState>, name: String, value: Value) -> Res<()> {
    if name == "settings" {
        crate::diag::set_detailed(value.get("diagnostics").and_then(Value::as_bool).unwrap_or(true));
    }
    state.store.set(&name, &value)
}

// ── Diagnostics ────────────────────────────────────────────────────────────

/// The saved settings, or the defaults if none have been saved yet.
fn settings_json(state: &AppState) -> Value {
    state.store.get("settings").unwrap_or_else(|| serde_json::to_value(Settings::default()).unwrap_or(Value::Null))
}

/// Asks where to save a diagnostics report, writes it there and shows it; returns its path.
#[tauri::command]
pub async fn diag_report(app: AppHandle, state: State<'_, AppState>, note: String) -> Res<Option<String>> {
    let settings = settings_json(&state);
    let devices = serde_json::to_value(state.devices.snapshot().devices).unwrap_or(Value::Null);
    let name = format!("velopopuli-diagnostics-{}.txt", chrono::Local::now().format("%Y%m%d-%H%M"));
    let dialog = app.dialog().file().set_title("Save diagnostics report").set_file_name(&name).add_filter("Text", &["txt"]);
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file()).await.map_err(blocking_err)?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    crate::diag::report(&path, &settings, &devices, &note)?;
    let _ = app.opener().reveal_item_in_dir(&path);
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Saves a report (asking where), then opens GitHub's new-issue page with the summary
/// filled in and shows the file, ready to drag onto the page. Returns the file's path.
#[tauri::command]
pub async fn diag_issue(app: AppHandle, state: State<'_, AppState>, note: String) -> Res<Option<String>> {
    let Some(path) = diag_report(app.clone(), state.clone(), note.clone()).await? else { return Ok(None) };
    let settings = settings_json(&state);
    let devices = serde_json::to_value(state.devices.snapshot().devices).unwrap_or(Value::Null);
    let report = crate::diag::build(&settings, &devices, &note, 64 * 1024)?;
    app.opener().open_url(crate::issues::issue_url(&note, &report.summary), None::<&str>).map_err(|e| e.to_string())?;
    Ok(Some(path))
}

#[tauri::command]
pub fn diag_open_folder(app: AppHandle) -> Res<()> {
    let dir = crate::diag::dir().ok_or("Diagnostics are not running.")?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn kv_del(state: State<AppState>, name: String) -> Res<()> {
    state.store.del(&name)
}

// ── Routes ─────────────────────────────────────────────────────────────────

fn meta_for(id: String, route: &Route, added_at: u64) -> RouteMeta {
    RouteMeta { id, name: route.name.clone(), distance: route.total, ascent: route.ascent, climbs: route.climbs.len(), added_at, preview: route.preview_outline(160) }
}

/// Validates a GPX file and adds it to the library.
fn import_gpx(state: &AppState, file_name: &str, text: &str) -> Res<RouteMeta> {
    let (name, points) = parse_gpx(text)?;
    let name = if name.is_empty() { file_name.to_string() } else { name };
    let route = build_route(&name, &points)?;
    let meta = meta_for(uuid::Uuid::new_v4().to_string(), &route, now_ms());
    state.store.set(&format!("route-{}", meta.id), &serde_json::json!({ "name": name, "gpx": text }))?;
    Ok(meta)
}

#[derive(Serialize)]
pub struct ImportResult {
    added: Vec<RouteMeta>,
    problems: Vec<String>,
}

#[tauri::command]
pub async fn routes_import_dialog(app: AppHandle, state: State<'_, AppState>) -> Res<ImportResult> {
    let picked =
        tauri::async_runtime::spawn_blocking(move || app.dialog().file().set_title("Import GPX routes").add_filter("GPX routes", &["gpx"]).blocking_pick_files()).await.map_err(blocking_err)?;
    let mut result = ImportResult { added: Vec::new(), problems: Vec::new() };
    for file in picked.unwrap_or_default() {
        let Ok(path) = file.into_path() else { continue };
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Route").to_string();
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| import_gpx(&state, &stem, &text)) {
            Ok(meta) => result.added.push(meta),
            Err(e) => result.problems.push(format!("{stem}: {e}")),
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn routes_import_text(state: State<AppState>, name: String, text: String) -> Res<RouteMeta> {
    import_gpx(&state, &name, &text)
}

#[tauri::command]
pub fn demo_meta() -> RouteMeta {
    meta_for(DEMO_ROUTE_ID.into(), &demo_route(), 0)
}

/// The built-in velodromes, each as its route ridden the usual way round (anticlockwise).
#[tauri::command]
pub fn tracks_meta() -> Vec<RouteMeta> {
    velopopuli_core::velodrome::TRACKS.iter().map(|t| meta_for(t.id.into(), &t.route(false), 0)).collect()
}

#[tauri::command]
pub fn badges() -> &'static [Badge] {
    &BADGES
}

// ── Files and links ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn choose_dir(app: AppHandle) -> Res<Option<String>> {
    let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_folder()).await.map_err(blocking_err)?;
    Ok(picked.and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn reveal_file(app: AppHandle, state: State<AppState>, path: String) -> Res<()> {
    if !state.store.owns_activity(Path::new(&path)) {
        return Err("Not a VeloPopuli activity file.".into());
    }
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Res<()> {
    if !url.starts_with("https://") {
        return Err("Only https links can be opened.".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

// ── Devices ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn devices_list(state: State<AppState>) -> DevicesSnapshot {
    state.devices.snapshot()
}

#[tauri::command]
pub async fn ble_scan_start(state: State<'_, AppState>) -> Res<()> {
    state.ble.scan_start(state.devices.clone()).await
}

#[tauri::command]
pub fn ble_scan_stop(state: State<AppState>) {
    state.ble.scan_stop();
}

#[tauri::command]
pub async fn ble_connect(state: State<'_, AppState>, id: String) -> Res<()> {
    state.ble.scan_stop();
    state.ble.connect(state.devices.clone(), &id).await
}

#[tauri::command]
pub fn ant_start(state: State<AppState>) {
    state.ant.start(state.devices.clone());
}

#[tauri::command]
pub fn ant_stop(state: State<AppState>) {
    state.ant.stop();
}

#[tauri::command]
pub fn demo_add(state: State<AppState>) {
    state.demo.add(&state.devices);
}

#[tauri::command]
pub fn demo_nudge(state: State<AppState>, delta: f64) {
    state.demo.nudge(delta);
}

#[tauri::command]
pub fn device_remove(state: State<AppState>, id: String) {
    state.devices.remove(&id);
}

// ── Ride ───────────────────────────────────────────────────────────────────

fn f32_bytes(header: &[u32], parts: &[&[f32]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(header.len() * 4 + parts.iter().map(|p| p.len() * 4).sum::<usize>());
    header.iter().for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
    parts.iter().flat_map(|p| p.iter()).for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
    out
}

/// Layout: `[ground vertices, trees, building vertices, water vertices, road vertices]` as u32,
/// then as f32: ground positions, normals and colours; trees (`TREE_FLOATS` each); building positions, normals, colours
/// and UVs; water positions and colours; road positions, normals, colours and UVs.
fn grid_response(mesh: GridMesh) -> Response {
    let (b, r) = (&mesh.buildings, &mesh.roads);
    Response::new(f32_bytes(
        &[(mesh.positions.len() / 3) as u32, (mesh.trees.len() / velopopuli_core::world::TREE_FLOATS) as u32, b.vertices() as u32, (mesh.water.len() / 3) as u32, r.vertices() as u32],
        &[&mesh.positions, &mesh.normals, &mesh.colors, &mesh.trees, &b.positions, &b.normals, &b.colors, &b.uvs, &mesh.water, &mesh.water_colors, &r.positions, &r.normals, &r.colors, &r.uvs],
    ))
}

#[tauri::command]
pub async fn ride_start(app: AppHandle, state: State<'_, AppState>, request: RideRequest) -> Res<RideInit> {
    if let Some(old) = state.session.lock().unwrap().take() {
        old.halt();
    }
    let (session, init) = Session::start(&app, &state.store, &state.http, state.devices.clone(), state.demo.clone(), &request).await?;
    *state.session.lock().unwrap() = Some(session);
    Ok(init)
}

/// The route's per-sample arrays as seven consecutive f64 blocks: x, z, lat, lon, elevation,
/// gradient, and the width of the road.
#[tauri::command]
pub fn ride_route_data(state: State<AppState>) -> Res<Response> {
    let session = state.current_session()?;
    let r = &session.world.route;
    let mut out = Vec::with_capacity(r.n * 72);
    for block in [&r.x, &r.z, &r.lat, &r.lon] {
        block.iter().for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
    }
    for block in [&r.ele, &r.grade] {
        block.iter().for_each(|v| out.extend_from_slice(&(*v as f64).to_le_bytes()));
    }
    (0..r.n).for_each(|i| out.extend_from_slice(&(session.world.half_width(i) * 2.0).to_le_bytes()));
    (0..r.n).for_each(|i| out.extend_from_slice(&session.world.bank(i).to_le_bytes()));
    (0..r.n).for_each(|i| out.extend_from_slice(&session.world.dip(i).to_le_bytes()));
    Ok(Response::new(out))
}

/// Road ribbon: positions (3 per vertex), UVs (2 per vertex), then how it looks per sample
/// (`Surface as u8`, or 5 for tarmac without a centre line), as f32;
/// `layout.roadSection` vertices per sample.
#[tauri::command]
pub async fn ride_road_mesh(state: State<'_, AppState>) -> Res<Response> {
    let world = state.current_session()?.world.clone();
    let mesh = tauri::async_runtime::spawn_blocking(move || world.road()).await.map_err(blocking_err)?;
    let looks: Vec<f32> = mesh.looks.iter().map(|&s| s as f32).collect();
    Ok(Response::new(f32_bytes(&[], &[&mesh.positions, &mesh.uvs, &looks])))
}

/// The tree models, one per kind. Layout: `[kinds, then the vertex count of each]` as u32,
/// then as f32: each kind's sprite frame (`[half width, bottom, top]`), then per kind its
/// positions, normals and colours (3 each per vertex).
#[tauri::command]
pub fn tree_models() -> Response {
    use velopopuli_core::world::{sprite_frame, tree_model, TREE_KINDS};
    let models: Vec<_> = (0..TREE_KINDS as u8).map(tree_model).collect();
    let frames: Vec<f32> = (0..TREE_KINDS as u8).flat_map(|k| sprite_frame(k).map(|v| v as f32)).collect();
    let mut header = vec![models.len() as u32];
    header.extend(models.iter().map(|m| m.vertices() as u32));
    let mut parts: Vec<&[f32]> = vec![&frames];
    parts.extend(models.iter().flat_map(|m| [&m.positions[..], &m.normals[..], &m.colors[..]]));
    Response::new(f32_bytes(&header, &parts))
}

/// Race barriers. Layout: `[sections, looks, then the vertex count of each look]` as u32, then as f32:
/// the sections (`[x, y, z, yaw, distance, look, pitch]` each), then per look its positions, normals,
/// colours (3 each per vertex) and UVs (2).
#[tauri::command]
pub async fn ride_barriers(state: State<'_, AppState>) -> Res<Response> {
    let world = state.current_session()?.world.clone();
    let (sections, models) = tauri::async_runtime::spawn_blocking(move || {
        let models: Vec<_> = (0..velopopuli_core::world::BARRIER_LOOKS).map(velopopuli_core::world::barrier_model).collect();
        (world.barriers(), models)
    })
    .await
    .map_err(blocking_err)?;
    let mut header = vec![sections.len() as u32, models.len() as u32];
    header.extend(models.iter().map(|m| m.vertices() as u32));
    let flat: Vec<f32> = sections.iter().flatten().copied().collect();
    let mut parts: Vec<&[f32]> = vec![&flat];
    for m in &models {
        parts.extend([&m.positions[..], &m.normals[..], &m.colors[..], &m.uvs[..]]);
    }
    Ok(Response::new(f32_bytes(&header, &parts)))
}

/// Bridges, tunnels and whatever crosses overhead. Layout: `[sunlit vertices, unlit vertices]`
/// as u32, then as f32: sunlit positions, normals and colours; unlit positions and colours.
#[tauri::command]
pub async fn ride_structures(state: State<'_, AppState>) -> Res<Response> {
    let world = state.current_session()?.world.clone();
    let s = tauri::async_runtime::spawn_blocking(move || world.structures()).await.map_err(blocking_err)?;
    Ok(Response::new(f32_bytes(&[s.lit.vertices() as u32, s.unlit.vertices() as u32], &[&s.lit.positions, &s.lit.normals, &s.lit.colors, &s.unlit.positions, &s.unlit.colors])))
}

#[tauri::command]
pub async fn terrain_chunk(state: State<'_, AppState>, cx: i32, cz: i32) -> Res<Response> {
    let world = state.current_session()?.world.clone();
    tauri::async_runtime::spawn_blocking(move || grid_response(world.chunk(cx, cz))).await.map_err(blocking_err)
}

#[tauri::command]
pub async fn terrain_far(state: State<'_, AppState>, cx: f64, cz: f64) -> Res<Response> {
    let world = state.current_session()?.world.clone();
    tauri::async_runtime::spawn_blocking(move || grid_response(world.far(cx, cz))).await.map_err(blocking_err)
}

#[tauri::command]
pub fn ride_toggle_pause(state: State<AppState>) -> Res<()> {
    state.current_session()?.toggle_pause();
    Ok(())
}

#[tauri::command]
pub fn ride_intensity(state: State<AppState>, delta: f64) -> Res<()> {
    state.current_session()?.nudge_intensity(delta);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GearPreview {
    presets: Vec<(String, String)>,
    /// `(label, ratio)` per gear, easiest first; empty with real gears.
    gears: Vec<(String, f64)>,
    /// The gear a ride starts in (from 0), and the ratio of the real gear.
    start: usize,
    real_ratio: f64,
}

/// What a gear setup amounts to, for the Settings page.
#[tauri::command]
pub fn gears_preview(gears: velopopuli_core::types::GearSettings) -> GearPreview {
    use velopopuli_core::gearing;
    let mut on = gears.clone();
    on.mode = velopopuli_core::types::GearsMode::Virtual;
    let list = gearing::gears(&on);
    GearPreview {
        presets: gearing::PRESETS.iter().map(|p| (p.id.to_string(), p.name.to_string())).collect(),
        start: gearing::start_gear(&on, &list),
        real_ratio: gearing::real_ratio(&on),
        gears: list.into_iter().map(|g| (g.label, g.ratio)).collect(),
    }
}

/// Shifts the virtual gear: +1 harder, −1 easier.
#[tauri::command]
pub fn ride_shift(state: State<AppState>, delta: i32) -> Res<()> {
    state.current_session()?.shift(delta);
    Ok(())
}

#[tauri::command]
pub fn ride_skip_step(state: State<AppState>) -> Res<()> {
    state.current_session()?.skip_step();
    Ok(())
}

#[tauri::command]
pub fn ride_update_settings(state: State<AppState>, settings: Settings) {
    if let Ok(session) = state.current_session() {
        session.update_settings(settings);
    }
}

/// Ends the ride and saves it. Resolves to `null` if it was too short to keep.
#[tauri::command]
pub async fn ride_finish(state: State<'_, AppState>) -> Res<Option<RideSummary>> {
    if let Some(n) = state.native.lock().unwrap().take() {
        n.stop();
    }
    let Some(session) = state.session.lock().unwrap().take() else { return Ok(None) };
    session.finish(&state.store)
}

#[tauri::command]
pub fn ride_abort(state: State<AppState>) {
    if let Some(n) = state.native.lock().unwrap().take() {
        n.stop();
    }
    if let Some(session) = state.session.lock().unwrap().take() {
        session.halt();
    }
}

// ── Native renderer ────────────────────────────────────────────────────────

#[tauri::command]
pub fn native_available() -> bool {
    cfg!(target_os = "macos")
}

/// Starts drawing the ride natively. The body is the page's drawing data (see `native::parse_assets`).
#[tauri::command]
pub async fn native_start(app: AppHandle, state: State<'_, AppState>, request: tauri::ipc::Request<'_>) -> Res<()> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err("Expected the drawing data as bytes.".into()) };
    let session = state.current_session()?;
    if let Some(old) = state.native.lock().unwrap().take() {
        old.stop();
    }
    let (world, engine, bytes) = (session.world.clone(), session.engine(), bytes.clone());
    let made = tauri::async_runtime::spawn_blocking(move || crate::native::Native::start(&app, world, engine, &bytes)).await.map_err(blocking_err)??;
    *state.native.lock().unwrap() = Some(made);
    Ok(())
}

#[tauri::command]
pub async fn native_stop(state: State<'_, AppState>) -> Res<()> {
    let native = state.native.lock().unwrap().take();
    if let Some(n) = native {
        tauri::async_runtime::spawn_blocking(move || n.stop()).await.map_err(blocking_err)?;
    }
    Ok(())
}

#[tauri::command]
pub fn native_camera(state: State<AppState>, index: Option<usize>) {
    if let Some(n) = state.native.lock().unwrap().as_ref() {
        match index {
            Some(i) => n.set_camera(i),
            None => n.cycle_camera(),
        }
    }
}

#[tauri::command]
pub fn native_look(state: State<AppState>, look: crate::native::Look) {
    if let Some(n) = state.native.lock().unwrap().as_ref() {
        n.set_look(look);
    }
}

/// For the automated check: saves the next native frame next to its report; returns frames drawn so far.
#[tauri::command]
pub fn native_capture(state: State<AppState>) -> Res<(u64, Option<String>)> {
    let path = std::env::var_os("VP_SMOKE").map(PathBuf::from).ok_or("Only during the automated check.")?;
    let native = state.native.lock().unwrap();
    let n = native.as_ref().ok_or("The native renderer is not running.")?;
    n.capture(path.with_extension("native.png"));
    Ok(n.frames())
}

/// For the automated check: whether the native picture asked for is still being written.
#[tauri::command]
pub fn native_saving(state: State<AppState>) -> bool {
    state.native.lock().unwrap().as_ref().is_some_and(|n| n.saving())
}

// ── Connectors (Strava, …) ─────────────────────────────────────────────────

impl AppState {
    fn connector_ctx(&self) -> Ctx {
        Ctx::new(self.store.dir(), &self.http)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorEntry {
    info: ConnectorInfo,
    status: ConnectorStatus,
    /// The ride setup can offer the rider's past rides on this service as ghosts.
    lists_rides: bool,
    /// The ride setup can ask this service for segments along the route.
    finds_segments: bool,
    /// Find a route can list the rider's saved routes on this service.
    lists_routes: bool,
}

/// Every connector with its description and current state, for the settings and upload UI.
#[tauri::command]
pub fn connectors_list(state: State<AppState>) -> Vec<ConnectorEntry> {
    let ctx = state.connector_ctx();
    connectors::ALL
        .iter()
        .map(|c| ConnectorEntry { info: c.info(), status: c.status(&ctx), lists_rides: c.lists_rides(), finds_segments: c.finds_segments(), lists_routes: c.lists_routes() })
        .collect()
}

/// The rider's rides on a connector, each marked whether it follows the route, for racing one
/// as a ghost. Pages back through older rides until a few follow the route.
#[tauri::command]
pub async fn connector_rides(state: State<'_, AppState>, id: String, route_id: String) -> Res<RideSearch> {
    let route = load_route(&state.store, &route_id)?;
    let ctx = state.connector_ctx();
    connectors::find_rides(connectors::get(&id)?, &ctx, &route).await
}

/// The rider's saved routes on a connector, for adding one to the library.
#[tauri::command]
pub async fn connector_routes(state: State<'_, AppState>, id: String) -> Res<connectors::RouteList> {
    connectors::get(&id)?.saved_routes(&state.connector_ctx()).await
}

/// One of the rider's saved routes on a connector, ready to preview and save like a planned one.
#[tauri::command]
pub async fn connector_route(state: State<'_, AppState>, id: String, route_id: String) -> Res<Planned> {
    let gpx = connectors::get(&id)?.route_gpx(&state.connector_ctx(), &route_id).await?;
    let (name, raw) = parse_gpx(&gpx)?;
    let route = build_route(&name, &raw)?;
    Ok(Planned::new(gpx, &route, Some(name).filter(|n| !n.trim().is_empty()), 0, false))
}

#[tauri::command]
pub fn connector_status(state: State<AppState>, id: String) -> Res<ConnectorStatus> {
    Ok(connectors::get(&id)?.status(&state.connector_ctx()))
}

#[tauri::command]
pub fn connector_configure(state: State<AppState>, id: String, fields: std::collections::BTreeMap<String, String>) -> Res<ConnectorStatus> {
    connectors::get(&id)?.configure(&state.connector_ctx(), &fields)
}

#[tauri::command]
pub async fn connector_connect(app: AppHandle, state: State<'_, AppState>, id: String) -> Res<ConnectorStatus> {
    let ctx = state.connector_ctx();
    connectors::get(&id)?
        .connect(
            &ctx,
            Box::new(move |url| {
                let _ = app.opener().open_url(url, None::<&str>);
            }),
        )
        .await
}

#[tauri::command]
pub async fn connector_disconnect(state: State<'_, AppState>, id: String) -> Res<ConnectorStatus> {
    let ctx = state.connector_ctx();
    connectors::get(&id)?.disconnect(&ctx).await
}

/// Sends a ride's FIT file to a connector and remembers the result against the ride,
/// so the ride isn't uploaded there twice.
#[tauri::command]
pub async fn connector_upload(state: State<'_, AppState>, id: String, ride_id: String, file_path: String, name: String, description: String) -> Res<UploadRecord> {
    // Only files this app wrote are eligible — the UI can't use this to send arbitrary paths.
    let file = PathBuf::from(&file_path);
    if !state.store.owns_activity(&file) {
        return Err("Not a VeloPopuli activity file.".into());
    }
    let ctx = state.connector_ctx();
    let record = connectors::get(&id)?.upload(&ctx, &UploadRequest { file, name, description }).await?;
    // The upload above can take a while and runs for several connectors at once; only the
    // read-change-write of the ride list is serialised.
    let _guard = state.rides_lock.lock().unwrap();
    let mut rides: Vec<RideSummary> = state.store.get("rides").unwrap_or_default();
    rides.iter_mut().for_each(RideSummary::migrate_uploads);
    if let Some(ride) = rides.iter_mut().find(|r| r.id == ride_id) {
        ride.uploads.insert(id, record.clone());
    }
    state.store.set("rides", &rides)?;
    Ok(record)
}

/// Whether the route is a closed loop that can be ridden in laps. Builds the route rather than reading the
/// stored list, so routes imported before laps existed work too.
#[tauri::command]
pub async fn route_is_loop(state: State<'_, AppState>, route_id: String) -> Res<bool> {
    Ok(load_route(&state.store, &route_id)?.closed)
}

/// Asks a connector for segments along the route, keeps the ones the route rides from start
/// to finish, and stores them with the route next to other connectors' segments.
#[tauri::command]
pub async fn segments_fetch(state: State<'_, AppState>, id: String, route_id: String) -> Res<Vec<Segment>> {
    let route = load_route(&state.store, &route_id)?;
    // Services answer per area with a handful of segments, so ask about the route in pieces.
    let mut boxes = Vec::new();
    let mut d = 0.0;
    while d < route.total {
        let (a, b) = ((d / route.step) as usize, (((d + 3000.0) / route.step) as usize).min(route.n - 1));
        let (lat, lon) = (&route.lat[a..=b], &route.lon[a..=b]);
        let fold = |v: &[f64], f: fn(f64, f64) -> f64, init: f64| v.iter().copied().fold(init, f);
        boxes.push([fold(lat, f64::min, 90.0) - 0.002, fold(lon, f64::min, 180.0) - 0.002, fold(lat, f64::max, -90.0) + 0.002, fold(lon, f64::max, -180.0) + 0.002]);
        d += 3000.0;
    }
    boxes.truncate(40);
    let ctx = state.connector_ctx();
    let found = connectors::get(&id)?.segments_near(&ctx, &boxes).await?;
    let mut segments: Vec<Segment> = library::route_segments(&state.store, &route_id).into_iter().filter(|s| s.connector_id() != Some(id.as_str())).collect();
    segments.extend(found.iter().filter_map(|f| {
        let (start, end) = place_segment(&route, &f.points, f.distance)?;
        Some(Segment { id: format!("{id}-{}", f.id), name: f.name.clone(), source: SegmentSource::Remote, connector: Some(id.clone()), start, end, avg_grade: f.avg_grade, record: f.record.clone() })
    }));
    segments.sort_by(|a, b| a.start.total_cmp(&b.start));
    state.store.set(&format!("segments-{route_id}"), &segments)?;
    Ok(segments)
}

#[tauri::command]
pub fn segments_list(state: State<AppState>, route_id: String) -> Vec<Segment> {
    library::route_segments(&state.store, &route_id)
}

// ── Workouts ───────────────────────────────────────────────────────────────

#[tauri::command]
pub fn workouts_list(state: State<AppState>) -> Vec<WorkoutMeta> {
    library::list_workouts(&state.store)
}

#[tauri::command]
pub async fn workouts_import_dialog(app: AppHandle, state: State<'_, AppState>) -> Res<ImportResult2> {
    let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().set_title("Import workouts").add_filter("Workouts", &["zwo", "erg", "mrc"]).blocking_pick_files())
        .await
        .map_err(blocking_err)?;
    let ftp = state.store.get::<Settings>("settings").unwrap_or_default().rider.ftp;
    let mut result = ImportResult2 { added: Vec::new(), problems: Vec::new() };
    for file in picked.unwrap_or_default() {
        let Ok(path) = file.into_path() else { continue };
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("workout").to_string();
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| library::import_workout(&state.store, &name, &text, ftp)) {
            Ok(n) => result.added.push(n),
            Err(e) => result.problems.push(format!("{name}: {e}")),
        }
    }
    Ok(result)
}

#[derive(Serialize)]
pub struct ImportResult2 {
    added: Vec<String>,
    problems: Vec<String>,
}

#[tauri::command]
pub fn workout_import_text(state: State<AppState>, name: String, text: String) -> Res<String> {
    let ftp = state.store.get::<Settings>("settings").unwrap_or_default().rider.ftp;
    library::import_workout(&state.store, &name, &text, ftp)
}

#[tauri::command]
pub fn workout_delete(state: State<AppState>, id: String) -> Res<()> {
    library::delete_workout(&state.store, &id)
}

// ── Ghosts ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn ghosts_list(state: State<AppState>, route_id: String) -> Vec<GhostMeta> {
    library::list_ghosts(&state.store, &route_id)
}

#[tauri::command]
pub async fn ghost_import_file(app: AppHandle, state: State<'_, AppState>, route_id: String) -> Res<Option<GhostMeta>> {
    let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().set_title("Choose a recorded ride to race").add_filter("Recorded rides", &["gpx", "fit"]).blocking_pick_file())
        .await
        .map_err(blocking_err)?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Ghost").to_string();
    let track = tauri::async_runtime::spawn_blocking(move || library::read_track(&path)).await.map_err(blocking_err)??;
    let route = load_route(&state.store, &route_id)?;
    library::add_ghost(&state.store, &route_id, &route, &name, &track).map(Some)
}

/// Fetches one of the rider's rides from a connector and keeps it as a ghost for the route.
#[tauri::command]
pub async fn ghost_import_connector(state: State<'_, AppState>, id: String, route_id: String, ride_id: String) -> Res<GhostMeta> {
    let ctx = state.connector_ctx();
    let track = connectors::get(&id)?.ride_track(&ctx, &ride_id).await?;
    let route = load_route(&state.store, &route_id)?;
    library::add_ghost(&state.store, &route_id, &route, &track.name, &track.points)
}

#[tauri::command]
pub fn ghost_delete(state: State<AppState>, route_id: String, id: String) -> Res<()> {
    library::delete_ghost(&state.store, &route_id, &id)
}

// ── Fitness and goals ──────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalStatus {
    name: String,
    done_km: f64,
    target_km: f64,
    from: String,
    to: String,
    days_left: i64,
    /// Kilometres a week still needed to make it.
    per_week_km: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekTotal {
    /// First day (Monday) of the week, days since the epoch.
    day: i64,
    km: f64,
    tss: f64,
    hours: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitnessOverview {
    durations: Vec<u32>,
    curve_all: Vec<f64>,
    curve_recent: Vec<f64>,
    ftp_estimate: Option<FtpEstimate>,
    load: Vec<LoadDay>,
    streaks: Streaks,
    goal: GoalStatus,
    weeks: Vec<WeekTotal>,
}

#[tauri::command]
pub fn fitness_overview(state: State<AppState>) -> FitnessOverview {
    let rides: Vec<RideSummary> = state.store.get("rides").unwrap_or_default();
    let settings: Settings = state.store.get("settings").unwrap_or_default();
    let today = local_day(now_ms());
    let best = |filter: &dyn Fn(&RideSummary) -> bool| -> Vec<f64> {
        (0..CURVE_DURATIONS.len()).map(|i| rides.iter().filter(|r| filter(r)).filter_map(|r| r.curve.get(i)).copied().fold(0.0, f64::max)).collect()
    };
    let curve_all = best(&|_| true);
    let curve_recent = best(&|r| today - local_day(r.started_at) <= 42);
    let ftp_estimate = estimate_ftp(&curve_recent, false);
    let tss: Vec<(i64, f64)> = rides.iter().map(|r| (local_day(r.started_at), r.tss)).collect();
    let mut load = training_load(&tss, today);
    if load.len() > 180 {
        load.drain(..load.len() - 180);
    }
    let days: Vec<i64> = rides.iter().map(|r| local_day(r.started_at)).collect();
    let (from, to) = library::goal_window(&settings.goal);
    let progress = library::goal_progress(&settings.goal, &rides, 0.0);
    let today_date = chrono::Local::now().date_naive();
    let days_left = (to - today_date).num_days().max(0);
    let left_km = (settings.goal.distance_km * (1.0 - progress)).max(0.0);
    let monday = today - (today + 3).rem_euclid(7);
    let weeks = (0..12)
        .rev()
        .map(|w| {
            let start = monday - 7 * w;
            let these = rides.iter().filter(|r| (start..start + 7).contains(&local_day(r.started_at)));
            let (mut km, mut t, mut h) = (0.0, 0.0, 0.0);
            for r in these {
                km += r.distance_m / 1000.0;
                t += r.tss;
                h += r.duration_s / 3600.0;
            }
            WeekTotal { day: start, km, tss: t, hours: h }
        })
        .collect();
    FitnessOverview {
        durations: CURVE_DURATIONS.to_vec(),
        curve_all,
        curve_recent,
        ftp_estimate,
        load,
        streaks: streaks(&days, today),
        goal: GoalStatus {
            name: settings.goal.name.clone(),
            done_km: progress * settings.goal.distance_km,
            target_km: settings.goal.distance_km,
            from: from.to_string(),
            to: to.to_string(),
            days_left,
            per_week_km: if days_left > 0 { left_km / (days_left as f64 / 7.0).max(1.0) } else { 0.0 },
        },
        weeks,
    }
}

// ── Group rides ────────────────────────────────────────────────────────────

/// Downloads the route a group-ride room is riding, reusing it if it is already in the library.
#[tauri::command]
pub async fn group_fetch_route(state: State<'_, AppState>, room: String) -> Res<RouteMeta> {
    let url = state.store.get::<Settings>("settings").unwrap_or_default().group.relay_url;
    let (name, gpx) = group::peek(&url, room.trim()).await?;
    let key = group::route_key(&gpx);
    // Reuse the route if it's already in the library.
    let routes: Vec<RouteMeta> = state.store.get("routes").unwrap_or_default();
    for r in &routes {
        #[derive(serde::Deserialize)]
        struct Stored {
            gpx: String,
        }
        if state.store.get::<Stored>(&format!("route-{}", r.id)).is_some_and(|s| group::route_key(&s.gpx) == key) {
            return Ok(r.clone());
        }
    }
    // The routes page adds it to the list it keeps.
    import_gpx(&state, &name, &gpx)
}

// ── Route planning ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn geo_search(state: State<'_, AppState>, query: String) -> Res<Vec<geo::Place>> {
    geo::search(&state.http, query.trim()).await
}

/// A planned route, ready to preview: its GPX and the numbers the library would show.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Planned {
    gpx: String,
    distance: f64,
    ascent: f64,
    climbs: usize,
    /// `[lat, lon]` outline and `[distance, elevation]` profile, thinned for drawing.
    line: Vec<[f64; 2]>,
    profile: Vec<[f64; 2]>,
    /// The route's own name, for a waymarked route.
    name: Option<String>,
    /// Places where a waymarked route's mapped ways don't meet.
    gaps: usize,
    /// Only the part of a waymarked route in the area searched.
    clipped: bool,
}

impl Planned {
    fn new(gpx: String, route: &Route, name: Option<String>, gaps: usize, clipped: bool) -> Self {
        let stride = (route.n / 600).max(1);
        Planned {
            distance: route.total,
            ascent: route.ascent,
            climbs: route.climbs.len(),
            line: route.preview_outline(1500),
            profile: (0..route.n).step_by(stride).map(|i| [i as f64 * route.step, route.ele[i] as f64]).collect(),
            gpx,
            name,
            gaps,
            clipped,
        }
    }
}

#[tauri::command]
pub async fn geo_plan(state: State<'_, AppState>, points: Vec<[f64; 2]>, profile: String) -> Res<Planned> {
    let gpx = geo::plan(&state.http, &points, &profile).await?;
    let route = build_route("Planned", &parse_gpx(&gpx)?.1)?;
    Ok(Planned::new(gpx, &route, None, 0, false))
}

/// The waymarked routes of a kind (mountain-bike or road) within `[south, west, north, east]`.
#[tauri::command]
pub async fn geo_waymarked_routes(state: State<'_, AppState>, kind: velopopuli_core::waymarked::Kind, bounds: [f64; 4]) -> Res<Vec<velopopuli_core::waymarked::WaymarkedSummary>> {
    geo::waymarked_routes(&state.http, kind, bounds).await
}

/// One waymarked route, ready to preview and save, with elevation from the terrain. `bounds`
/// is the area it was found in: a road route is cut down to its part in there.
#[tauri::command]
pub async fn geo_waymarked_route(state: State<'_, AppState>, kind: velopopuli_core::waymarked::Kind, id: u64, bounds: [f64; 4]) -> Res<Planned> {
    let way = geo::waymarked_route(&state.http, kind, id, bounds).await?;
    let raw: Vec<velopopuli_core::gpx::RawPoint> = way.line.iter().map(|p| velopopuli_core::gpx::RawPoint { lat: p[0], lon: p[1], ele: None }).collect();
    let mut route = build_route(&way.name, &raw)?;
    // The relation has no heights; the terrain tiles do.
    let dem = crate::terrain::load_dem(&state.http, &route).await;
    if let Some(dem) = &dem {
        velopopuli_core::world::align_dem(&mut route, dem);
    }
    // Every 20 m or so is plenty for a ride; elevations only when the terrain gave them.
    let stride = ((20.0 / route.step).round() as usize).max(1);
    let points: Vec<([f64; 2], Option<f64>)> = (0..route.n).step_by(stride).chain([route.n - 1]).map(|i| ([route.lat[i], route.lon[i]], dem.as_ref().map(|_| route.ele[i] as f64))).collect();
    let gpx = velopopuli_core::waymarked::to_gpx(&way.name, &points);
    Ok(Planned::new(gpx, &route, Some(way.name), way.gaps, way.clipped))
}

/// Street-level photos along the route being ridden.
#[tauri::command]
pub async fn ride_photos(state: State<'_, AppState>) -> Res<Vec<velopopuli_core::segments::Photo>> {
    let session = state.current_session()?;
    let token = state.store.get::<Settings>("settings").unwrap_or_default().look.mapillary_token;
    photos::along(&state.http, &state.store, &session.route_id, &session.world.route, &token).await
}

#[tauri::command]
pub async fn photo_image(state: State<'_, AppState>, url: String) -> Res<String> {
    photos::image(&state.http, &url).await
}

/// The WMO weather code at a place right now, from Open-Meteo.
#[tauri::command]
pub async fn weather_now(state: State<'_, AppState>, lat: f64, lon: f64) -> Res<u32> {
    let url = format!("https://api.open-meteo.com/v1/forecast?latitude={lat:.4}&longitude={lon:.4}&current=weather_code");
    let res = state.http.get(url).timeout(std::time::Duration::from_secs(8)).send().await.map_err(|e| format!("Could not reach the weather service: {e}"))?;
    let json: Value = res.json().await.map_err(|e| e.to_string())?;
    json["current"]["weather_code"].as_u64().map(|c| c as u32).ok_or_else(|| "The weather service gave no reading.".into())
}

#[tauri::command]
pub fn geo_classics() -> &'static [geo::Classic] {
    geo::CLASSICS
}

// ── Fan and lights ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn smart_set_token(state: State<AppState>, token: String) -> Res<()> {
    secrets::save_named(state.store.dir(), "homeassistant", token.trim())
}

/// Turns the fan on full for a few seconds, then off, to check the set-up.
#[tauri::command]
pub async fn smart_test(state: State<'_, AppState>) -> Res<()> {
    let cfg = state.store.get::<Settings>("settings").unwrap_or_default().smart;
    let link = smart::SmartLink::start(state.store.dir(), cfg.clone()).ok_or("Choose a fan or lights connection and enter its address first.")?;
    let full = cfg.fan_full.max(cfg.fan_from + 1.0);
    let (speed, power, hr) = match cfg.fan_by.as_str() {
        "power" => (0.0, full, None),
        "hr" => (0.0, 250.0, Some(full)),
        _ => (full, 250.0, None),
    };
    link.update(speed, power, hr, 200.0);
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    link.stop();
    Ok(())
}

// ── Diagnostics ────────────────────────────────────────────────────────────

/// Set only by the automated check: which screen to open and how long to run before reporting.
#[derive(Serialize)]
pub struct SmokeConfig {
    screen: String,
    ms: u64,
    /// Which camera view to use on the ride screen.
    camera: u32,
    workout: Option<String>,
    room: Option<String>,
    ghosts: Option<Vec<String>>,
    /// Seconds of window frames to save before the end, for demo clips (VP_SMOKE_CLIP).
    clip: Option<f64>,
    /// Virtual gears to shift during the ride (VP_SMOKE_SHIFT, negative = easier).
    shift: Option<i32>,
    /// Scroll to the heading with this text before capturing (VP_SMOKE_SCROLL).
    scroll: Option<String>,
    /// Laps to ride on a closed route (VP_SMOKE_LAPS, 0 = until stopped).
    laps: Option<u32>,
    /// Ride this route instead of the newest in the library (VP_SMOKE_ROUTE, e.g. `herne-hill`).
    route: Option<String>,
}

/// Automated check only: saves window frame `index` as `<name>.frames/NNNN.png`, for demo clips.
#[tauri::command]
pub fn smoke_frame(app: AppHandle, index: u32) {
    let Some(path) = std::env::var_os("VP_SMOKE").map(PathBuf::from) else { return };
    let Some(window) = app.get_webview_window("main") else { return };
    let dir = path.with_extension("frames");
    let _ = std::fs::create_dir_all(&dir);
    crate::snapshot::window_png(&window, dir.join(format!("{index:04}.png")), |r| {
        if let Err(e) = r {
            eprintln!("clip frame: {e}");
        }
    });
}

#[tauri::command]
pub fn smoke_config() -> Option<SmokeConfig> {
    std::env::var_os("VP_SMOKE")?;
    Some(SmokeConfig {
        screen: std::env::var("VP_SMOKE_SCREEN").unwrap_or_else(|_| "ride".into()),
        ms: std::env::var("VP_SMOKE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(9000),
        camera: std::env::var("VP_SMOKE_CAM").ok().and_then(|v| v.parse().ok()).unwrap_or(0),
        workout: std::env::var("VP_SMOKE_WORKOUT").ok(),
        room: std::env::var("VP_SMOKE_ROOM").ok(),
        ghosts: std::env::var("VP_SMOKE_GHOSTS").ok().map(|v| v.split(',').map(str::to_string).collect()),
        clip: std::env::var("VP_SMOKE_CLIP").ok().and_then(|v| v.parse().ok()),
        shift: std::env::var("VP_SMOKE_SHIFT").ok().and_then(|v| v.parse().ok()),
        scroll: std::env::var("VP_SMOKE_SCROLL").ok(),
        laps: std::env::var("VP_SMOKE_LAPS").ok().and_then(|v| v.parse().ok()),
        route: std::env::var("VP_SMOKE_ROUTE").ok(),
    })
}

/// Receives the automated check's screenshot (a PNG data URL) and notes, writes them out and quits.
#[tauri::command]
pub fn smoke_done(app: AppHandle, state: State<AppState>, png: Option<String>, info: String) {
    if let Some(path) = std::env::var_os("VP_SMOKE").map(PathBuf::from) {
        // VP_SMOKE_REPORT=<path> also saves a diagnostics report there, without the save dialog.
        if let Some(report) = std::env::var_os("VP_SMOKE_REPORT").map(PathBuf::from) {
            let settings = settings_json(&state);
            let devices = serde_json::to_value(state.devices.snapshot().devices).unwrap_or(Value::Null);
            if let Err(e) = crate::diag::report(&report, &settings, &devices, "automated check") {
                eprintln!("diagnostics report failed: {e}");
            }
        }
        if let Some(bytes) = png.as_deref().and_then(|p| p.split(',').nth(1)).and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok()) {
            let _ = std::fs::write(&path, bytes);
        }
        let _ = std::fs::write(path.with_extension("txt"), info);
        // VP_SMOKE_WINDOW=1 also saves the whole window (page and HUD) as <name>.window.png.
        if std::env::var_os("VP_SMOKE_WINDOW").is_some() {
            if let Some(window) = app.get_webview_window("main") {
                let out = path.with_extension("window.png");
                let app = app.clone();
                crate::snapshot::window_png(&window, out, move |r| {
                    if let Err(e) = r {
                        eprintln!("window snapshot: {e}");
                    }
                    app.exit(0);
                });
                return;
            }
        }
        app.exit(0);
    }
}

/// Lets the UI surface its errors in the terminal the app was started from.
#[tauri::command]
pub fn log(message: String) {
    crate::diag::event("ui", &message);
    eprintln!("[ui] {message}");
}
