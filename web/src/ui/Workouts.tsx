import { useEffect, useState } from 'react'
import { errorText } from '../bridge'
import { fmtTime, zoneColor } from '../core/fmt'
import { api, useSettings } from '../state'
import type { ConnectorEntry, PlannedWorkout, WorkoutMeta } from '../types'

/** A workout's power profile as coloured blocks, like the ones on a training plan. */
export function WorkoutShape({ shape, height = 56, at }: { shape: [number, number][]; height?: number; at?: number }): React.JSX.Element {
  const total = shape.length ? shape[shape.length - 1][0] : 1
  const top = Math.max(1.3, ...shape.map((p) => p[1])) * 1.05
  const blocks: React.JSX.Element[] = []
  for (let i = 0; i + 1 < shape.length; i += 2) {
    const [t0, a] = shape[i]
    const [t1, b] = shape[i + 1]
    const x0 = (t0 / total) * 1000
    const x1 = (t1 / total) * 1000
    const y = (v: number): number => height - (v / top) * height
    blocks.push(<path key={i} d={`M${x0} ${height}L${x0} ${y(a)}L${x1} ${y(b)}L${x1} ${height}Z`} fill={zoneColor((a + b) / 2, 1)} />)
  }
  return (
    <svg className="workout-shape" viewBox={`0 0 1000 ${height}`} preserveAspectRatio="none" style={{ height }}>
      {blocks}
      {at !== undefined && (
        <line x1={(at / total) * 1000} x2={(at / total) * 1000} y1={0} y2={height} stroke="#fff" strokeWidth={3} vectorEffect="non-scaling-stroke" />
      )}
    </svg>
  )
}

/** "Today", "Tomorrow", or the weekday and date, for a YYYY-MM-DD day. */
function dayLabel(date: string): string {
  const [y, m, d] = date.split('-').map(Number)
  const day = new Date(y, m - 1, d)
  const today = new Date()
  today.setHours(0, 0, 0, 0)
  const ahead = Math.round((day.getTime() - today.getTime()) / 86_400_000)
  if (ahead === 0) return 'Today'
  if (ahead === 1) return 'Tomorrow'
  return day.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'short' })
}

/** The workouts on a connected service's training calendar, each ready to add to the library. */
function PlannedWorkouts({ connector, onAdded }: { connector: ConnectorEntry; onAdded: () => void }): React.JSX.Element {
  const { id, name } = connector.info
  const [planned, setPlanned] = useState<PlannedWorkout[] | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState('')
  useEffect(() => {
    api.connectors.planned(id).then(setPlanned, (err) => setError(errorText(err)))
  }, [id])

  const add = async (w: PlannedWorkout): Promise<void> => {
    setBusy(w.id)
    setError('')
    try {
      await api.connectors.addPlanned(id, w.id)
      setPlanned((list) => list?.map((p) => (p.id === w.id ? { ...p, added: true } : p)) ?? null)
      onAdded()
    } catch (err) {
      setError(`${w.name}: ${errorText(err)}`)
    } finally {
      setBusy('')
    }
  }

  return (
    <section className="planned-workouts">
      <h2>Planned on {name}</h2>
      {error && <div className="error">{error}</div>}
      {planned === null && !error && <p className="muted">Looking at your {name} calendar…</p>}
      {planned?.length === 0 && <p className="muted">No rides planned on your {name} calendar for the next two weeks.</p>}
      {planned && planned.length > 0 && (
        <div className="grid">
          {planned.map((w) => (
            <div className="card workout" key={w.id}>
              <span className="planned-day">{dayLabel(w.date)}</span>
              <h3>{w.name}</h3>
              <div className="route-stats">
                {w.durationS > 0 && <span>{fmtTime(w.durationS)}</span>}
                {w.load !== null && <span>{Math.round(w.load)} load</span>}
              </div>
              {w.description && <p className="muted small planned-notes">{w.description}</p>}
              <button
                className={w.added ? 'ghost' : 'primary'}
                disabled={busy === w.id}
                onClick={() => void add(w)}
                title={w.added ? `Fetch it again from ${name}, in case it changed` : undefined}
              >
                {busy === w.id ? 'Adding…' : w.added ? 'Added · update' : 'Add to my workouts'}
              </button>
            </div>
          ))}
        </div>
      )}
    </section>
  )
}

export function Workouts(): React.JSX.Element {
  const { settings } = useSettings()
  const [list, setList] = useState<WorkoutMeta[]>([])
  const [error, setError] = useState('')
  const [planners, setPlanners] = useState<ConnectorEntry[]>([])
  const refresh = (): void => void api.workouts.list().then(setList)
  useEffect(refresh, [])
  useEffect(() => {
    api.connectors.list().then(
      (all) => setPlanners(all.filter((c) => c.plansWorkouts && c.status.connected)),
      () => setPlanners([])
    )
  }, [])

  const importFiles = async (): Promise<void> => {
    try {
      const r = await api.workouts.importDialog()
      setError(r.problems.join(' · '))
      refresh()
    } catch (err) {
      setError(errorText(err))
    }
  }

  const onDrop = async (e: React.DragEvent): Promise<void> => {
    e.preventDefault()
    const problems: string[] = []
    for (const f of [...e.dataTransfer.files].filter((f) => /\.(zwo|erg|mrc)$/i.test(f.name))) {
      try {
        await api.workouts.importText(f.name, await f.text())
      } catch (err) {
        problems.push(`${f.name}: ${errorText(err)}`)
      }
    }
    setError(problems.join(' · '))
    refresh()
  }

  return (
    <div className="drop-target" onDragOver={(e) => e.preventDefault()} onDrop={(e) => void onDrop(e)}>
      <div className="page-head">
        <div>
          <h1>Workouts</h1>
          <p className="muted">
            Ride a structured session on any route: the trainer holds each target (ERG) while the scenery rolls by. Targets are a share of your FTP, now{' '}
            {settings.rider.ftp} W. Import Zwift <code>.zwo</code>, <code>.erg</code> or <code>.mrc</code> files, or drag them here.
          </p>
        </div>
        <button className="primary" onClick={() => void importFiles()}>
          Import workouts…
        </button>
      </div>
      {error && <div className="error">{error}</div>}
      {planners.map((c) => (
        <PlannedWorkouts key={c.info.id} connector={c} onAdded={refresh} />
      ))}
      {planners.length > 0 && <h2>Your workouts</h2>}
      <div className="grid">
        {list.map((w) => (
          <div className="card workout" key={w.id}>
            <WorkoutShape shape={w.shape} />
            <h3>{w.name}</h3>
            <div className="route-stats">
              <span>{fmtTime(w.durationS)}</span>
              <span>{Math.round(w.stress)} TSS</span>
              {w.builtIn && <span>built in</span>}
            </div>
            <p className="muted small">{w.description}</p>
            {!w.builtIn && (
              <button className="ghost" onClick={() => confirm(`Delete “${w.name}”?`) && void api.workouts.remove(w.id).then(refresh)}>
                Delete
              </button>
            )}
          </div>
        ))}
      </div>
      <p className="muted small">Pick a workout when you start a ride: press Ride on a route and choose it in the set-up dialog.</p>
    </div>
  )
}
