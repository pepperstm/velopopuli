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
  climbs: Climb[]
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
    return [s, Math.abs(s) * ((route.width[k] ?? 6.5) / 2) + offset * s]
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

export function sampleAt(route: Route, dist: number, out?: RoutePos): RoutePos {
  const f = Math.max(0, Math.min(route.n - 1, dist / route.step))
  const i = Math.min(route.n - 2, Math.floor(f))
  const t = f - i
  const lerp = (a: Float64Array): number => a[i] + (a[i + 1] - a[i]) * t
  // Heading is taken over a ~25 m baseline so GPS jitter doesn't shake the camera.
  const a = Math.max(0, i - 2)
  const b = Math.min(route.n - 1, i + 3)
  const o = out ?? ({} as RoutePos)
  o.x = lerp(route.x)
  o.z = lerp(route.z)
  o.ele = lerp(route.ele)
  o.grade = lerp(route.grade)
  o.heading = Math.atan2(route.x[b] - route.x[a], route.z[b] - route.z[a])
  return o
}
