import * as THREE from 'three'
import type { TimeOfDay, Weather } from '../types'

/** Lighting and sky for one time of day. */
interface Light {
  zenith: number
  horizon: number
  hemiSky: number
  hemiGround: number
  hemi: number
  sun: number
  sunColor: number
  /** Direction towards the sun (or moon). */
  sunDir: [number, number, number]
  /** A lamp on the rider, for riding in the dark. */
  lamp: number
}

const TIMES: Record<Exclude<TimeOfDay, 'clock'>, Light> = {
  dawn: {
    zenith: 0x5a78b8,
    horizon: 0xf3b38a,
    hemiSky: 0xd8c8e8,
    hemiGround: 0x5a5a48,
    hemi: 1.3,
    sun: 1.8,
    sunColor: 0xffb27a,
    sunDir: [0.9, 0.18, 0.2],
    lamp: 0
  },
  day: {
    zenith: 0x3f86d6,
    horizon: 0xbfd6e8,
    hemiSky: 0xe6f1ff,
    hemiGround: 0x6b7a4f,
    hemi: 2.0,
    sun: 2.4,
    sunColor: 0xfff1d6,
    sunDir: [0.5, 1, 0.35],
    lamp: 0
  },
  evening: {
    zenith: 0x4c78b8,
    horizon: 0xf0cf9a,
    hemiSky: 0xf0e2c8,
    hemiGround: 0x6a6448,
    hemi: 1.6,
    sun: 2.2,
    sunColor: 0xffc88a,
    sunDir: [-0.8, 0.35, 0.3],
    lamp: 0
  },
  dusk: {
    zenith: 0x23315e,
    horizon: 0xc8705a,
    hemiSky: 0x8a7fa8,
    hemiGround: 0x3a3630,
    hemi: 0.8,
    sun: 0.9,
    sunColor: 0xff8a5a,
    sunDir: [-0.9, 0.08, 0.25],
    lamp: 1.5
  },
  night: {
    zenith: 0x060a18,
    horizon: 0x1a2236,
    hemiSky: 0x40507a,
    hemiGround: 0x101418,
    hemi: 0.45,
    sun: 0.35,
    sunColor: 0xa8c0ff,
    sunDir: [0.3, 0.8, -0.4],
    lamp: 3.0
  }
}

/** Time of day from the clock. */
export function timeFromHour(hour: number): Exclude<TimeOfDay, 'clock'> {
  if (hour >= 5.5 && hour < 8) return 'dawn'
  if (hour >= 8 && hour < 17) return 'day'
  if (hour >= 17 && hour < 19) return 'evening'
  if (hour >= 19 && hour < 21) return 'dusk'
  return 'night'
}

/** WMO weather code (as Open-Meteo reports it) to the weather drawn. */
export function weatherFromCode(code: number): Exclude<Weather, 'live'> {
  if (code === 45 || code === 48) return 'fog'
  if ((code >= 71 && code <= 77) || code === 85 || code === 86) return 'snow'
  if ((code >= 51 && code <= 67) || (code >= 80 && code <= 82) || code >= 95) return 'rain'
  if (code >= 2) return 'cloudy'
  return 'clear'
}

export interface Look {
  zenith: THREE.Color
  horizon: THREE.Color
  fog: { color: THREE.Color; near: number; far: number }
  hemi: { sky: THREE.Color; ground: THREE.Color; intensity: number }
  sun: { color: THREE.Color; intensity: number; dir: THREE.Vector3 }
  lamp: number
  precipitation: 'none' | 'rain' | 'snow'
  /** How hard it is raining or snowing: about 0.25 for drizzle, 1 steady, 1.6 a downpour. */
  intensity: number
  /** How wet the roads are, 0–1. */
  wet: number
  /** How much snow lies on the ground, 0–1. */
  snow: number
  /** The wind at 10 m, if known: m/s, and degrees clockwise from north that it blows from. */
  wind: { speed: number; from: number } | null
}

/** The weather at the route now, as the app reads it from Open-Meteo. */
export interface Conditions {
  code: number
  temperature: number
  /** 0–1. */
  cloudCover: number
  /** mm an hour. */
  rain: number
  /** cm an hour. */
  snowfall: number
  /** Metres. */
  visibility: number
  /** m/s at 10 m. */
  windSpeed: number
  windGusts: number
  /** Degrees clockwise from north. */
  windFrom: number
  /** Metres. */
  snowDepth: number
  wet: boolean
}

/** The real weather, drawn: cloud, haze, rain or snow and their strength, wet roads, lying snow, wind. */
export function lookFromConditions(time: Exclude<TimeOfDay, 'clock'>, c: Conditions): Look {
  const kind = weatherFromCode(c.code)
  const base = look(time, kind === 'fog' ? 'cloudy' : kind)
  const t = TIMES[time]
  const falling = c.snowfall > 0 || kind === 'snow' ? 'snow' : c.rain > 0 || kind === 'rain' ? 'rain' : 'none'
  const overcast = Math.min(0.9, c.cloudCover * 0.75 + (falling !== 'none' ? 0.15 : 0))
  const grey = new THREE.Color(time === 'night' ? 0x14181f : time === 'dusk' ? 0x4a4a52 : 0x9aa3ad)
  const horizon = new THREE.Color(t.horizon).lerp(grey.clone().offsetHSL(0, 0, 0.06), overcast)
  // Fog thins out to nothing at about the reported visibility.
  const far = Math.max(120, Math.min(8500, c.visibility))
  const rate = falling === 'snow' ? c.snowfall * 4 : c.rain
  return {
    ...base,
    zenith: new THREE.Color(t.zenith).lerp(grey, overcast),
    horizon,
    fog: { color: horizon.clone(), near: Math.max(5, far * 0.04), far },
    sun: { ...base.sun, intensity: t.sun * (1 - 0.7 * c.cloudCover) },
    lamp: t.lamp + (far < 1000 || rate > 2 ? 0.6 : 0),
    precipitation: falling,
    intensity: falling === 'none' ? 0 : Math.min(1.6, Math.max(0.25, 0.25 + Math.log10(1 + rate) * 0.9)),
    wet: c.wet || falling === 'rain' ? 1 : 0,
    snow: Math.min(1, c.snowDepth / 0.05),
    wind: { speed: c.windSpeed, from: c.windFrom }
  }
}

/** The wind as it moves the air near the ground, in the scene's x (east) and z (south), m/s. */
export function windVector(wind: Look['wind']): { x: number; z: number } {
  if (!wind) return { x: 0, z: 0 }
  const a = (wind.from * Math.PI) / 180
  const near = wind.speed * 0.6
  return { x: -Math.sin(a) * near, z: Math.cos(a) * near }
}

/** Combines a time of day with the weather into colours, light levels and haze. */
export function look(time: Exclude<TimeOfDay, 'clock'>, weather: Exclude<Weather, 'live'>): Look {
  const t = TIMES[time]
  const grey = new THREE.Color(time === 'night' ? 0x14181f : time === 'dusk' ? 0x4a4a52 : 0x9aa3ad)
  const overcast = weather === 'clear' ? 0 : weather === 'cloudy' ? 0.65 : weather === 'fog' ? 0.85 : 0.8
  const zenith = new THREE.Color(t.zenith).lerp(grey, overcast)
  const horizon = new THREE.Color(t.horizon).lerp(grey.clone().offsetHSL(0, 0, 0.06), overcast)
  const sunScale = weather === 'clear' ? 1 : weather === 'cloudy' ? 0.45 : 0.3
  const fog =
    weather === 'fog'
      ? { near: 10, far: 230 }
      : weather === 'rain' || weather === 'snow'
        ? { near: 60, far: 2200 }
        : weather === 'cloudy'
          ? { near: 200, far: 6000 }
          : { near: 300, far: 8500 }
  return {
    zenith,
    horizon,
    fog: { color: horizon.clone(), ...fog },
    hemi: { sky: new THREE.Color(t.hemiSky), ground: new THREE.Color(t.hemiGround), intensity: t.hemi * (weather === 'clear' ? 1 : 1.1) },
    sun: { color: new THREE.Color(t.sunColor), intensity: t.sun * sunScale, dir: new THREE.Vector3(...t.sunDir).normalize() },
    lamp: t.lamp + (weather === 'fog' || weather === 'rain' ? 0.6 : 0),
    precipitation: weather === 'rain' ? 'rain' : weather === 'snow' ? 'snow' : 'none',
    intensity: weather === 'rain' || weather === 'snow' ? 0.7 : 0,
    wet: weather === 'rain' ? 1 : 0,
    snow: weather === 'snow' ? 0.8 : 0,
    wind: null
  }
}

/** Falling rain or snow in a box that travels with the camera. */
export class Precipitation {
  readonly object: THREE.Object3D
  private positions: Float32Array
  private count: number
  private size = 70
  private height = 40

  /** The wind's push on the falling rain or snow, m/s in x and z. */
  private wind = { x: 0, z: 0 }

  constructor(
    private kind: 'rain' | 'snow',
    intensity = 0.7,
    wind: { x: number; z: number } = { x: 0, z: 0 }
  ) {
    this.wind = wind
    this.count = Math.round(((kind === 'rain' ? 6000 : 4000) * Math.min(1.6, Math.max(0.2, intensity))) / 0.7)
    const vertsPer = kind === 'rain' ? 2 : 1
    this.positions = new Float32Array(this.count * 3 * vertsPer)
    for (let i = 0; i < this.count; i++) this.seed(i, Math.random() * this.height)
    const geo = new THREE.BufferGeometry()
    geo.setAttribute('position', new THREE.BufferAttribute(this.positions, 3))
    if (kind === 'rain') {
      this.object = new THREE.LineSegments(geo, new THREE.LineBasicMaterial({ color: 0xaab8c8, transparent: true, opacity: 0.45, fog: false }))
    } else {
      this.object = new THREE.Points(geo, new THREE.PointsMaterial({ color: 0xffffff, size: 0.12, transparent: true, opacity: 0.9, fog: false }))
    }
    this.object.frustumCulled = false
  }

  private seed(i: number, y: number): void {
    const x = (Math.random() - 0.5) * this.size
    const z = (Math.random() - 0.5) * this.size
    if (this.kind === 'rain') {
      // A streak along the way the drop is moving: down, and with the wind.
      const k = i * 6
      const slant = 0.7 / 14
      this.positions.set([x, y, z, x + 0.02 - this.wind.x * slant, y + 0.7, z - this.wind.z * slant], k)
    } else {
      this.positions.set([x, y, z], i * 3)
    }
  }

  /** Moves the flakes or drops on, centred on the camera. */
  update(dt: number, camera: THREE.Vector3, speed: number, heading: number): void {
    this.object.position.set(camera.x, camera.y - this.height / 2, camera.z)
    const fall = (this.kind === 'rain' ? 14 : 1.2) * dt
    // Riding into it: everything streams back past the rider.
    const back = speed * dt
    const bx = -Math.sin(heading) * back + this.wind.x * dt
    const bz = -Math.cos(heading) * back + this.wind.z * dt
    const half = this.size / 2
    const wrap = (v: number): number => (v < -half ? v + this.size : v > half ? v - this.size : v)
    const per = this.kind === 'rain' ? 2 : 1
    for (let i = 0; i < this.count; i++) {
      const k = i * 3 * per
      const y = this.positions[k + 1] - fall
      if (y < 0) {
        this.seed(i, this.height)
        continue
      }
      const sway = this.kind === 'snow' ? Math.sin(y * 0.7 + i) * 0.3 * dt : 0
      for (let v = 0; v < per; v++) {
        const o = k + v * 3
        this.positions[o] = wrap(this.positions[o] + bx + sway)
        this.positions[o + 1] -= fall
        this.positions[o + 2] = wrap(this.positions[o + 2] + bz)
      }
    }
    ;(this.object as THREE.Points).geometry.attributes.position.needsUpdate = true
  }

  dispose(): void {
    const o = this.object as THREE.Points
    o.geometry.dispose()
    ;(o.material as THREE.Material).dispose()
  }
}
