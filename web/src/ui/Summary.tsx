import { useEffect, useRef, useState } from 'react'
import { errorText } from '../bridge'
import { fmtDist, fmtEle, fmtTime, speedUnit, speedValue } from '../core/fmt'
import { api, useBadges, useSettings } from '../state'
import { uploadOf, type ConnectorEntry, type RideSummary, type UploadRecord } from '../types'
import { ReportButton } from './Diagnostics'

/** One upload button per connector, with its outcome once sent. */
function UploadButton({ ride, entry, auto }: { ride: RideSummary; entry: ConnectorEntry; auto: boolean }): React.JSX.Element | null {
  const { info, status } = entry
  const [state, setState] = useState<'idle' | 'busy' | 'error'>('idle')
  const [record, setRecord] = useState<UploadRecord | undefined>(uploadOf(ride, info.id))
  const [error, setError] = useState('')
  const started = useRef(false)

  const upload = async (): Promise<void> => {
    setState('busy')
    try {
      // The backend also records the upload against the ride, so it isn't sent twice.
      const result = await api.connectors.upload(
        info.id,
        ride.id,
        ride.filePath,
        `${ride.routeName} — VeloPopuli`,
        `Virtual ride of ${ride.routeName}${ride.completed ? '' : ' (partial)'} on VeloPopuli.`
      )
      setRecord(result)
      setState('idle')
    } catch (err) {
      setError(errorText(err))
      setState('error')
    }
  }

  useEffect(() => {
    if (auto && status.connected && !record && !started.current) {
      started.current = true
      void upload()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  if (record) {
    if (record.url) {
      const url = record.url
      return <button onClick={() => void api.openExternal(url)}>View on {info.name} ↗</button>
    }
    return (
      <span className="muted small">
        Sent to {info.name}
        {record.message ? ` — ${record.message}` : ''}
      </span>
    )
  }
  if (!status.connected) return null
  return (
    <>
      <button className={`upload ${info.id}`} disabled={state === 'busy'} onClick={() => void upload()}>
        {state === 'busy' ? `Uploading to ${info.name}…` : state === 'error' ? `Retry ${info.name} upload` : `Upload to ${info.name}`}
      </button>
      {state === 'error' && <span className="error inline">{error}</span>}
    </>
  )
}

/** Upload buttons shared by the summary and history screens. `auto` sends the ride to the
 *  connectors chosen in Settings as soon as it is shown. */
export function UploadButtons({ ride, auto }: { ride: RideSummary; auto?: boolean }): React.JSX.Element | null {
  const { settings } = useSettings()
  const [entries, setEntries] = useState<ConnectorEntry[] | null>(null)

  useEffect(() => {
    void api.connectors.list().then(setEntries)
  }, [])

  if (!entries) return null
  if (!entries.some((e) => e.status.connected) && !entries.some((e) => uploadOf(ride, e.info.id))) {
    return <span className="muted small">Connect {entries.map((e) => e.info.name).join(' or ')} in Settings to upload rides.</span>
  }
  return (
    <>
      {entries.map((entry) => (
        <UploadButton key={entry.info.id} ride={ride} entry={entry} auto={Boolean(auto) && settings.autoUpload.includes(entry.info.id)} />
      ))}
    </>
  )
}

export function Summary({ ride, onDone }: { ride: RideSummary; onDone: () => void }): React.JSX.Element {
  const { settings, update } = useSettings()
  const u = settings.units
  const badges = useBadges()
  const est = ride.ftpEstimate
  const stat = (label: string, value: string): React.JSX.Element => (
    <div className="stat">
      <div className="stat-value">{value}</div>
      <div className="stat-label">{label}</div>
    </div>
  )
  return (
    <div className="summary">
      <h1>{ride.completed ? 'Route complete!' : 'Ride saved'}</h1>
      <p className="muted">{ride.routeName}</p>
      {ride.isPb && <div className="pb-banner">★ New personal best — {fmtTime(ride.durationS)}. This ride is now your ghost.</div>}
      <div className="stats">
        {stat('Distance', fmtDist(ride.distanceM, u, 2))}
        {stat('Time', fmtTime(ride.durationS))}
        {stat('Climbed', fmtEle(ride.ascentM, u))}
        {stat(`Avg ${speedUnit(u)}`, speedValue(ride.avgSpeed, u).toFixed(1))}
        {stat('Avg power', `${Math.round(ride.avgPower)} W`)}
        {stat('Normalised', `${Math.round(ride.np)} W`)}
        {stat('Max power', `${Math.round(ride.maxPower)} W`)}
        {stat('Energy', `${Math.round(ride.kj)} kJ`)}
        {ride.avgHr > 0 && stat('Avg HR', `${Math.round(ride.avgHr)} bpm`)}
        {ride.maxHr > 0 && stat('Max HR', `${Math.round(ride.maxHr)} bpm`)}
        {ride.avgCadence > 0 && stat('Cadence', `${Math.round(ride.avgCadence)} rpm`)}
        {stat('TSS', ride.tss.toFixed(0))}
      </div>
      {ride.workout && <div className="pb-banner dim">Workout: {ride.workout}</div>}
      {ride.segmentPrs.length > 0 && (
        <div className="pb-banner">
          ⏱ New segment best{ride.segmentPrs.length > 1 ? 's' : ''}: {ride.segmentPrs.join(', ')}
        </div>
      )}
      {est && (
        <div className="pb-banner">
          This ride suggests an FTP of <b>{est.watts} W</b> ({est.method}).{' '}
          {settings.rider.ftp !== est.watts ? (
            <button className="small" onClick={() => update((s) => ({ ...s, rider: { ...s.rider, ftp: est.watts } }))}>
              Set FTP to {est.watts} W (now {settings.rider.ftp} W)
            </button>
          ) : (
            <span>Saved.</span>
          )}
        </div>
      )}
      <div className="card reward">
        <div className="xp">+{ride.xp} XP</div>
        {ride.newBadges.length > 0 && (
          <div className="badges">
            {ride.newBadges.map((id) => {
              const badge = badges.find((b) => b.id === id)
              return (
                <div className="badge earned" key={id}>
                  <b>{badge?.name ?? id}</b>
                  <span>{badge?.description}</span>
                </div>
              )
            })}
          </div>
        )}
      </div>
      <div className="row wrap">
        <button className="primary" onClick={onDone}>
          Done
        </button>
        <UploadButtons ride={ride} auto />
        <button className="ghost" onClick={() => void api.revealFile(ride.filePath)}>
          Show .FIT file
        </button>
        <ReportButton prompt={`Ride on ${ride.routeName}: `} />
      </div>
    </div>
  )
}
