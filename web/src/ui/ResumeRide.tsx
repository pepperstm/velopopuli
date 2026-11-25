import { useState } from 'react'
import { errorText } from '../bridge'
import { fmtDist, fmtTime } from '../core/fmt'
import { api, useSettings } from '../state'
import type { RideCheckpoint, RideRequest, RideSummary } from '../types'

/** How long ago, in words. */
function ago(ms: number): string {
  const minutes = Math.max(0, Math.round((Date.now() - ms) / 60_000))
  if (minutes < 2) return 'just now'
  if (minutes < 90) return `${minutes} minutes ago`
  const hours = Math.round(minutes / 60)
  if (hours < 36) return `${hours} hours ago`
  return `${Math.round(hours / 24)} days ago`
}

/** Shown on launch when the last ride was cut short: carry on, save what was ridden, or discard it. */
export function ResumeRide({
  checkpoint,
  onCarryOn,
  onSaved,
  onClose
}: {
  checkpoint: RideCheckpoint
  onCarryOn: (request: RideRequest) => void
  onSaved: (ride: RideSummary) => void
  onClose: () => void
}): React.JSX.Element {
  const { settings } = useSettings()
  const [busy, setBusy] = useState('')
  const [error, setError] = useState('')
  const [discarding, setDiscarding] = useState(false)
  const { routeName, durationS, distanceM, savedAt, canCarryOn } = checkpoint

  const save = async (): Promise<void> => {
    setBusy('save')
    setError('')
    try {
      const ride = await api.ride.checkpointSave()
      if (ride) onSaved(ride)
      else onClose()
    } catch (err) {
      setError(`Could not save it: ${errorText(err)}`)
      setBusy('')
    }
  }

  const discard = async (): Promise<void> => {
    setBusy('discard')
    await api.ride.checkpointDiscard().catch(() => undefined)
    onClose()
  }

  return (
    <div className="modal-backdrop">
      <div className="modal resume-ride">
        <h2>{canCarryOn ? `Carry on your ride on ${routeName}?` : `Your ride on ${routeName} wasn't saved`}</h2>
        <p>
          VeloPopuli closed before the ride ended, {ago(savedAt)}.{' '}
          <strong>
            {fmtTime(durationS)}, {fmtDist(distanceM, settings.units)}
          </strong>{' '}
          ridden so far.
        </p>
        {canCarryOn ? (
          <p className="muted small">Carry on from where you were, and the clock picks up where it stopped. The time away is left out, as if you'd paused.</p>
        ) : (
          <p className="muted small">It's too long ago to carry on, but you can still save what you rode.</p>
        )}
        {error && <div className="error">{error}</div>}
        {discarding ? (
          <div className="row wrap">
            <span>Throw this ride away?</span>
            <button className="danger" disabled={busy !== ''} onClick={() => void discard()}>
              Discard it
            </button>
            <button onClick={() => setDiscarding(false)}>Keep it</button>
          </div>
        ) : (
          <div className="row wrap">
            {canCarryOn && (
              <button className="primary" disabled={busy !== ''} onClick={() => onCarryOn({ ...checkpoint.request, resume: true })}>
                Carry on
              </button>
            )}
            <button className={canCarryOn ? '' : 'primary'} disabled={busy !== ''} onClick={() => void save()}>
              {busy === 'save' ? 'Saving…' : 'Save what you rode'}
            </button>
            <button className="ghost" disabled={busy !== ''} onClick={() => setDiscarding(true)}>
              Discard
            </button>
          </div>
        )}
      </div>
    </div>
  )
}
