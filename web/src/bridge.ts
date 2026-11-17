// The typed doorway to the Rust side. Nothing else in the UI calls `invoke`.

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type {
  Badge,
  BtCandidate,
  ConnectorEntry,
  ConnectorStatus,
  ControllerAction,
  DevicesSnapshot,
  FitnessOverview,
  GearPreview,
  GearSettings,
  GhostMeta,
  Planned,
  RideInit,
  RideRequest,
  RideSearch,
  RideState,
  RideSummary,
  RouteList,
  RouteMeta,
  Segment,
  Settings,
  UploadRecord,
  WorkoutMeta
} from './types'

/** Subscribes to a backend event; returns a synchronous unsubscribe. */
function on<T>(event: string, cb: (payload: T) => void): () => void {
  const pending = listen<T>(event, (e) => cb(e.payload))
  return () => void pending.then((unlisten) => unlisten())
}

export const api = {
  kv: {
    get: <T>(name: string) => invoke<T | null>('kv_get', { name }),
    set: (name: string, value: unknown) => invoke<void>('kv_set', { name, value }),
    del: (name: string) => invoke<void>('kv_del', { name })
  },
  routes: {
    importDialog: () => invoke<{ added: RouteMeta[]; problems: string[] }>('routes_import_dialog'),
    importText: (name: string, text: string) => invoke<RouteMeta>('routes_import_text', { name, text }),
    importRemote: (id: string, rideId: string, name: string) => invoke<RouteMeta>('route_import_connector', { id, rideId, name }),
    demoMeta: () => invoke<RouteMeta>('demo_meta'),
    /** The built-in velodromes, each as ridden anticlockwise (its ID with `-clockwise` the other way). */
    tracksMeta: () => invoke<RouteMeta[]>('tracks_meta'),
    isLoop: (routeId: string) => invoke<boolean>('route_is_loop', { routeId })
  },
  badges: () => invoke<Badge[]>('badges'),
  chooseDir: () => invoke<string | null>('choose_dir'),
  revealFile: (path: string) => invoke<void>('reveal_file', { path }),
  openExternal: (url: string) => invoke<void>('open_external', { url }),
  devices: {
    list: () => invoke<DevicesSnapshot>('devices_list'),
    onChanged: (cb: (s: DevicesSnapshot) => void) => on('devices:changed', cb),
    scanStart: () => invoke<void>('ble_scan_start'),
    scanStop: () => invoke<void>('ble_scan_stop'),
    onCandidates: (cb: (c: BtCandidate[]) => void) => on('ble:candidates', cb),
    connect: (id: string) => invoke<void>('ble_connect', { id }),
    antStart: () => invoke<void>('ant_start'),
    antStop: () => invoke<void>('ant_stop'),
    demoAdd: () => invoke<void>('demo_add'),
    demoNudge: (delta: number) => invoke<void>('demo_nudge', { delta }),
    remove: (id: string) => invoke<void>('device_remove', { id }),
    /** Looks again for the Bluetooth sensors connected before that aren't connected now. */
    lookForKnown: () => invoke<void>('devices_look_for_known'),
    /** Stops reconnecting a remembered Bluetooth sensor. */
    forget: (id: string) => invoke<void>('device_forget', { id })
  },
  ride: {
    start: (request: RideRequest) => invoke<RideInit>('ride_start', { request }),
    intensity: (delta: number) => invoke<void>('ride_intensity', { delta }),
    skipStep: () => invoke<void>('ride_skip_step'),
    shift: (delta: number) => invoke<void>('ride_shift', { delta }),
    routeData: () => invoke<ArrayBuffer>('ride_route_data'),
    roadMesh: () => invoke<ArrayBuffer>('ride_road_mesh'),
    structures: () => invoke<ArrayBuffer>('ride_structures'),
    barriers: () => invoke<ArrayBuffer>('ride_barriers'),
    trees: () => invoke<ArrayBuffer>('tree_models'),
    chunk: (cx: number, cz: number) => invoke<ArrayBuffer>('terrain_chunk', { cx, cz }),
    far: (cx: number, cz: number) => invoke<ArrayBuffer>('terrain_far', { cx, cz }),
    togglePause: () => invoke<void>('ride_toggle_pause'),
    updateSettings: (settings: Settings) => invoke<void>('ride_update_settings', { settings }),
    finish: () => invoke<RideSummary | null>('ride_finish'),
    abort: () => invoke<void>('ride_abort'),
    onState: (cb: (s: RideState) => void) => on('ride:state', cb),
    onLoading: (cb: (message: string) => void) => on('ride:loading', cb),
    onControllerAction: (cb: (action: ControllerAction) => void) => on('controller:action', cb)
  },
  connectors: {
    list: () => invoke<ConnectorEntry[]>('connectors_list'),
    status: (id: string) => invoke<ConnectorStatus>('connector_status', { id }),
    configure: (id: string, fields: Record<string, string>) => invoke<ConnectorStatus>('connector_configure', { id, fields }),
    connect: (id: string) => invoke<ConnectorStatus>('connector_connect', { id }),
    disconnect: (id: string) => invoke<ConnectorStatus>('connector_disconnect', { id }),
    upload: (id: string, rideId: string, filePath: string, name: string, description: string) =>
      invoke<UploadRecord>('connector_upload', { id, rideId, filePath, name, description }),
    rides: (id: string, routeId: string) => invoke<RideSearch>('connector_rides', { id, routeId }),
    /** The rider's saved routes on the service. */
    routes: (id: string) => invoke<RouteList>('connector_routes', { id }),
    /** One saved route, ready to preview and save like a planned one. */
    route: (id: string, routeId: string) => invoke<Planned>('connector_route', { id, routeId }),
    /** Every ride the rider has on the service, newest first, with a preview of each. */
    rideList: (id: string) => invoke<RideSearch>('connector_ride_list', { id })
  },
  workouts: {
    list: () => invoke<WorkoutMeta[]>('workouts_list'),
    importDialog: () => invoke<{ added: string[]; problems: string[] }>('workouts_import_dialog'),
    importText: (name: string, text: string) => invoke<string>('workout_import_text', { name, text }),
    remove: (id: string) => invoke<void>('workout_delete', { id })
  },
  ghosts: {
    list: (routeId: string) => invoke<GhostMeta[]>('ghosts_list', { routeId }),
    importFile: (routeId: string) => invoke<GhostMeta | null>('ghost_import_file', { routeId }),
    importRemote: (id: string, routeId: string, rideId: string) => invoke<GhostMeta>('ghost_import_connector', { id, routeId, rideId }),
    remove: (routeId: string, id: string) => invoke<void>('ghost_delete', { routeId, id })
  },
  segments: {
    list: (routeId: string) => invoke<Segment[]>('segments_list', { routeId }),
    fetch: (id: string, routeId: string) => invoke<Segment[]>('segments_fetch', { id, routeId })
  },
  photos: {
    forRide: () => invoke<{ distance: number; id: string; url: string; capturedAt: number }[]>('ride_photos'),
    image: (url: string) => invoke<string>('photo_image', { url })
  },
  native: {
    available: () => invoke<boolean>('native_available'),
    start: (assets: Uint8Array) => invoke<void>('native_start', assets),
    stop: () => invoke<void>('native_stop'),
    camera: (index?: number) => invoke<void>('native_camera', { index: index ?? null }),
    look: (look: Record<string, unknown>) => invoke<void>('native_look', { look }),
    capture: () => invoke<[number, string | null]>('native_capture'),
    saving: () => invoke<boolean>('native_saving')
  },
  weatherNow: (lat: number, lon: number) => invoke<number>('weather_now', { lat, lon }),
  fitness: () => invoke<FitnessOverview>('fitness_overview'),
  geo: {
    search: (query: string) => invoke<{ name: string; detail: string; lat: number; lon: number }[]>('geo_search', { query }),
    /** Plans through the waypoints, the section from waypoint i in `styles[i]` (missing ones take the last). */
    plan: (points: [number, number][], styles: string[]) => invoke<Planned>('geo_plan', { points, styles }),
    /** Waypoints for a loop of about `distance` metres from `start`, heading off towards `direction` (degrees from north). */
    roundTrip: (start: [number, number], distance: number, direction: number, style: string) =>
      invoke<[number, number][]>('geo_round_trip', { start, distance, direction, style }),
    classics: () => invoke<{ name: string; region: string; points: [number, number][] }[]>('geo_classics'),
    /** Waymarked routes of a kind (mountain-bike or signposted road) within `[south, west, north, east]`. */
    waymarkedRoutes: (kind: 'mtb' | 'road', bounds: [number, number, number, number]) =>
      invoke<{ id: number; name: string; reference: string; network: string; distance: number | null; bounds: [number, number, number, number] | null }[]>(
        'geo_waymarked_routes',
        { kind, bounds }
      ),
    /** One waymarked route, joined into a line with elevation, ready to save. A road route is cut down to its part within `bounds`. */
    waymarkedRoute: (kind: 'mtb' | 'road', id: number, bounds: [number, number, number, number]) => invoke<Planned>('geo_waymarked_route', { kind, id, bounds })
  },
  group: {
    fetchRoute: (room: string) => invoke<RouteMeta>('group_fetch_route', { room })
  },
  smart: {
    setToken: (token: string) => invoke<void>('smart_set_token', { token }),
    test: () => invoke<void>('smart_test')
  },
  smoke: {
    config: () =>
      invoke<{
        screen: string
        ms: number
        camera: number
        workout: string | null
        room: string | null
        ghosts: string[] | null
        clip: number | null
        shift: number | null
        scroll: string | null
        laps: number | null
        route: string | null
        plan: string | null
      } | null>('smoke_config'),
    frame: (index: number) => invoke<void>('smoke_frame', { index }),
    done: (png: string | null, info: string) => invoke<void>('smoke_done', { png, info })
  },
  gears: {
    preview: (gears: GearSettings) => invoke<GearPreview>('gears_preview', { gears })
  },
  diag: {
    report: (note: string) => invoke<string | null>('diag_report', { note }),
    openFolder: () => invoke<void>('diag_open_folder'),
    issue: (note: string) => invoke<string | null>('diag_issue', { note })
  },
  log: (message: string) => invoke<void>('log', { message })
}

/** Commands reject with a plain string; show that rather than "[object Object]". */
export function errorText(err: unknown): string {
  return typeof err === 'string' ? err : err instanceof Error ? err.message : String(err)
}

export const platform: 'win32' | 'darwin' | 'linux' = /Windows/i.test(navigator.userAgent) ? 'win32' : /Mac/i.test(navigator.userAgent) ? 'darwin' : 'linux'
