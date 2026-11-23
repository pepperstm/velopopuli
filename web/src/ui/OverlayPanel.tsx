import { useState } from 'react'
import { fmtDist, fmtTime, gradeColor, powerZone, speedUnit, speedValue, zoneColor } from '../core/fmt'
import type { Route } from '../core/gpx'
import { api } from '../state'
import type { RideInit, RideState, Settings } from '../types'
import { MiniMap } from './MiniMap'

/** How see-through the panel's background can be, from faint to nearly solid. */
const OPACITIES = [0.3, 0.5, 0.7, 0.9]
const OPACITY_KEY = 'vp-overlay-opacity'

function savedOpacity(): number {
  try {
    const v = Number(localStorage.getItem(OPACITY_KEY))
    return OPACITIES.includes(v) ? v : 0.5
  } catch {
    return 0.5
  }
}

interface Props {
  route: Route
  init: RideInit
  hud: RideState
  settings: Settings
  lapRide: boolean
  onLeave: () => void
  onPause: () => void
}

/**
 * The ride as a small panel floating over other apps (overlay mode): the numbers and the map
 * only, on a see-through background, so a video can play underneath.
 */
export function OverlayPanel({ route, init, hud, settings, lapRide, onLeave, onPause }: Props): React.JSX.Element {
  const [opacity, setOpacity] = useState(savedOpacity)
  const units = settings.units
  const ftp = settings.rider.ftp
  const cycleOpacity = (): void => {
    const next = OPACITIES[(OPACITIES.indexOf(opacity) + 1) % OPACITIES.length]
    setOpacity(next)
    try {
      localStorage.setItem(OPACITY_KEY, String(next))
    } catch {
      // Remembering it is only a convenience.
    }
  }
  const status = hud.finished
    ? 'Finished'
    : hud.noSensors
      ? 'No sensors connected'
      : hud.paused
        ? 'Paused'
        : !hud.moving
          ? hud.elapsed > 0
            ? 'Stopped'
            : 'Start pedalling'
          : null
  const next = init.workout && hud.workout ? init.workout.steps[hud.workout.index + 1] : undefined

  return (
    <div className="overlay-panel" style={{ '--overlay-alpha': opacity } as React.CSSProperties}>
      <div className="op-head" data-tauri-drag-region>
        <b data-tauri-drag-region>{route.name}</b>
        <span className="op-clock" data-tauri-drag-region>
          {fmtTime(hud.elapsed)}
        </span>
        <button onClick={cycleOpacity} title="How see-through the panel is" aria-label="Change the panel's opacity">
          ◐
        </button>
        <button onClick={onLeave} title="Back to the full window (O)" aria-label="Back to the full window">
          ⤢
        </button>
      </div>

      <div className="op-power" style={{ borderColor: zoneColor(hud.power, ftp) }}>
        <b style={{ color: zoneColor(hud.power, ftp) }}>{Math.round(hud.power)}</b>
        <span>
          W · {(hud.power / settings.rider.weightKg).toFixed(1)} W/kg · Z{powerZone(hud.power, ftp)}
        </span>
      </div>

      <div className="op-tiles">
        <div>
          <b>{speedValue(hud.speed, units).toFixed(1)}</b>
          <span>{speedUnit(units)}</span>
        </div>
        <div>
          <b>{hud.hr === null ? '–' : Math.round(hud.hr)}</b>
          <span>♥ bpm</span>
        </div>
        <div>
          <b>{hud.cadence === null ? '–' : Math.round(hud.cadence)}</b>
          <span>rpm</span>
        </div>
        {hud.gear && (
          <div key={hud.gear.index} className="gear-tile">
            <b>{hud.gear.index + 1}</b>
            <span>gear</span>
          </div>
        )}
      </div>

      <div className="op-road">
        <b style={{ color: gradeColor(hud.grade) }}>
          {hud.grade >= 0 ? '▲' : '▼'} {Math.abs(hud.grade * 100).toFixed(1)}%
        </b>
        <span>
          {lapRide
            ? `Lap ${hud.lap}${init.laps ? ` of ${init.laps}` : ''} · ${fmtDist(hud.ridden, units, 1)}`
            : `${fmtDist(Math.max(0, route.total - hud.distance), units, 1)} to go`}
        </span>
      </div>

      {init.workout && hud.workout && (
        <div className="op-workout">
          <b style={{ color: zoneColor(hud.workoutTarget ?? 0, ftp) }}>
            {hud.workout.kind === 'power' ? `${Math.round(hud.workoutTarget ?? 0)} W` : hud.workout.kind === 'max' ? 'All out' : 'Free ride'}
          </b>
          <span>
            {fmtTime(hud.workout.stepLeftS)} left
            {next
              ? ` · next ${next.kind === 'power' ? `${Math.round(next.from * ftp * hud.intensity)} W` : next.kind === 'max' ? 'all out' : 'free'}`
              : ' · last step'}
          </span>
        </div>
      )}
      {hud.cue && <div className="op-cue">{hud.cue}</div>}
      {status && (
        <div className="op-status">
          <span>{status}</span>
          {hud.paused && <button onClick={onPause}>Resume</button>}
          {hud.noSensors && <button onClick={onLeave}>Open the app</button>}
        </div>
      )}

      <MiniMap route={route} lat={hud.lat} lon={hud.lon} />

      <div className="op-controls">
        <button onClick={onPause}>{hud.paused ? 'Resume' : 'Pause'}</button>
        {hud.gear && (
          <div className="stepper">
            <button onClick={() => void api.ride.shift(-1)} aria-label="Easier gear">
              −
            </button>
            <span>Gear {hud.gear.index + 1}</span>
            <button onClick={() => void api.ride.shift(1)} aria-label="Harder gear">
              +
            </button>
          </div>
        )}
      </div>
    </div>
  )
}
