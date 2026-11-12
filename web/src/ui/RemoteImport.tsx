import { useEffect, useMemo, useRef, useState } from 'react'
import { errorText } from '../bridge'
import { distUnit, distValue, fmtDist, fmtTime } from '../core/fmt'
import { api, useSettings } from '../state'
import type { ConnectorEntry, RemoteRide, RouteMeta } from '../types'
import { RoutePreview } from './Home'

/** Strava's sport types, which Dreeve uses too, as the bike they were ridden on. */
const BIKES: Record<string, string> = {
  Ride: 'Road',
  GravelRide: 'Gravel',
  MountainBikeRide: 'Mountain bike',
  EBikeRide: 'E-bike',
  EMountainBikeRide: 'E-mountain bike',
  VirtualRide: 'Virtual',
  Velomobile: 'Velomobile',
  Handcycle: 'Handcycle'
}
const bikeName = (sport: string): string => BIKES[sport] ?? (sport || 'Other')

const fmtDate = (start: string): string => {
  const d = new Date(start)
  return Number.isNaN(d.getTime()) ? start : d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' })
}

/**
 * Two handles on one track for a whole-number range from 0 to `max`. Drawn from plain elements
 * because two overlapping native range inputs can't share the pointer in WebKitGTK.
 */
function DualRange(props: { max: number; value: [number, number]; unit: string; onChange: (v: [number, number]) => void }): React.JSX.Element {
  const { max, value, unit, onChange } = props
  const [lo, hi] = value
  const track = useRef<HTMLDivElement>(null)
  /** The handle being dragged: 0 for the shortest, 1 for the longest, 'either' until a stacked pair is moved. */
  const dragging = useRef<0 | 1 | 'either' | null>(null)

  const at = (clientX: number): number => {
    const box = track.current?.getBoundingClientRect()
    if (!box || box.width === 0) return 0
    return Math.round(Math.min(1, Math.max(0, (clientX - box.left) / box.width)) * max)
  }
  const set = (handle: 0 | 1, v: number): void => onChange(handle === 0 ? [Math.min(Math.max(0, v), hi), hi] : [lo, Math.max(Math.min(max, v), lo)])

  const onDown = (e: React.PointerEvent): void => {
    const v = at(e.clientX)
    track.current?.setPointerCapture(e.pointerId)
    // Both handles on the spot pressed: the direction of the drag picks one.
    if (lo === hi && v === lo) {
      dragging.current = 'either'
      return
    }
    // Otherwise the nearer handle jumps there; between two stacked ones, the side pressed decides.
    const handle: 0 | 1 = Math.abs(v - lo) < Math.abs(v - hi) || (lo === hi && v < lo) ? 0 : 1
    dragging.current = handle
    set(handle, v)
  }
  const onMove = (e: React.PointerEvent): void => {
    if (dragging.current === null) return
    const v = at(e.clientX)
    if (dragging.current === 'either') {
      if (v === lo) return
      dragging.current = v < lo ? 0 : 1
    }
    set(dragging.current, v)
  }
  const onUp = (e: React.PointerEvent): void => {
    dragging.current = null
    track.current?.releasePointerCapture(e.pointerId)
  }
  const onKey = (handle: 0 | 1) => (e: React.KeyboardEvent) => {
    const now = handle === 0 ? lo : hi
    const step = e.shiftKey ? 10 : 1
    const next = { ArrowLeft: now - step, ArrowDown: now - step, ArrowRight: now + step, ArrowUp: now + step, Home: 0, End: max }[e.key]
    if (next === undefined) return
    e.preventDefault()
    set(handle, next)
  }

  const pct = (v: number): string => `${(v / max) * 100}%`
  const handle = (h: 0 | 1, v: number, label: string): React.JSX.Element => (
    <div
      className="dual-range-thumb"
      style={{ left: pct(v) }}
      role="slider"
      tabIndex={0}
      aria-label={`${label}, in ${unit}`}
      aria-valuemin={0}
      aria-valuemax={max}
      aria-valuenow={v}
      onKeyDown={onKey(h)}
    />
  )

  return (
    <div className="dual-range" ref={track} onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp}>
      <div className="dual-range-rail" />
      <div className="dual-range-fill" style={{ left: pct(lo), width: pct(hi - lo) }} />
      {handle(0, lo, 'Shortest')}
      {handle(1, hi, 'Longest')}
    </div>
  )
}

/** A near-fullscreen list of the rider's rides on a service, to import one or more as routes. */
export function RemoteImport(props: { connector: ConnectorEntry; onAdded: (route: RouteMeta) => void; onClose: () => void }): React.JSX.Element {
  const { connector, onAdded, onClose } = props
  const name = connector.info.name
  const { settings } = useSettings()
  const u = settings.units
  const [rides, setRides] = useState<RemoteRide[] | null>(null)
  const [more, setMore] = useState(false)
  const [error, setError] = useState('')
  const [query, setQuery] = useState('')
  const [bike, setBike] = useState('')
  /** The distance range in the rider's unit; `null` until the rides are in and set its bounds. */
  const [range, setRange] = useState<[number, number] | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [imported, setImported] = useState<string[]>([])

  useEffect(() => {
    api.connectors.rideList(connector.info.id).then(
      (found) => {
        setRides(found.rides)
        setMore(found.more)
      },
      (err) => setError(errorText(err))
    )
  }, [connector.info.id])

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const maxDist = useMemo(() => Math.max(1, Math.ceil(Math.max(0, ...(rides ?? []).map((r) => distValue(r.distance, u))))), [rides, u])
  const [lo, hi] = range ?? [0, maxDist]
  const bikes = useMemo(() => [...new Set((rides ?? []).map((r) => r.sport))].sort((a, b) => bikeName(a).localeCompare(bikeName(b))), [rides])

  const shown = useMemo(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean)
    return (rides ?? []).filter((r) => {
      const d = distValue(r.distance, u)
      const text = `${r.name} ${fmtDate(r.start)}`.toLowerCase()
      return (!bike || r.sport === bike) && d >= lo && d <= hi && words.every((w) => text.includes(w))
    })
  }, [rides, query, bike, lo, hi, u])

  const take = (ride: RemoteRide): void => {
    setBusy(ride.id)
    setError('')
    api.routes.importRemote(connector.info.id, ride.id, ride.name).then(
      (route) => {
        onAdded(route)
        setImported((ids) => [...ids, ride.id])
        setBusy(null)
      },
      (err) => {
        setError(`${ride.name}: ${errorText(err)}`)
        setBusy(null)
      }
    )
  }

  const unit = distUnit(u)
  const filtered = Boolean(query || bike || range)

  return (
    <div className="modal-backdrop remote-import-backdrop" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal remote-import" role="dialog" aria-label={`Import from ${name}`}>
        <div className="remote-import-head">
          <h2>Import from {name}</h2>
          <button className="ghost" onClick={onClose} aria-label="Close">
            ✕
          </button>
        </div>

        <div className="remote-import-filters">
          <input type="search" placeholder="Search by name or date" value={query} autoFocus onChange={(e) => setQuery(e.target.value)} />
          <select value={bike} onChange={(e) => setBike(e.target.value)}>
            <option value="">All bikes</option>
            {bikes.map((s) => (
              <option key={s} value={s}>
                {bikeName(s)}
              </option>
            ))}
          </select>
          <div className="range-filter">
            <span className="muted small">
              {lo}–{hi} {unit}
            </span>
            <DualRange max={maxDist} value={[lo, hi]} unit={unit} onChange={setRange} />
          </div>
          {filtered && (
            <button
              className="ghost small"
              onClick={() => {
                setQuery('')
                setBike('')
                setRange(null)
              }}
            >
              Reset
            </button>
          )}
        </div>

        {error && <div className="error">{error}</div>}
        {!rides && !error && <p className="muted">Loading your {name} rides…</p>}
        {rides && (
          <p className="muted small">
            {shown.length} of {rides.length} rides{more ? ` (the newest ${rides.length}; older ones aren't listed)` : ''}
          </p>
        )}

        <div className="remote-import-list">
          {shown.map((r) => {
            const done = imported.includes(r.id)
            return (
              <div className="card route" key={r.id}>
                <RoutePreview outline={r.preview ?? []} />
                <h3 title={r.name}>{r.name}</h3>
                <div className="route-stats">
                  <span>{fmtDist(r.distance, u)}</span>
                  <span>{fmtTime(r.movingTime)}</span>
                  <span>{bikeName(r.sport)}</span>
                </div>
                <div className="route-best">{fmtDate(r.start)}</div>
                <div className="row">
                  <button className="primary" disabled={done || busy !== null} onClick={() => take(r)}>
                    {done ? 'Imported' : busy === r.id ? 'Importing…' : 'Import'}
                  </button>
                </div>
              </div>
            )
          })}
          {rides && !shown.length && <p className="muted">No ride matches these filters.</p>}
        </div>
      </div>
    </div>
  )
}
