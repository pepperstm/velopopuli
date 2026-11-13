import { useEffect, useRef, useState } from 'react'
import L from 'leaflet'
import 'leaflet/dist/leaflet.css'
import { errorText } from '../bridge'
import { distUnit, fmtDist, fmtEle, type Units } from '../core/fmt'
import { api, useSettings } from '../state'
import type { Climb } from '../core/gpx'
import type { ConnectorEntry, Planned, RemoteRoute, RouteList, RouteMeta, SurfaceClass, WayClass } from '../types'

interface Place {
  name: string
  detail: string
  lat: number
  lon: number
}

interface Classic {
  name: string
  region: string
  points: [number, number][]
}

type LatLon = [number, number]
type Kind = 'mtb' | 'road'
type Bounds = [number, number, number, number]

/** The two kinds of waymarked route, as their sections on the page. */
const KINDS: { kind: Kind; title: string; about: string; find: string; looking: string; none: string; numbered: string }[] = [
  {
    kind: 'road',
    title: 'Signposted road routes',
    about:
      'Cycle routes on the signs, mapped in OpenStreetMap: local loops, regional routes and national ones. A long route is cut to its part in the area on the map.',
    find: 'Find road routes here',
    looking: 'Looking for signposted road routes…',
    none: 'No signposted cycle routes here. Try another area, or plan one by clicking on the map.',
    numbered: 'Cycle route'
  },
  {
    kind: 'mtb',
    title: 'Mountain-bike routes',
    about: 'Waymarked trails mapped in OpenStreetMap, in the area on the map.',
    find: 'Find MTB routes here',
    looking: 'Looking for mountain-bike routes…',
    none: 'No waymarked mountain-bike routes here. Try another area, or plan one with the “Mountain bike” style.',
    numbered: 'MTB route'
  }
]

interface WaymarkedRoute {
  id: number
  name: string
  reference: string
  network: string
  distance: number | null
  bounds: [number, number, number, number] | null
}

const NETWORKS: Record<string, string> = { lcn: 'local', rcn: 'regional', ncn: 'national', icn: 'international' }

/** Routing styles, as BRouter profiles: the same list as `planner::STYLES` in Rust. */
const STYLES = [
  { id: 'fastbike', label: 'Road bike', hint: 'Fast, direct roads' },
  { id: 'fastbike-verylowtraffic', label: 'Quiet roads', hint: 'Lanes and minor roads, away from traffic' },
  { id: 'gravel', label: 'Gravel', hint: 'Gravel roads and good tracks where there are some' },
  { id: 'trekking', label: 'Touring', hint: 'A mix of quiet roads, cycle paths and good tracks' },
  { id: 'safety', label: 'Safest', hint: 'Cycle paths and the quietest roads, even if longer' },
  { id: 'mtb', label: 'Mountain bike', hint: 'Trails and paths' }
]

const SURFACES: Record<SurfaceClass, { label: string; colour: string }> = {
  paved: { label: 'Paved', colour: '#d4ee2a' },
  cobbles: { label: 'Cobbles', colour: '#ff9f43' },
  gravel: { label: 'Gravel', colour: '#c9a36b' },
  dirt: { label: 'Dirt and grass', colour: '#a0673a' },
  unknown: { label: 'Not mapped', colour: '#7d8590' }
}

const WAYS: Record<WayClass, { label: string; colour: string }> = {
  mainRoad: { label: 'Main roads', colour: '#ff6b6b' },
  minorRoad: { label: 'Minor roads', colour: '#d4ee2a' },
  cyclePath: { label: 'Cycle paths', colour: '#4ddb8a' },
  track: { label: 'Tracks', colour: '#c9a36b' },
  path: { label: 'Paths', colour: '#b38bff' }
}

const DIRECTIONS: { label: string; degrees: number | null }[] = [
  { label: 'Any way', degrees: null },
  { label: 'North', degrees: 0 },
  { label: 'North-east', degrees: 45 },
  { label: 'East', degrees: 90 },
  { label: 'South-east', degrees: 135 },
  { label: 'South', degrees: 180 },
  { label: 'South-west', degrees: 225 },
  { label: 'West', degrees: 270 },
  { label: 'North-west', degrees: 315 }
]

/** "Try another" in a chosen direction turns a little each time, so each loop is different. */
const NUDGES = [0, 25, -25, 50, -50, 75, -75]

/** The waypoints, and the style of each section between them (`styles[i]` from waypoint i). */
interface Plan {
  points: LatLon[]
  styles: string[]
}

const EMPTY: Plan = { points: [], styles: [] }

/** How a waypoint is shown on the map: S, 1, 2, …, F. */
function waypointLabel(i: number, count: number): string {
  return i === 0 ? 'S' : i === count - 1 ? 'F' : String(i)
}

function fmtDuration(s: number): string {
  const minutes = Math.max(1, Math.round(s / 60))
  const h = Math.floor(minutes / 60)
  return h ? `${h} h ${String(minutes % 60).padStart(2, '0')} min` : `${minutes} min`
}

function climbName(c: Climb): string {
  return c.category === 'HC' ? 'HC climb' : c.category ? `Cat ${c.category} climb` : 'Climb'
}

function Elevation({ profile, climbs }: { profile: [number, number][]; climbs: Climb[] }): React.JSX.Element | null {
  if (profile.length < 2) return null
  const W = 600
  const H = 90
  const total = profile[profile.length - 1][0] || 1
  const lo = Math.min(...profile.map((p) => p[1]))
  const hi = Math.max(lo + 50, ...profile.map((p) => p[1]))
  const pts = profile.map(([d, e]) => `${((d / total) * W).toFixed(1)},${(H - ((e - lo) / (hi - lo)) * (H - 6)).toFixed(1)}`)
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="chart elevation" preserveAspectRatio="none">
      {climbs.map((c, i) => (
        <rect key={i} className="climb" x={(c.start / total) * W} width={((c.end - c.start) / total) * W} y={0} height={H} />
      ))}
      <path d={`M0,${H}L${pts.join('L')}L${W},${H}Z`} />
    </svg>
  )
}

/** How much of the route is each surface (or kind of way): a bar, and a legend that shows it on the map. */
function Shares<T extends string>({
  title,
  shares,
  info,
  total,
  units,
  focus,
  onFocus
}: {
  title: string
  shares: { class: T; metres: number }[]
  info: Record<T, { label: string; colour: string }>
  total: number
  units: Units
  focus: string | null
  onFocus: (c: T | null) => void
}): React.JSX.Element {
  return (
    <div className="shares">
      <div className="muted small">{title}</div>
      <div className="share-bar">
        {shares.map((s) => (
          <span key={s.class} style={{ flex: s.metres, background: info[s.class].colour }} />
        ))}
      </div>
      <ul className="share-legend">
        {shares.map((s) => (
          <li key={s.class} className={focus === s.class ? 'on' : ''} onMouseEnter={() => onFocus(s.class)} onMouseLeave={() => onFocus(null)}>
            <i style={{ background: info[s.class].colour }} />
            {info[s.class].label}
            <b>{fmtDist(s.metres, units)}</b>
            <span className="muted">{Math.round((s.metres / Math.max(1, total)) * 100)}%</span>
          </li>
        ))}
      </ul>
    </div>
  )
}

/** Plan a new route on a map, make a loop, or pick one: a famous climb, a waymarked route, one of your own. */
export function Explore({ onSaved }: { onSaved: () => void }): React.JSX.Element {
  const { settings } = useSettings()
  const u = settings.units
  const el = useRef<HTMLDivElement>(null)
  const map = useRef<L.Map | null>(null)
  const layer = useRef<L.LayerGroup | null>(null)
  const [plan, setPlan] = useState<Plan>(EMPTY)
  const [past, setPast] = useState<Plan[]>([])
  const [future, setFuture] = useState<Plan[]>([])
  /** The style new sections get. */
  const [style, setStyle] = useState('fastbike')
  const [planned, setPlanned] = useState<Planned | null>(null)
  /** What the route on the map is coloured by, and the surface or kind of way picked out. */
  const [colourBy, setColourBy] = useState<'route' | 'surface' | 'way'>('route')
  const [focus, setFocus] = useState<string | null>(null)
  const [loopDistance, setLoopDistance] = useState(40)
  const [loopDirection, setLoopDirection] = useState<number | null>(null)
  /** The last loop made, so "Try another" starts from the same place. */
  const [loop, setLoop] = useState<{ start: LatLon; tries: number } | null>(null)
  const [query, setQuery] = useState('')
  const [places, setPlaces] = useState<Place[]>([])
  const [classics, setClassics] = useState<Classic[]>([])
  const [name, setName] = useState('')
  const [busy, setBusy] = useState('')
  const [error, setError] = useState('')
  const [saved, setSaved] = useState('')
  /** Routes found for each kind, and the area they were looked for in. */
  const [found, setFound] = useState<Partial<Record<Kind, { bounds: Bounds; routes: WaymarkedRoute[] }>>>({})
  /** Connected services that keep the rider's own routes, and the routes listed from each. */
  const [connectors, setConnectors] = useState<ConnectorEntry[]>([])
  const [remote, setRemote] = useState<Record<string, RouteList>>({})

  // The map's handlers are set up once; they read the latest plan and style through these.
  const planRef = useRef(plan)
  planRef.current = plan
  const styleRef = useRef(style)
  styleRef.current = style
  /** Set when a drag along the route ends, so the click that follows doesn't add a waypoint too. */
  const inserting = useRef(false)

  /** Changes the plan, so that it can be undone. */
  const edit = (change: (p: Plan) => Plan): void => {
    const current = planRef.current
    const next = change(current)
    if (next === current) return
    setPast((p) => [...p.slice(-49), current])
    setFuture([])
    planRef.current = next
    setPlan(next)
  }
  const undo = (): void => {
    if (!past.length) return
    setFuture((f) => [planRef.current, ...f])
    const previous = past[past.length - 1]
    setPast((p) => p.slice(0, -1))
    setPlan(previous)
  }
  const redo = (): void => {
    if (!future.length) return
    setPast((p) => [...p, planRef.current])
    setPlan(future[0])
    setFuture((f) => f.slice(1))
  }

  const add = (ll: LatLon): void =>
    edit((p) => (p.points.length ? { points: [...p.points, ll], styles: [...p.styles, styleRef.current] } : { points: [ll], styles: [] }))
  /** A waypoint in the middle of section `i`, which is split in two of the same style. */
  const insert = (i: number, ll: LatLon): void =>
    edit((p) => ({
      points: [...p.points.slice(0, i + 1), ll, ...p.points.slice(i + 1)],
      styles: [...p.styles.slice(0, i + 1), p.styles[i] ?? styleRef.current, ...p.styles.slice(i + 1)]
    }))
  const move = (i: number, ll: LatLon): void => edit((p) => ({ ...p, points: p.points.map((q, k) => (k === i ? ll : q)) }))
  /** Removing a waypoint joins the sections either side into one, in the first one's style. */
  const remove = (i: number): void =>
    edit((p) => {
      const n = p.points.length
      const drop = i === 0 ? 0 : i === n - 1 ? n - 2 : i
      return { points: p.points.filter((_, k) => k !== i), styles: n <= 2 ? [] : p.styles.filter((_, k) => k !== drop) }
    })
  const same = (a: LatLon, b: LatLon): boolean => a[0] === b[0] && a[1] === b[1]
  const isLoop = plan.points.length > 2 && same(plan.points[0], plan.points[plan.points.length - 1])

  useEffect(() => {
    const m = L.map(el.current!, { zoomControl: true }).setView([46.2, 7.2], 6)
    m.attributionControl.setPrefix(false)
    L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', { maxZoom: 18, attribution: '© OpenStreetMap contributors' }).addTo(m)
    layer.current = L.layerGroup().addTo(m)
    m.on('click', (e: L.LeafletMouseEvent) => {
      if (inserting.current) {
        inserting.current = false
        return
      }
      add([e.latlng.lat, e.latlng.lng])
    })
    map.current = m
    void api.geo.classics().then(setClassics)
    // Automated check: plan waypoints, or make a loop, as soon as the map is up.
    void api.smoke.config().then((smoke) => {
      const spec = smoke?.plan
      if (!spec) return
      const nums = (s: string): number[] => s.split(',').map(Number)
      if (spec.startsWith('loop:')) {
        const [lat, lon, km] = nums(spec.slice(5))
        m.setView([lat, lon], 11)
        void api.geo.roundTrip([lat, lon], km * 1000, 90, styleRef.current).then((points) => {
          edit(() => ({ points, styles: points.slice(1).map(() => styleRef.current) }))
          m.fitBounds(L.latLngBounds(points).pad(0.15))
        })
      } else {
        const points = spec.split(';').map((p) => nums(p) as LatLon)
        edit(() => ({ points, styles: points.slice(1).map(() => styleRef.current) }))
        m.fitBounds(L.latLngBounds(points).pad(0.3))
      }
    })
    void api.connectors.list().then((all) => setConnectors(all.filter((c) => c.listsRoutes && c.status.connected)))
    return () => {
      m.remove()
      map.current = null
    }
    // The handlers read the plan through refs, so the map is made once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Undo and redo from the keyboard, when not typing.
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== 'z') return
      const t = e.target as HTMLElement
      if (t.tagName === 'INPUT' || t.tagName === 'SELECT' || t.tagName === 'TEXTAREA') return
      e.preventDefault()
      if (e.shiftKey) redo()
      else undo()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

  /** Which section of the planned route a point on its line is in. */
  const sectionAt = (p: Planned, ll: L.LatLng): number => {
    const k = Math.cos((ll.lat * Math.PI) / 180)
    let best = 0
    let bestD = Infinity
    p.line.forEach(([lat, lon], i) => {
      const d = (lat - ll.lat) ** 2 + ((lon - ll.lng) * k) ** 2
      if (d < bestD) {
        bestD = d
        best = i
      }
    })
    const section = p.ends.filter((e) => e <= best).length - 1
    return Math.max(0, Math.min(planRef.current.points.length - 2, section))
  }

  // Draw the route and the waypoints.
  useEffect(() => {
    const g = layer.current
    const m = map.current
    if (!g || !m) return
    g.clearLayers()
    if (planned) {
      L.polyline(planned.line, { color: '#24271f', weight: 8, opacity: 0.85, interactive: false }).addTo(g)
      if (colourBy === 'route' || !planned.breakdown) {
        L.polyline(planned.line, { color: '#d4ee2a', weight: 5, opacity: 1, interactive: false }).addTo(g)
      } else {
        for (const run of planned.breakdown.runs) {
          const key = colourBy === 'surface' ? run.surface : run.way
          const colour = colourBy === 'surface' ? SURFACES[run.surface].colour : WAYS[run.way].colour
          const dim = focus !== null && focus !== key
          L.polyline(run.line, { color: colour, weight: focus === key ? 7 : 5, opacity: dim ? 0.25 : 1, interactive: false }).addTo(g)
        }
      }
      // A wide, invisible line along the route to grab: drag from it to add a waypoint there.
      if (planned.ends.length === plan.points.length && plan.points.length >= 2) {
        const grab = L.polyline(planned.line, { weight: 22, opacity: 0.001, bubblingMouseEvents: false, className: 'route-grab' })
        grab.on('mousedown', (e: L.LeafletMouseEvent) => {
          L.DomEvent.stop(e)
          const section = sectionAt(planned, e.latlng)
          m.dragging.disable()
          const ghost = L.marker(e.latlng, {
            icon: L.divIcon({ className: 'wp ghost', html: '<span>+</span>', iconSize: [22, 22] }),
            interactive: false
          }).addTo(g)
          const follow = (ev: L.LeafletMouseEvent): void => {
            ghost.setLatLng(ev.latlng)
          }
          const drop = (ev: L.LeafletMouseEvent): void => {
            m.off('mousemove', follow)
            m.off('mouseup', drop)
            m.dragging.enable()
            ghost.remove()
            inserting.current = true
            setTimeout(() => (inserting.current = false), 300)
            insert(section, [ev.latlng.lat, ev.latlng.lng])
          }
          m.on('mousemove', follow)
          m.on('mouseup', drop)
        })
        grab.addTo(g)
      }
    }
    plan.points.forEach((p, i) => {
      // On a loop the finish is the start: one marker for both.
      if (isLoop && i === plan.points.length - 1) return
      const marker = L.marker(p, {
        draggable: true,
        icon: L.divIcon({ className: 'wp', html: `<span>${isLoop && i === 0 ? 'S/F' : waypointLabel(i, plan.points.length)}</span>`, iconSize: [22, 22] })
      })
      marker.on('dragend', () => {
        const ll = marker.getLatLng()
        const at: LatLon = [ll.lat, ll.lng]
        // Moving a loop's start moves its finish with it.
        if (isLoop && i === 0) edit((q) => ({ ...q, points: q.points.map((x, k) => (k === 0 || k === q.points.length - 1 ? at : x)) }))
        else move(i, at)
      })
      marker.on('click', () => remove(i))
      marker.addTo(g)
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [plan, planned, colourBy, focus])

  // Plan whenever the waypoints or the styles change.
  useEffect(() => {
    setSaved('')
    if (plan.points.length < 2) {
      setPlanned(null)
      return
    }
    let live = true
    const timer = setTimeout(() => {
      setBusy('Planning the route…')
      setError('')
      api.geo.plan(plan.points, plan.styles).then(
        (p) => live && (setPlanned(p), setBusy('')),
        (err) => live && (setError(errorText(err)), setBusy(''))
      )
    }, 300)
    return () => {
      live = false
      clearTimeout(timer)
    }
  }, [plan])

  const search = async (): Promise<void> => {
    if (!query.trim()) return
    setBusy('Searching…')
    try {
      setPlaces(await api.geo.search(query))
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy('')
    }
  }

  const makeLoop = async (again: boolean): Promise<void> => {
    const m = map.current
    if (!m) return
    const centre = m.getCenter()
    const start: LatLon = again && loop ? loop.start : (plan.points[0] ?? [centre.lat, centre.lng])
    const tries = again && loop ? loop.tries + 1 : 0
    const direction = loopDirection === null ? Math.random() * 360 : loopDirection + NUDGES[tries % NUDGES.length]
    const metres = loopDistance * (u === 'imperial' ? 1609.344 : 1000)
    setBusy('Making a loop…')
    setError('')
    try {
      const points = await api.geo.roundTrip(start, metres, direction, style)
      setLoop({ start, tries })
      edit(() => ({ points, styles: points.slice(1).map(() => style) }))
      if (!name.trim() || name.startsWith('Loop from')) setName(`Loop from ${(await placeName(start)) ?? 'here'}`)
      m.fitBounds(L.latLngBounds(points).pad(0.15))
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy('')
    }
  }

  /** A short name for the place at a point, from the place search, if it answers. */
  const placeName = async (p: LatLon): Promise<string | null> => {
    try {
      const found = await api.geo.search(`${p[0].toFixed(4)}, ${p[1].toFixed(4)}`)
      return found[0]?.name || null
    } catch {
      return null
    }
  }

  const findWaymarked = async (kind: Kind, looking: string): Promise<void> => {
    const b = map.current?.getBounds()
    if (!b) return
    setBusy(looking)
    setError('')
    try {
      const bounds: Bounds = [b.getSouth(), b.getWest(), b.getNorth(), b.getEast()]
      const routes = await api.geo.waymarkedRoutes(kind, bounds)
      // Road routes: the ones wholly on the map first, ahead of national routes that only pass through.
      const fits = (r: WaymarkedRoute): boolean =>
        !!r.bounds && r.bounds[0] >= bounds[0] && r.bounds[1] >= bounds[1] && r.bounds[2] <= bounds[2] && r.bounds[3] <= bounds[3]
      if (kind === 'road') routes.sort((a, b) => Number(fits(b)) - Number(fits(a)))
      setFound((f) => ({ ...f, [kind]: { bounds, routes } }))
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy('')
    }
  }

  const pickWaymarked = async (kind: Kind, r: WaymarkedRoute, bounds: Bounds): Promise<void> => {
    edit(() => EMPTY)
    setBusy(`Fetching ${r.name} and its elevation…`)
    setError('')
    try {
      const p = await api.geo.waymarkedRoute(kind, r.id, bounds)
      setPlanned(p)
      setName(p.name ?? r.name)
      map.current?.fitBounds(L.latLngBounds(p.line).pad(0.15))
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy('')
    }
  }

  const listRemote = async (c: ConnectorEntry): Promise<void> => {
    setBusy(`Loading your ${c.info.name} routes…`)
    setError('')
    try {
      const list = await api.connectors.routes(c.info.id)
      setRemote((all) => ({ ...all, [c.info.id]: list }))
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy('')
    }
  }

  const pickRemote = async (c: ConnectorEntry, r: RemoteRoute): Promise<void> => {
    edit(() => EMPTY)
    setBusy(`Fetching ${r.name} from ${c.info.name}…`)
    setError('')
    try {
      const p = await api.connectors.route(c.info.id, r.id)
      setPlanned(p)
      setName(r.name)
      map.current?.fitBounds(L.latLngBounds(p.line).pad(0.15))
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy('')
    }
  }

  const pickClassic = (c: Classic): void => {
    setName(c.name)
    edit(() => ({ points: c.points, styles: c.points.slice(1).map(() => style) }))
    map.current?.fitBounds(L.latLngBounds(c.points).pad(0.4))
  }

  const save = async (): Promise<void> => {
    if (!planned) return
    try {
      const meta = await api.routes.importText(
        name.trim() || 'Planned route',
        planned.gpx.replace(/<name>[^<]*<\/name>/, `<name>${escapeXml(name.trim() || 'Planned route')}</name>`)
      )
      const routes = (await api.kv.get<RouteMeta[]>('routes')) ?? []
      await api.kv.set('routes', [meta, ...routes])
      setSaved(meta.name)
      onSaved()
    } catch (err) {
      setError(errorText(err))
    }
  }

  const n = plan.points.length
  const sectionLengths = planned && planned.sections.length === plan.styles.length ? planned.sections : null
  const showOnMap = (by: 'surface' | 'way') => (c: string | null) => {
    if (c) setColourBy(by)
    setFocus(c)
  }

  return (
    <div className="explore">
      <div className="explore-side">
        <h1>Find a route</h1>
        <p className="muted small">
          Click the map to add waypoints from start to finish. Drag the route to add a waypoint in between, drag a waypoint to move it, and click one to remove
          it. Routing by BRouter, with real elevation.
        </p>
        <div className="row">
          <input
            value={query}
            placeholder="Search for a place"
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && void search()}
          />
          <button onClick={() => void search()}>Search</button>
        </div>
        {places.length > 0 && (
          <div className="pick-list">
            {places.map((p, i) => (
              <button
                key={i}
                className="ghost"
                onClick={() => {
                  map.current?.setView([p.lat, p.lon], 13)
                  setPlaces([])
                }}
              >
                <b>{p.name}</b> <span className="muted small">{p.detail}</span>
              </button>
            ))}
          </div>
        )}
        <label className="field">
          <span>Style for new sections</span>
          <select value={style} onChange={(e) => setStyle(e.target.value)}>
            {STYLES.map((s) => (
              <option key={s.id} value={s.id}>
                {s.label}: {s.hint}
              </option>
            ))}
          </select>
        </label>
        <div className="row wrap">
          <button disabled={!past.length} onClick={undo} title="Undo (⌘Z / Ctrl+Z)">
            Undo
          </button>
          <button disabled={!future.length} onClick={redo} title="Redo (⇧⌘Z / Ctrl+Shift+Z)">
            Redo
          </button>
          <button disabled={n < 2} onClick={() => edit((p) => ({ points: [...p.points].reverse(), styles: [...p.styles].reverse() }))}>
            Reverse
          </button>
          <button
            disabled={n < 2 || isLoop}
            title="Back to the start from the finish"
            onClick={() => edit((p) => ({ points: [...p.points, p.points[0]], styles: [...p.styles, p.styles[p.styles.length - 1] ?? styleRef.current] }))}
          >
            Close the loop
          </button>
          <button
            disabled={n < 2}
            title="Ride back the way you came"
            onClick={() => edit((p) => ({ points: [...p.points, ...p.points.slice(0, -1).reverse()], styles: [...p.styles, ...[...p.styles].reverse()] }))}
          >
            Out and back
          </button>
          <button disabled={!n} onClick={() => (edit(() => EMPTY), setName(''), setLoop(null))}>
            Clear
          </button>
        </div>

        <section className="card make-loop">
          <h2>Make a loop</h2>
          <p className="muted small">
            A round trip from {n ? 'your start (S)' : 'the middle of the map'}, in the style for new sections. Each one can be changed like any route.
          </p>
          <div className="row">
            <input
              type="number"
              min={u === 'imperial' ? 2 : 3}
              max={u === 'imperial' ? 180 : 300}
              value={loopDistance}
              onChange={(e) => setLoopDistance(Math.max(1, Number(e.target.value) || 0))}
              aria-label="Loop distance"
            />
            <span>{distUnit(u)}</span>
            <select
              value={loopDirection ?? ''}
              onChange={(e) => setLoopDirection(e.target.value === '' ? null : Number(e.target.value))}
              aria-label="Heading off"
            >
              {DIRECTIONS.map((d) => (
                <option key={d.label} value={d.degrees ?? ''}>
                  {d.degrees === null ? d.label : `Heading ${d.label.toLowerCase()}`}
                </option>
              ))}
            </select>
          </div>
          <div className="row">
            <button className="primary" disabled={!!busy} onClick={() => void makeLoop(false)}>
              Make a loop
            </button>
            <button disabled={!!busy || !loop} onClick={() => void makeLoop(true)}>
              Try another
            </button>
          </div>
        </section>

        {busy && <div className="muted">{busy}</div>}
        {error && <div className="error">{error}</div>}

        {n >= 2 && (
          <div className="card sections">
            <div className="muted small">Sections</div>
            {plan.styles.map((s, i) => (
              <div className="section-row" key={i}>
                <span className="section-ends">
                  {isLoop && i === 0 ? 'S/F' : waypointLabel(i, n)} → {isLoop && i === n - 2 ? 'S/F' : waypointLabel(i + 1, n)}
                </span>
                <span className="muted small">{sectionLengths ? fmtDist(sectionLengths[i], u) : ''}</span>
                <select value={s} onChange={(e) => edit((p) => ({ ...p, styles: p.styles.map((x, k) => (k === i ? e.target.value : x)) }))}>
                  {STYLES.map((st) => (
                    <option key={st.id} value={st.id}>
                      {st.label}
                    </option>
                  ))}
                </select>
              </div>
            ))}
            {plan.styles.some((s) => s !== style) && (
              <button className="ghost small" onClick={() => edit((p) => ({ ...p, styles: p.styles.map(() => style) }))}>
                Make every section “{STYLES.find((s) => s.id === style)?.label}”
              </button>
            )}
          </div>
        )}

        {planned && (
          <div className="card planned">
            <div className="route-stats">
              <span>{fmtDist(planned.distance, u)}</span>
              <span>↗ {fmtEle(planned.ascent, u)}</span>
              <span>
                {planned.climbs} climb{planned.climbs === 1 ? '' : 's'}
              </span>
            </div>
            {planned.estimate.seconds > 0 && (
              <div className="small">
                About <b>{fmtDuration(planned.estimate.seconds)}</b> at {planned.estimate.watts} W, three quarters of your FTP.
              </div>
            )}
            <Elevation profile={planned.profile} climbs={planned.climbList} />
            {planned.climbList.length > 0 && (
              <ul className="climb-list small">
                {planned.climbList.slice(0, 8).map((c, i) => (
                  <li key={i}>
                    <b>{climbName(c)}</b> from {fmtDist(c.start, u)}: {fmtDist(c.length, u)} at {(c.avgGrade * 100).toFixed(1)}%, ↗ {fmtEle(c.gain, u)}
                  </li>
                ))}
              </ul>
            )}
            {planned.breakdown && planned.breakdown.runs.length > 0 && (
              <>
                <div className="row colour-by">
                  <span className="muted small">On the map:</span>
                  {(['route', 'surface', 'way'] as const).map((by) => (
                    <button key={by} className={`small ${colourBy === by ? 'on' : 'ghost'}`} onClick={() => (setColourBy(by), setFocus(null))}>
                      {by === 'route' ? 'Route' : by === 'surface' ? 'Surface' : 'Road type'}
                    </button>
                  ))}
                </div>
                <Shares
                  title="Surface"
                  shares={planned.breakdown.surfaces}
                  info={SURFACES}
                  total={planned.breakdown.runs[planned.breakdown.runs.length - 1].to}
                  units={u}
                  focus={colourBy === 'surface' ? focus : null}
                  onFocus={showOnMap('surface')}
                />
                <Shares
                  title="Road type"
                  shares={planned.breakdown.ways}
                  info={WAYS}
                  total={planned.breakdown.runs[planned.breakdown.runs.length - 1].to}
                  units={u}
                  focus={colourBy === 'way' ? focus : null}
                  onFocus={showOnMap('way')}
                />
              </>
            )}
            {!!planned.gaps && (
              <div className="muted small">
                The map's pieces of this route don't meet in {planned.gaps} place{planned.gaps === 1 ? '' : 's'}; you'll ride straight across
                {planned.gaps === 1 ? ' that gap' : ' those gaps'}.
              </div>
            )}
            {planned.clipped && (
              <div className="muted small">This is the part of the route in the area you searched. Move the map and search again for another stretch.</div>
            )}
            <div className="row">
              <input value={name} placeholder="Name this route" onChange={(e) => setName(e.target.value)} />
              <button className="primary" onClick={() => void save()}>
                Save to library
              </button>
            </div>
            {saved && <div className="ok small">“{saved}” is in your routes, ready to ride.</div>}
          </div>
        )}
        {connectors.map((c) => {
          const list = remote[c.info.id]
          return (
            <section key={c.info.id}>
              <h2>Your {c.info.name} routes</h2>
              <p className="muted small">The routes you've made or saved on {c.info.name}, starred ones first.</p>
              <button disabled={!!busy} onClick={() => void listRemote(c)}>
                {list ? `Refresh my ${c.info.name} routes` : `Show my ${c.info.name} routes`}
              </button>
              {list?.privateHidden && (
                <div className="muted small">
                  Private routes aren't shown: VeloPopuli isn't allowed to read them yet. Connect {c.info.name} again in Settings and allow it, to include them.
                </div>
              )}
              {list && list.routes.length === 0 && <div className="muted small">No routes on your {c.info.name} yet.</div>}
              {list && list.routes.length > 0 && (
                <div className="pick-list">
                  {list.routes.map((r) => (
                    <button key={r.id} className="ghost" disabled={!!busy} onClick={() => void pickRemote(c, r)}>
                      <b>
                        {r.starred ? '★ ' : ''}
                        {r.name}
                      </b>{' '}
                      <span className="muted small">
                        {[fmtDist(r.distance, u), r.ascent ? `↗ ${fmtEle(r.ascent, u)}` : '', r.private ? 'private' : ''].filter(Boolean).join(' · ')}
                      </span>
                    </button>
                  ))}
                </div>
              )}
            </section>
          )
        })}
        {KINDS.map((k) => {
          const here = found[k.kind]
          return (
            <section key={k.kind}>
              <h2>{k.title}</h2>
              <p className="muted small">{k.about}</p>
              <button disabled={!!busy} onClick={() => void findWaymarked(k.kind, k.looking)}>
                {k.find}
              </button>
              {here && here.routes.length === 0 && <div className="muted small">{k.none}</div>}
              {here && here.routes.length > 0 && (
                <div className="pick-list">
                  {here.routes.map((r) => (
                    <button key={r.id} className="ghost" disabled={!!busy} onClick={() => void pickWaymarked(k.kind, r, here.bounds)}>
                      <b>{r.name}</b>{' '}
                      <span className="muted small">
                        {[
                          r.reference && r.name !== `${k.numbered} ${r.reference}` ? r.reference : '',
                          NETWORKS[r.network] ?? '',
                          r.distance ? fmtDist(r.distance, u) : ''
                        ]
                          .filter(Boolean)
                          .join(' · ')}
                      </span>
                    </button>
                  ))}
                </div>
              )}
            </section>
          )
        })}
        <h2>Famous climbs</h2>
        <div className="pick-list classics">
          {classics.map((c) => (
            <button key={c.name} className="ghost" onClick={() => pickClassic(c)}>
              <b>{c.name}</b> <span className="muted small">{c.region}</span>
            </button>
          ))}
        </div>
      </div>
      <div ref={el} className="explore-map" />
    </div>
  )
}

function escapeXml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}
