// The route as the renderer sees it. All route processing (parsing, smoothing,
// climb detection) happens in the Rust core; these are just its output arrays.

export interface Climb {
  /** Start/end as distance along the route in metres. */
  start: number
  end: number
  gain: number
  length: number
  avgGrade: number
  category: string
}

export interface Route {
  name: string
  step: number
  n: number
  total: number
  /** Local east (x) / south (z) metres — the 3D ground plane. */
  x: Float64Array
  z: Float64Array
  ele: Float64Array
  /** Gradient as a fraction (0.05 = 5%). */
  grade: Float64Array
  lat: Float64Array
  lon: Float64Array
  /** Width of the road in metres, per sample (it follows the mapped road). */
  width: Float64Array
  /** Cross-slope per sample (rise per metre across, + when the +offset side is higher): berms on tight trail turns. */
  bank: Float64Array
  /** How far the riding surface dips per sample, in metres: a ford. */
  dip: Float64Array
  climbs: Climb[]
}

/** `d` folded into `[0, total)`: where on a loop a distance of any size (or sign) lies. */
export function wrapDist(total: number, d: number): number {
  return ((d % total) + total) % total
}

/** Signed shortest difference `to - from` round a loop, in `(-total/2, total/2]`. */
export function wrapGap(total: number, from: number, to: number): number {
  const g = wrapDist(total, to - from)
  return g > total / 2 ? g - total : g
}

/** Half the road's width at `dist` metres along the route. */
export function halfWidthAt(route: Route, dist: number): number {
  const i = Math.max(0, Math.min(route.n - 1, Math.round(dist / route.step)))
  return (route.width[i] ?? 6.5) / 2
}

/**
 * The trail's cross-slope at `dist`, and how far the surface `offset` metres across stands above
 * the centre-line height (the inside edge of a berm stays put; the outside rises). As World::bank_at.
 */
export function bankAt(route: Route, dist: number, offset: number): [number, number] {
  const f = Math.max(0, Math.min(route.n - 1, dist / route.step))
  const i = Math.min(route.n - 2, Math.floor(f))
  const t = f - i
  const at = (k: number): [number, number] => {
    const s = route.bank[k] ?? 0
    return [s, Math.abs(s) * ((route.width[k] ?? 6.5) / 2) + offset * s - (route.dip[k] ?? 0)]
  }
  const [a, b] = [at(i), at(i + 1)]
  return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

export interface RoutePos {
  x: number
  z: number
  ele: number
  grade: number
  /** Direction of travel as a yaw angle for an object whose forward axis is +Z. */
  heading: number
}

/** A point along the route. The line runs on a smooth curve through the samples (Catmull-Rom)
 * rather than straight from one to the next, so a tight bend is ridden round, not in a kink at
 * every sample; the heading is the curve's own direction there. */
export function sampleAt(route: Route, dist: number, out?: RoutePos): RoutePos {
  const f = Math.max(0, Math.min(route.n - 1, dist / route.step))
  const i = Math.min(route.n - 2, Math.floor(f))
  const t = f - i
  const lerp = (a: Float64Array): number => a[i] + (a[i + 1] - a[i]) * t
  const [x, dx] = curve(route.x, route.n, i, t)
  const [z, dz] = curve(route.z, route.n, i, t)
  const o = out ?? ({} as RoutePos)
  o.x = x
  o.z = z
  o.ele = lerp(route.ele)
  o.grade = lerp(route.grade)
  o.heading = Math.atan2(dx, dz)
  return o
}

/** The Catmull-Rom curve through `a` between samples `i` and `i + 1`, at `t` of the way: its
 * value and its rate of change. */
function curve(a: Float64Array, n: number, i: number, t: number): [number, number] {
  const p0 = a[Math.max(0, i - 1)]
  const p1 = a[i]
  const p2 = a[i + 1]
  const p3 = a[Math.min(n - 1, i + 2)]
  const b = p2 - p0
  const c = 2 * p0 - 5 * p1 + 4 * p2 - p3
  const d = -p0 + 3 * p1 - 3 * p2 + p3
  return [p1 + 0.5 * t * (b + t * (c + t * d)), 0.5 * (b + t * (2 * c + 3 * t * d))]
}

/** Over how far a bend's tightness is judged, either side of the rider. */
const LEAN_SPAN = 4
/** The most a rider leans into a bend. */
const MAX_LEAN = (40 * Math.PI) / 180

/** How far a rider at `speed` (m/s) leans into the bend at `dist`, in radians: the lean that
 * balances the turn, atan(v² / g r). Negative leans left, the way `rotation.z` rolls. */
export function leanAt(route: Route, dist: number, speed: number): number {
  const h = (d: number): number => sampleAt(route, d, scratch).heading
  const turn = Math.atan2(Math.sin(h(dist + LEAN_SPAN) - h(dist - LEAN_SPAN)), Math.cos(h(dist + LEAN_SPAN) - h(dist - LEAN_SPAN)))
  // Turning towards +x (heading rising) is turning left.
  const lean = Math.atan((speed * speed * (turn / (2 * LEAN_SPAN))) / 9.81)
  return -Math.max(-MAX_LEAN, Math.min(MAX_LEAN, lean))
}
const scratch = {} as RoutePos
