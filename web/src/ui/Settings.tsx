import { useEffect, useState } from 'react'
import { errorText } from '../bridge'
import { api, useSettings } from '../state'
import type { GearPreview, GearSettings, Settings, StravaStatus } from '../types'
import { ReportButton } from './Diagnostics'

function Num(props: {
  label: string
  value: number
  onChange: (v: number) => void
  min: number
  max: number
  step?: number
  suffix?: string
  hint?: string
}): React.JSX.Element {
  // Keep the raw text so half-typed values ("7", "") aren't clamped mid-edit.
  const [text, setText] = useState(String(props.value))
  useEffect(() => setText(String(props.value)), [props.value])
  const commit = (): void => {
    const v = parseFloat(text)
    if (isFinite(v)) props.onChange(Math.max(props.min, Math.min(props.max, v)))
    else setText(String(props.value))
  }
  return (
    <label className="field">
      <span>{props.label}</span>
      <div className="input-row">
        <input
          type="number"
          value={text}
          min={props.min}
          max={props.max}
          step={props.step ?? 1}
          onChange={(e) => setText(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => e.key === 'Enter' && commit()}
        />
        {props.suffix && <em>{props.suffix}</em>}
      </div>
      {props.hint && <small>{props.hint}</small>}
    </label>
  )
}

function Toggle(props: { label: string; value: boolean; onChange: (v: boolean) => void; hint?: string }): React.JSX.Element {
  return (
    <label className="field toggle">
      <input type="checkbox" checked={props.value} onChange={(e) => props.onChange(e.target.checked)} />
      <span>{props.label}</span>
      {props.hint && <small>{props.hint}</small>}
    </label>
  )
}

const WHEELS: [string, number][] = [
  ['700 × 25c', 2105],
  ['700 × 28c', 2136],
  ['700 × 32c', 2155],
  ['700 × 38c', 2180],
  ['650b × 47', 2080],
  ['29 × 2.2"', 2300],
  ['27.5 × 2.2"', 2200]
]

/** Teeth from text like "50, 34" or "11 12 13". */
function teeth(text: string): number[] {
  return text
    .split(/[^0-9]+/)
    .map(Number)
    .filter((t) => t >= 6 && t <= 90)
}

/** A key name as the rider would recognise it. */
function keyName(key: string): string {
  return (
    ({ ' ': 'Space', ArrowRight: '→', ArrowLeft: '←', ArrowUp: '↑', ArrowDown: '↓', PageDown: 'Page Down', PageUp: 'Page Up' } as Record<string, string>)[
      key
    ] ?? key
  )
}

function KeyList(props: { label: string; keys: string[]; onChange: (keys: string[]) => void }): React.JSX.Element {
  const [listening, setListening] = useState(false)
  useEffect(() => {
    if (!listening) return
    const onKey = (e: KeyboardEvent): void => {
      e.preventDefault()
      e.stopPropagation()
      if (e.key !== 'Escape' && !props.keys.includes(e.key)) props.onChange([...props.keys, e.key])
      setListening(false)
    }
    window.addEventListener('keydown', onKey, { capture: true })
    return () => window.removeEventListener('keydown', onKey, { capture: true })
  }, [listening, props])
  return (
    <div className="field">
      <span>{props.label}</span>
      <div className="row wrap key-list">
        {props.keys.map((k) => (
          <button key={k} className="key-chip" title="Remove" onClick={() => props.onChange(props.keys.filter((x) => x !== k))}>
            {keyName(k)} ×
          </button>
        ))}
        <button className={listening ? 'primary' : 'ghost'} onClick={() => setListening(true)}>
          {listening ? 'Press a key or remote button…' : '+ Add'}
        </button>
      </div>
    </div>
  )
}

function GearsCard(): React.JSX.Element {
  const { settings, update } = useSettings()
  const g = settings.gears
  const set = (patch: Partial<GearSettings>): void => update((s) => ({ ...s, gears: { ...s.gears, ...patch } }))
  const [preview, setPreview] = useState<GearPreview | null>(null)
  const [rings, setRings] = useState(g.chainrings.join(', '))
  const [cogs, setCogs] = useState(g.cogs.join(', '))
  useEffect(() => {
    void api.gears.preview(g).then(setPreview)
  }, [g])
  const virtual = g.mode === 'virtual'
  const wheelKnown = WHEELS.some(([, mm]) => mm === g.wheelMm)
  const top = preview ? Math.max(...preview.gears.map(([, r]) => r), preview.realRatio) : 1

  return (
    <section className="card">
      <h2>Gears</h2>
      <div className="seg gear-mode">
        <button className={!virtual ? 'on' : ''} onClick={() => set({ mode: 'real' })}>
          My bike's gears
        </button>
        <button className={virtual ? 'on' : ''} onClick={() => set({ mode: 'virtual' })}>
          Virtual gears
        </button>
      </div>
      {!virtual ? (
        <p className="muted small">
          Shift your bike as you would on the road — there's nothing to set up. The trainer feels how fast the flywheel turns, so a harder gear means more force
          per pedal stroke, just as it should.
        </p>
      ) : (
        <>
          <p className="muted small">
            Leave the bike in one gear and shift with keys, a Bluetooth remote or the buttons on the ride screen. VeloPopuli changes the trainer's resistance so
            each virtual gear feels like the real thing. For trainers with a single cog, or to spare your drivetrain.
          </p>
          <div className="fields">
            <label className="field">
              <span>Gear set</span>
              <select value={g.preset} onChange={(e) => set({ preset: e.target.value })}>
                {preview?.presets.map(([id, name]) => (
                  <option key={id} value={id}>
                    {name}
                  </option>
                ))}
                <option value="custom">My own chainrings and cassette</option>
              </select>
            </label>
            {g.preset === 'custom' && (
              <>
                <label className="field">
                  <span>Chainrings (teeth)</span>
                  <input value={rings} placeholder="50, 34" onChange={(e) => setRings(e.target.value)} onBlur={() => set({ chainrings: teeth(rings) })} />
                </label>
                <label className="field">
                  <span>Cassette (teeth)</span>
                  <input
                    value={cogs}
                    placeholder="11, 12, 13, 14, 15, 17, 19, 21, 24, 27, 30, 34"
                    onChange={(e) => setCogs(e.target.value)}
                    onBlur={() => set({ cogs: teeth(cogs) })}
                  />
                </label>
              </>
            )}
            <Num label="Bike stays in: chainring" value={g.realChainring} min={20} max={60} suffix="teeth" onChange={(v) => set({ realChainring: v })} />
            <Num
              label="…and cog"
              value={g.realCog}
              min={10}
              max={52}
              suffix="teeth"
              hint="The gear your bike is actually in on the trainer"
              onChange={(v) => set({ realCog: v })}
            />
            <label className="field">
              <span>Wheel</span>
              <select value={wheelKnown ? g.wheelMm : 0} onChange={(e) => Number(e.target.value) && set({ wheelMm: Number(e.target.value) })}>
                {WHEELS.map(([name, mm]) => (
                  <option key={mm} value={mm}>
                    {name}
                  </option>
                ))}
                {!wheelKnown && <option value={0}>{g.wheelMm} mm</option>}
              </select>
              <small>Used to show your cadence when you have no cadence sensor</small>
            </label>
            <label className="field">
              <span>Feel</span>
              <select value={g.feel} onChange={(e) => set({ feel: e.target.value as GearSettings['feel'] })}>
                <option value="realistic">Realistic — like real gears</option>
                <option value="steps">Simple steps — a fixed amount per gear</option>
              </select>
            </label>
            {g.feel === 'steps' && (
              <Num
                label="Each gear adds"
                value={Math.round(g.step * 1000) / 10}
                min={0.2}
                max={3}
                step={0.1}
                suffix="% gradient"
                onChange={(v) => set({ step: v / 100 })}
              />
            )}
            <label className="field">
              <span>Start each ride in</span>
              <select value={g.startGear} onChange={(e) => set({ startGear: Number(e.target.value) })}>
                <option value={0}>The gear closest to your bike's</option>
                {preview?.gears.map(([label], i) => (
                  <option key={i} value={i + 1}>
                    Gear {i + 1} ({label})
                  </option>
                ))}
              </select>
            </label>
          </div>
          {preview && preview.gears.length > 0 && (
            <div className="gear-preview" aria-label="Your gears, easiest to hardest">
              {preview.gears.map(([label, ratio], i) => (
                <div key={i} className={`gear-bar${i === preview.start ? ' start' : ''}`} title={`Gear ${i + 1}: ${label} (${ratio.toFixed(2)})`}>
                  <div style={{ height: `${(ratio / top) * 100}%` }} />
                </div>
              ))}
            </div>
          )}
          {preview && (
            <p className="muted small">
              {(() => {
                const named = !preview.gears[0]?.[0].startsWith('Gear ')
                const show = ([label, ratio]: [string, number]): string => (named ? label : `ratio ${ratio.toFixed(2)}`)
                const first = preview.gears[0]
                const last = preview.gears[preview.gears.length - 1]
                const start = preview.gears[preview.start]
                return first && last && start
                  ? `${preview.gears.length} gears, from ${show(first)} to ${show(last)}. Rides start in gear ${preview.start + 1}${named ? ` (${start[0]})` : ''}, which feels like your bike's ${g.realChainring}×${g.realCog}.`
                  : 'No gears — check the chainring and cassette teeth.'
              })()}
            </p>
          )}
          <div className="fields">
            <KeyList label="Harder gear" keys={g.keysHarder} onChange={(keys) => set({ keysHarder: keys })} />
            <KeyList label="Easier gear" keys={g.keysEasier} onChange={(keys) => set({ keysEasier: keys })} />
          </div>
          <p className="muted small">
            Most Bluetooth presentation clickers and media remotes send arrow or Page Up / Page Down keys — press <b>+ Add</b>, then the button, to use one as a
            shifter. Gears do nothing in ERG mode, where the trainer holds the power whatever gear you're in.
          </p>
        </>
      )}
    </section>
  )
}

function StravaCard(): React.JSX.Element {
  const { settings, update } = useSettings()
  const [status, setStatus] = useState<StravaStatus | null>(null)
  const [clientId, setClientId] = useState('')
  const [secret, setSecret] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    void api.strava.status().then((s) => {
      setStatus(s)
      setClientId(s.clientId)
    })
  }, [])

  const run = async (fn: () => Promise<StravaStatus>): Promise<void> => {
    setBusy(true)
    setError('')
    try {
      setStatus(await fn())
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section className="card">
      <h2>Strava</h2>
      {status?.connected ? (
        <>
          <p>
            Connected{status.athlete ? ` as ${status.athlete}` : ''}. Rides upload as <b>Virtual Rides</b> with power, heart rate and the route map.
          </p>
          <Toggle label="Upload every ride automatically" value={settings.stravaAutoUpload} onChange={(v) => update((s) => ({ ...s, stravaAutoUpload: v }))} />
          <button disabled={busy} onClick={() => void run(api.strava.disconnect)}>
            Disconnect
          </button>
        </>
      ) : (
        <>
          <p className="muted">
            Strava gives every app its own key, and an open-source app can't ship one — so you create a free personal key once (about a minute):
          </p>
          <ol className="muted steps">
            <li>
              Open{' '}
              <a href="#" onClick={() => void api.openExternal('https://www.strava.com/settings/api')}>
                strava.com/settings/api
              </a>{' '}
              and create an application (any name and website).
            </li>
            <li>
              Set <b>Authorization Callback Domain</b> to <code>localhost</code>.
            </li>
            <li>Paste the Client ID and Client Secret below, then connect.</li>
          </ol>
          <div className="fields">
            <label className="field">
              <span>Client ID</span>
              <input value={clientId} onChange={(e) => setClientId(e.target.value)} inputMode="numeric" />
            </label>
            <label className="field">
              <span>Client secret</span>
              <input
                type="password"
                value={secret}
                placeholder={status?.configured ? 'saved — leave blank to keep' : ''}
                onChange={(e) => setSecret(e.target.value)}
              />
            </label>
          </div>
          <button
            className="primary"
            disabled={busy || !clientId.trim() || (!secret.trim() && !status?.configured)}
            onClick={() =>
              void run(async () => {
                await api.strava.setApp(clientId, secret)
                setSecret('')
                return api.strava.connect()
              })
            }
          >
            {busy ? 'Waiting for Strava in your browser…' : 'Connect to Strava'}
          </button>
        </>
      )}
      {error && <div className="error">{error}</div>}
    </section>
  )
}

function FanCard(): React.JSX.Element {
  const { settings, update } = useSettings()
  const smart = settings.smart
  const set = (patch: Partial<Settings['smart']>): void => update((s) => ({ ...s, smart: { ...s.smart, ...patch } }))
  const [token, setToken] = useState('')
  const [status, setStatus] = useState('')
  const unit = smart.fanBy === 'power' ? 'W' : smart.fanBy === 'hr' ? 'bpm' : 'km/h'
  return (
    <section className="card">
      <h2>Fan &amp; lights</h2>
      <p className="muted">Switch a fan on (and faster) as you speed up or work harder, and colour a light by power zone.</p>
      <div className="fields">
        <label className="field">
          <span>Connection</span>
          <select value={smart.kind} onChange={(e) => set({ kind: e.target.value as Settings['smart']['kind'] })}>
            <option value="none">None</option>
            <option value="homeassistant">Home Assistant</option>
            <option value="shelly">Shelly plug</option>
            <option value="tasmota">Tasmota plug</option>
            <option value="webhook">Web address (webhook)</option>
          </select>
        </label>
        {smart.kind !== 'none' && (
          <label className="field">
            <span>{smart.kind === 'homeassistant' ? 'Home Assistant address' : smart.kind === 'webhook' ? 'Address to call' : "Plug's IP address"}</span>
            <input
              value={smart.url}
              onChange={(e) => set({ url: e.target.value.trim() })}
              placeholder={
                smart.kind === 'homeassistant'
                  ? 'http://homeassistant.local:8123'
                  : smart.kind === 'webhook'
                    ? 'http://…/fan?level={level}&zone={zone}'
                    : '192.168.1.40'
              }
            />
            {smart.kind === 'webhook' && (
              <small>
                {'{level}'} (0–100), {'{zone}'} (1–7), {'{power}'}, {'{speed}'} and {'{hr}'} are filled in.
              </small>
            )}
          </label>
        )}
        {smart.kind === 'homeassistant' && (
          <>
            <label className="field">
              <span>Long-lived access token</span>
              <input
                type="password"
                value={token}
                placeholder="kept in the system keychain"
                onChange={(e) => setToken(e.target.value)}
                onBlur={() => token && void api.smart.setToken(token).then(() => (setToken(''), setStatus('Token saved.')))}
              />
            </label>
            <label className="field">
              <span>Fan entity</span>
              <input value={smart.fanEntity} onChange={(e) => set({ fanEntity: e.target.value.trim() })} placeholder="fan.desk_fan or switch.fan_plug" />
            </label>
            <label className="field">
              <span>Light entity</span>
              <input value={smart.lightEntity} onChange={(e) => set({ lightEntity: e.target.value.trim() })} placeholder="light.pain_cave" />
            </label>
          </>
        )}
        {smart.kind !== 'none' && (
          <>
            <label className="field">
              <span>Fan follows</span>
              <select value={smart.fanBy} onChange={(e) => set({ fanBy: e.target.value as Settings['smart']['fanBy'] })}>
                <option value="speed">Speed</option>
                <option value="power">Power</option>
                <option value="hr">Heart rate</option>
              </select>
            </label>
            <Num label="Fan on from" value={smart.fanFrom} min={0} max={1000} suffix={unit} onChange={(v) => set({ fanFrom: v })} />
            <Num label="Full speed at" value={smart.fanFull} min={0} max={1000} suffix={unit} onChange={(v) => set({ fanFull: v })} />
          </>
        )}
      </div>
      {smart.kind !== 'none' && (
        <button
          onClick={() => {
            setStatus('Fan on full for five seconds…')
            void api.smart.test().then(
              () => setStatus('Test sent. Did the fan come on and go off again?'),
              (err) => setStatus(errorText(err))
            )
          }}
        >
          Test
        </button>
      )}
      {status && <div className="muted small">{status}</div>}
    </section>
  )
}

export function SettingsScreen(): React.JSX.Element {
  const { settings, update } = useSettings()
  const rider = (patch: Partial<Settings['rider']>): void => update((s) => ({ ...s, rider: { ...s.rider, ...patch } }))
  const physics = (patch: Partial<Settings['physics']>): void => update((s) => ({ ...s, physics: { ...s.physics, ...patch } }))
  const trainer = (patch: Partial<Settings['trainer']>): void => update((s) => ({ ...s, trainer: { ...s.trainer, ...patch } }))
  const look = (patch: Partial<Settings['look']>): void => update((s) => ({ ...s, look: { ...s.look, ...patch } }))
  const goal = (patch: Partial<Settings['goal']>): void => update((s) => ({ ...s, goal: { ...s.goal, ...patch } }))

  return (
    <div className="settings">
      <div className="page-head">
        <h1>Settings</h1>
      </div>

      <section className="card">
        <h2>Rider</h2>
        <div className="fields">
          <label className="field">
            <span>Name</span>
            <input value={settings.rider.name} onChange={(e) => rider({ name: e.target.value })} />
          </label>
          <Num label="Weight" value={settings.rider.weightKg} min={30} max={200} step={0.1} suffix="kg" onChange={(v) => rider({ weightKg: v })} />
          <Num label="Bike weight" value={settings.rider.bikeKg} min={4} max={30} step={0.1} suffix="kg" onChange={(v) => rider({ bikeKg: v })} />
          <Num
            label="FTP"
            value={settings.rider.ftp}
            min={50}
            max={600}
            suffix="W"
            hint="Sets power-zone colours and TSS."
            onChange={(v) => rider({ ftp: v })}
          />
          <Num label="Max heart rate" value={settings.rider.maxHr} min={100} max={230} suffix="bpm" onChange={(v) => rider({ maxHr: v })} />
          <label className="field">
            <span>Units</span>
            <select value={settings.units} onChange={(e) => update((s) => ({ ...s, units: e.target.value as Settings['units'] }))}>
              <option value="metric">Metric (km, m)</option>
              <option value="imperial">Imperial (mi, ft)</option>
            </select>
          </label>
        </div>
      </section>

      <section className="card">
        <h2>Trainer &amp; physics</h2>
        <div className="fields">
          <Num
            label="Trainer difficulty"
            value={Math.round(settings.trainer.difficulty * 100)}
            min={0}
            max={100}
            step={5}
            suffix="%"
            hint="How much of the real gradient your trainer applies. Your virtual speed always uses the full gradient."
            onChange={(v) => trainer({ difficulty: v / 100 })}
          />
          <Num
            label="Steepest gradient"
            value={Math.round(settings.trainer.maxGrade * 100)}
            min={0}
            max={30}
            suffix="%"
            hint="The trainer never goes steeper than this (after difficulty). 0 means no limit."
            onChange={(v) => trainer({ maxGrade: v / 100 })}
          />
          <Num
            label="Downhill feel"
            value={Math.round(settings.trainer.downhill * 100)}
            min={0}
            max={100}
            step={5}
            suffix="%"
            hint="Share of descents passed on, so you don't spin out on the way down."
            onChange={(v) => trainer({ downhill: v / 100 })}
          />
          <Num
            label="Drag area (CdA)"
            value={settings.physics.cda}
            min={0.15}
            max={0.7}
            step={0.01}
            suffix="m²"
            hint="≈0.32 on the hoods, 0.25 in an aero tuck."
            onChange={(v) => physics({ cda: v })}
          />
          <Num
            label="Rolling resistance (Crr)"
            value={settings.physics.crr}
            min={0.002}
            max={0.02}
            step={0.0005}
            hint="Your tyres on smooth tarmac: 0.004 road tyres, 0.005 wider or gravel tyres."
            onChange={(v) => physics({ crr: v })}
          />
          <Num
            label="Descent speed limit"
            value={settings.physics.maxSpeedKph}
            min={30}
            max={120}
            suffix="km/h"
            hint="Stands in for braking into corners."
            onChange={(v) => physics({ maxSpeedKph: v })}
          />
        </div>
        <Toggle
          label="Feel the road surface"
          hint="Concrete, gravel, cobbles and earth roll heavier than tarmac — you go slower on screen and the trainer gets harder. Not in ERG."
          value={settings.physics.roadSurfaces}
          onChange={(v) => physics({ roadSurfaces: v })}
        />
        <Toggle
          label="Estimate power from a speed sensor"
          hint="For basic (non-smart) trainers with no power meter. Uses a typical fluid-trainer curve, so treat the watts as approximate."
          value={settings.trainer.virtualPower}
          onChange={(v) => trainer({ virtualPower: v })}
        />
        <Toggle
          label="Hold workout targets (ERG)"
          hint="During a structured workout the trainer sets the resistance for each target. Off: targets are shown, and the gradient sets the resistance."
          value={settings.trainer.workoutErg}
          onChange={(v) => trainer({ workoutErg: v })}
        />
      </section>

      <GearsCard />

      <section className="card">
        <h2>Virtual world</h2>
        <Toggle
          label="Real terrain around the route"
          hint="Downloads open elevation data for the area so the mountains match the real place. Off (or offline) uses generated hills."
          value={settings.realTerrain}
          onChange={(v) => update((s) => ({ ...s, realTerrain: v }))}
        />
        <Toggle
          label="Buildings from OpenStreetMap"
          hint="Downloads the building outlines along each route once (then kept on this computer) and stands them in the landscape."
          value={settings.buildings}
          onChange={(v) => update((s) => ({ ...s, buildings: v }))}
        />
        <Toggle
          label="Forests, water, roads, bridges and tunnels from OpenStreetMap"
          hint="Downloads the map of each route's surroundings once: woods are planted where they really are, lakes and rivers are filled, the other roads, paths and railways are laid out, and your road is carried over bridges and through tunnels where the real one is."
          value={settings.scenery}
          onChange={(v) => update((s) => ({ ...s, scenery: v }))}
        />
        <Toggle
          label="Satellite and aerial imagery on the ground"
          hint="Covers the ground near the route with real imagery, downloaded as you ride and kept for next time: national aerial photos in Switzerland, France, Spain, the Netherlands and the US, and Sentinel-2 satellite imagery everywhere else."
          value={settings.look.imagery}
          onChange={(v) => look({ imagery: v })}
        />
        {settings.look.imagery && (
          <label className="field">
            <span>Your own imagery provider (optional)</span>
            <input
              type="password"
              value={settings.look.imageryUrl}
              placeholder="https://…/{z}/{x}/{y}.jpg?access_token=…"
              onChange={(e) => look({ imageryUrl: e.target.value.trim() })}
            />
            <small>
              A tile URL with {'{z}'}, {'{x}'} and {'{y}'}, including your key, from a provider you have an account with (Mapbox, MapTiler and others). It is
              used under your account and its terms instead of the open sources.
            </small>
          </label>
        )}
        <div className="fields">
          <label className="field">
            <span>Time of day</span>
            <select value={settings.look.timeOfDay} onChange={(e) => look({ timeOfDay: e.target.value as Settings['look']['timeOfDay'] })}>
              <option value="dawn">Dawn</option>
              <option value="day">Day</option>
              <option value="evening">Evening</option>
              <option value="dusk">Dusk</option>
              <option value="night">Night</option>
              <option value="clock">Same as now</option>
            </select>
          </label>
          <label className="field">
            <span>Weather</span>
            <select value={settings.look.weather} onChange={(e) => look({ weather: e.target.value as Settings['look']['weather'] })}>
              <option value="clear">Clear</option>
              <option value="cloudy">Cloudy</option>
              <option value="rain">Rain</option>
              <option value="fog">Fog</option>
              <option value="snow">Snow</option>
              <option value="live">The weather there right now</option>
            </select>
          </label>
        </div>
        <Toggle
          label="Landmarks, place names and points of interest"
          hint="Wind turbines, lighthouses, masts, chimneys, water towers, windmills, castles and monuments on the skyline; village signs, summit boards with the climb's height, cafés, viewpoints and more along the route. All from OpenStreetMap."
          value={settings.look.landmarks}
          onChange={(v) => look({ landmarks: v })}
        />
        <Toggle
          label="Street-level photos from Mapillary"
          hint="Shows real photos taken along the road, where Mapillary has them. Needs a free client token from mapillary.com/dashboard/developers."
          value={settings.look.photos}
          onChange={(v) => look({ photos: v })}
        />
        {settings.look.photos && (
          <label className="field">
            <span>Mapillary client token</span>
            <input type="password" value={settings.look.mapillaryToken} placeholder="MLY|…" onChange={(e) => look({ mapillaryToken: e.target.value.trim() })} />
          </label>
        )}
        <Toggle
          label="Native renderer (experimental, macOS)"
          hint="Draws the 3D world straight on the graphics card (Metal) instead of inside the app's web page, for lower overhead."
          value={settings.nativeRenderer}
          onChange={(v) => update((s) => ({ ...s, nativeRenderer: v }))}
        />
        <Toggle
          label="Pace riders"
          hint="Computer riders holding a steady power to chase or drop."
          value={settings.bots.enabled}
          onChange={(v) => update((s) => ({ ...s, bots: { ...s.bots, enabled: v } }))}
        />
        {settings.bots.enabled && (
          <div className="fields">
            {settings.bots.wkg.map((w, i) => (
              <Num
                key={i}
                label={`Pace rider ${i + 1}`}
                value={w}
                min={0.5}
                max={6}
                step={0.1}
                suffix="W/kg"
                onChange={(v) => update((s) => ({ ...s, bots: { ...s.bots, wkg: s.bots.wkg.map((x, k) => (k === i ? v : x)) } }))}
              />
            ))}
          </div>
        )}
      </section>

      <section className="card">
        <h2>Season goal</h2>
        <div className="fields">
          <label className="field">
            <span>Name</span>
            <input value={settings.goal.name} onChange={(e) => goal({ name: e.target.value })} />
          </label>
          <Num label="Distance" value={settings.goal.distanceKm} min={0} max={50000} step={50} suffix="km" onChange={(v) => goal({ distanceKm: v })} />
          <label className="field">
            <span>Counting from</span>
            <input type="date" value={settings.goal.from} onChange={(e) => goal({ from: e.target.value })} />
            <small>Empty: 1 September.</small>
          </label>
          <label className="field">
            <span>Deadline</span>
            <input type="date" value={settings.goal.deadline} onChange={(e) => goal({ deadline: e.target.value })} />
            <small>Empty: the first day of spring (20 March).</small>
          </label>
        </div>
      </section>

      <FanCard />

      <section className="card">
        <h2>Group rides</h2>
        <p className="muted">
          Riding together goes through a small relay server that passes everyone's position around. Anyone can run one: it is the <code>velopopuli-relay</code>{' '}
          program in this app's source (<code>cargo run --release -p velopopuli-relay</code>), and needs port 47800 open. Everyone in the group enters the same
          address here.
        </p>
        <label className="field">
          <span>Relay address</span>
          <input
            value={settings.group.relayUrl}
            onChange={(e) => update((s) => ({ ...s, group: { relayUrl: e.target.value.trim() } }))}
            placeholder="ws://relay.example.org:47800"
          />
        </label>
      </section>

      <StravaCard />

      <section className="card">
        <h2>Diagnostics</h2>
        <p className="muted small">
          If something misbehaves (a trainer dropping out, power reading zero, resistance not changing), save a report and attach it to your bug report. It
          covers the last couple of days: connections, drop-outs, the data each sensor sent and what was sent to the trainer.
        </p>
        <Toggle
          label="Detailed diagnostics"
          hint="Also log every message the sensors send (about 3 MB an hour of riding; the last 12 app runs are kept)."
          value={settings.diagnostics}
          onChange={(v) => update((s) => ({ ...s, diagnostics: v }))}
        />
        <div className="row">
          <ReportButton label="Save diagnostics report…" className="" />
          <button className="ghost" onClick={() => void api.diag.openFolder()}>
            Open logs folder
          </button>
        </div>
      </section>

      <section className="card">
        <h2>Ride files</h2>
        <p className="muted">
          Every ride is saved as a .FIT file. Optionally copy each one to a second folder — for example iCloud Drive or Dropbox, so it is on your phone straight
          away.
        </p>
        <div className="row wrap">
          <code>{settings.exportDir || 'No extra export folder'}</code>
          <button onClick={() => void api.chooseDir().then((dir) => dir && update((s) => ({ ...s, exportDir: dir })))}>Choose folder…</button>
          {settings.exportDir && <button onClick={() => update((s) => ({ ...s, exportDir: '' }))}>Clear</button>}
        </div>
      </section>
    </div>
  )
}
