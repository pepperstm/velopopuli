import { useEffect, useState } from 'react'
import { errorText } from '../bridge'
import { fmtDist, fmtTime } from '../core/fmt'
import { api, useSettings } from '../state'
import type { ConnectorEntry, GhostMeta, RemoteRide, RideRequest, RideSearch, RouteMeta, Segment, WorkoutMeta } from '../types'
import { WorkoutShape } from './Workouts'

const hex = (c: number): string => `#${c.toString(16).padStart(6, '0')}`

/** Chosen before a ride: a workout, ghosts to race, segments from connected services and a group-ride room. */
export function RideSetup(props: { route: RouteMeta; room?: string; onCancel: () => void; onStart: (r: RideRequest) => void }): React.JSX.Element {
  const { route } = props
  const { settings } = useSettings()
  const u = settings.units
  const [workouts, setWorkouts] = useState<WorkoutMeta[]>([])
  const [workoutId, setWorkoutId] = useState<string>('')
  const [ghosts, setGhosts] = useState<GhostMeta[]>([])
  const [ghostIds, setGhostIds] = useState<string[]>([])
  const [segments, setSegments] = useState<Segment[]>([])
  /** Rides fetched from one connector, while the rider picks one; `all` also lists those off this route. */
  const [remote, setRemote] = useState<{ connector: ConnectorEntry; search: RideSearch; all: boolean } | null>(null)
  const [group, setGroup] = useState(Boolean(props.room))
  const [room, setRoom] = useState(props.room ?? Math.random().toString(36).slice(2, 8).toUpperCase())
  const [busy, setBusy] = useState('')
  const [error, setError] = useState('')
  const [isLoop, setIsLoop] = useState(false)
  const [laps, setLaps] = useState('')
  const [connectors, setConnectors] = useState<ConnectorEntry[]>([])
  const isDemo = route.id === 'demo'
  // A built-in velodrome (its card is the anticlockwise route): which way round to ride it.
  const isTrack = route.id === 'herne-hill'
  const [clockwise, setClockwise] = useState(false)
  const rideId = isTrack && clockwise ? `${route.id}-clockwise` : route.id
  const segmentConnectors = connectors.filter((c) => c.findsSegments && c.status.connected)
  const nameOf = (id: string | undefined): string => connectors.find((c) => c.info.id === id)?.info.name ?? id ?? 'a service'
  const sourceNames = [...new Set(segments.map((s) => nameOf(s.connector)))].join(' and ')

  useEffect(() => {
    void api.workouts.list().then(setWorkouts)
    void api.routes.isLoop(rideId).then(setIsLoop, () => setIsLoop(false))
    void api.connectors.list().then(setConnectors)
    if (!isDemo) {
      void api.ghosts.list(rideId).then((g) => {
        setGhosts(g)
        setGhostIds(g.map((x) => x.id))
      })
      void api.segments.list(rideId).then(setSegments)
    }
  }, [rideId, isDemo])

  const task = async (label: string, fn: () => Promise<void>): Promise<void> => {
    setBusy(label)
    setError('')
    try {
      await fn()
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy('')
    }
  }

  const addGhost = (g: GhostMeta | null): void => {
    if (!g) return
    setGhosts((all) => [...all, g])
    setGhostIds((ids) => [...ids, g.id])
  }
  const pickRemote = (a: RemoteRide): void => {
    if (!remote) return
    const { connector } = remote
    void task(`Fetching the ride from ${connector.info.name}…`, async () => {
      addGhost(await api.ghosts.importRemote(connector.info.id, rideId, a.id))
      setRemote(null)
    })
  }

  const lapRide = isLoop && laps !== ''
  const chosen = workouts.find((w) => w.id === workoutId)

  return (
    <div className="ride-overlay solid setup-backdrop">
      <div className="modal setup">
        <h2>{route.name}</h2>
        <p className="muted">
          {fmtDist(route.distance, u)} · {route.climbs} climb{route.climbs === 1 ? '' : 's'}
        </p>

        {isTrack && (
          <>
            <h3>Direction</h3>
            <select value={clockwise ? 'clockwise' : 'anticlockwise'} onChange={(e) => setClockwise(e.target.value === 'clockwise')}>
              <option value="anticlockwise">Anticlockwise, as raced</option>
              <option value="clockwise">Clockwise</option>
            </select>
          </>
        )}

        {isLoop && (
          <>
            <h3>Laps</h3>
            <select value={laps} onChange={(e) => setLaps(e.target.value)}>
              <option value="">Once round</option>
              {[2, 3, 5, 10, 20].map((n) => (
                <option key={n} value={n}>
                  {n} laps
                </option>
              ))}
              <option value="0">Until I stop</option>
            </select>
            {laps !== '' && <p className="muted small">Ghosts are off on a lap ride, and it won't set the route's best time.</p>}
          </>
        )}

        <h3>Workout</h3>
        <select value={workoutId} onChange={(e) => setWorkoutId(e.target.value)}>
          <option value="">No workout: free ride, the gradient sets the effort</option>
          {workouts.map((w) => (
            <option key={w.id} value={w.id}>
              {w.name} · {fmtTime(w.durationS)} · {Math.round(w.stress)} TSS
            </option>
          ))}
        </select>
        {chosen && (
          <>
            <WorkoutShape shape={chosen.shape} height={40} />
            <p className="muted small">
              {chosen.description}{' '}
              {settings.trainer.workoutErg ? 'The trainer holds each target (ERG).' : 'Targets are shown; the trainer follows the gradient.'}
            </p>
          </>
        )}

        {!isDemo && (
          <>
            {!lapRide && (
              <>
                <h3>Ghosts to race</h3>
                {ghosts.length === 0 && (
                  <p className="muted small">Import someone's recorded ride of this road (GPX or FIT, or from a connected service) to race it.</p>
                )}
                {ghosts.map((g) => (
                  <label key={g.id} className="field toggle ghost-row">
                    <input
                      type="checkbox"
                      checked={ghostIds.includes(g.id)}
                      onChange={(e) => setGhostIds((ids) => (e.target.checked ? [...ids, g.id] : ids.filter((i) => i !== g.id)))}
                    />
                    <i style={{ background: hex(g.color) }} />
                    <span>
                      {g.name}{' '}
                      <em className="muted small">
                        {fmtDist(g.distance, u)} in {fmtTime(g.timeS)}
                      </em>
                    </span>
                    <button
                      className="ghost small"
                      onClick={(e) => {
                        e.preventDefault()
                        void api.ghosts.remove(rideId, g.id).then(() => setGhosts((all) => all.filter((x) => x.id !== g.id)))
                      }}
                    >
                      ✕
                    </button>
                  </label>
                ))}
                <div className="row">
                  <button disabled={!!busy} onClick={() => void task('Reading the ride…', async () => addGhost(await api.ghosts.importFile(rideId)))}>
                    From a file…
                  </button>
                  {connectors
                    .filter((c) => c.listsRides && c.status.connected)
                    .map((c) => (
                      <button
                        key={c.info.id}
                        disabled={!!busy}
                        onClick={() =>
                          void task(`Loading your ${c.info.name} rides…`, async () =>
                            setRemote({ connector: c, search: await api.connectors.rides(c.info.id, rideId), all: false })
                          )
                        }
                      >
                        From {c.info.name}…
                      </button>
                    ))}
                </div>
                {remote && (
                  <RemotePicker
                    remote={remote}
                    routeLength={route.distance}
                    busy={!!busy}
                    onAll={() => setRemote({ ...remote, all: true })}
                    onPick={(a) => pickRemote(a)}
                  />
                )}
              </>
            )}

            <h3>Segments</h3>
            <p className="muted small">
              The route's climbs are always timed.{' '}
              {segments.length
                ? `${segments.length} segment${segments.length === 1 ? '' : 's'} from ${sourceNames} on this route: ${segments
                    .slice(0, 4)
                    .map((s) => s.name)
                    .join(', ')}${segments.length > 4 ? '…' : ''}`
                : 'Segments from a connected service can be timed too.'}
            </p>
            {segmentConnectors.length ? (
              <div className="row">
                {segmentConnectors.map((c) => (
                  <button
                    key={c.info.id}
                    disabled={!!busy}
                    onClick={() => void task(`Looking for ${c.info.name} segments…`, async () => setSegments(await api.segments.fetch(c.info.id, rideId)))}
                  >
                    {segments.some((s) => s.connector === c.info.id) ? `Refresh ${c.info.name} segments` : `Find ${c.info.name} segments`}
                  </button>
                ))}
              </div>
            ) : (
              <p className="muted small">
                Connect{' '}
                {connectors
                  .filter((c) => c.findsSegments)
                  .map((c) => c.info.name)
                  .join(' or ') || 'a service'}{' '}
                in Settings to find segments.
              </p>
            )}
          </>
        )}

        <h3>Group ride</h3>
        <label className="field toggle">
          <input type="checkbox" checked={group} onChange={(e) => setGroup(e.target.checked)} />
          <span>Ride with friends</span>
          <small>Share the room code; they choose “Join a group ride” on the Routes page. Relay: {settings.group.relayUrl}</small>
        </label>
        {group && (
          <label className="field">
            <span>Room code</span>
            <input
              value={room}
              onChange={(e) =>
                setRoom(
                  e.target.value
                    .toUpperCase()
                    .replace(/[^A-Z0-9-]/g, '')
                    .slice(0, 32)
                )
              }
            />
          </label>
        )}

        {busy && <div className="muted">{busy}</div>}
        {error && <div className="error">{error}</div>}
        <div className="row">
          <button
            className="primary"
            disabled={!!busy || (group && !room)}
            onClick={() =>
              props.onStart({
                routeId: rideId,
                workoutId: workoutId || null,
                ghostIds: lapRide ? [] : ghostIds,
                groupRoom: group ? room : null,
                laps: lapRide ? Number(laps) : null
              })
            }
          >
            Start riding
          </button>
          <button onClick={props.onCancel}>Cancel</button>
        </div>
      </div>
    </div>
  )
}

/** The rides that follow this route first; the rest on request, since a ride may still fit
 *  where its simplified outline didn't. */
function RemotePicker(props: {
  remote: { connector: ConnectorEntry; search: RideSearch; all: boolean }
  /** Metres. */
  routeLength: number
  busy: boolean
  onAll: () => void
  onPick: (ride: RemoteRide) => void
}): React.JSX.Element {
  const { settings } = useSettings()
  const { connector, search, all } = props.remote
  const name = connector.info.name
  // The ones covering most of the route first; newest first among equals.
  const fitting = search.rides.filter((r) => r.followsRoute).sort((a, b) => Math.round(b.followedM / 500) - Math.round(a.followedM / 500))
  const others = search.rides.filter((r) => !r.followsRoute)
  const row = (a: RemoteRide): React.JSX.Element => (
    <button key={a.id} className="ghost" disabled={props.busy} onClick={() => props.onPick(a)}>
      {a.name} · {a.start.slice(0, 10)} · {fmtDist(a.distance, settings.units)}
      {a.followsRoute && ` · ${Math.min(100, Math.round((a.followedM / props.routeLength) * 100))} % of the route`}
    </button>
  )
  const looked = `${search.rides.length}${search.more ? ' most recent' : ''}`
  return (
    <div className="pick-list">
      {search.rides.length === 0 && <div className="muted small">No rides with GPS on your {name}.</div>}
      {search.rides.length > 0 && (
        <div className="muted small">
          {fitting.length
            ? `${fitting.length} of your ${looked} ${name} rides ride this route.`
            : `None of your ${looked} ${name} rides seem to ride this route from its start.`}
        </div>
      )}
      {fitting.map(row)}
      {others.length > 0 &&
        (all ? (
          others.map(row)
        ) : (
          <button className="ghost" onClick={props.onAll}>
            Show the other {others.length} rides
          </button>
        ))}
    </div>
  )
}
