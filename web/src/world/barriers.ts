import * as THREE from 'three'

/** Sections are drawn from this far behind the rider to this far ahead. */
const BEHIND_M = 150
const AHEAD_M = 900
/** The drawn stretch is moved on once the rider has gone this far. */
const STEP_M = 40

let banner: THREE.CanvasTexture | null = null

/**
 * The banner hung on some barriers: white with the event name in grey, tinted per barrier
 * by its vertex colours. Plain steel samples the white top-left corner.
 */
export function bannerTexture(): THREE.CanvasTexture {
  if (banner) return banner
  const canvas = document.createElement('canvas')
  canvas.width = 512
  canvas.height = 128
  const ctx = canvas.getContext('2d')!
  ctx.fillStyle = '#ffffff'
  ctx.fillRect(0, 0, 512, 128)
  ctx.fillStyle = '#2e322a'
  // The logo's V (the website's mark, drawn on a 36-unit grid), then the name.
  ctx.save()
  ctx.translate(20, 22)
  ctx.scale(2.6, 2.6)
  ctx.fill(new Path2D('M3 8h8l7 18L29 8h5L20 32h-7z'))
  ctx.fill(new Path2D('M15 8h8l-4 9z'))
  ctx.restore()
  ctx.font = '800 54px Manrope, "DM Sans", -apple-system, Helvetica, Arial, sans-serif'
  ctx.textAlign = 'center'
  ctx.textBaseline = 'middle'
  ctx.fillText('VELOPOPULI', 318, 68)
  banner = new THREE.CanvasTexture(canvas)
  banner.colorSpace = THREE.SRGBColorSpace
  banner.anisotropy = 8
  return banner
}

/** Crowd barriers along the route (built in Rust, see `world/barriers.rs`), drawn near the rider. */
export class Barriers {
  readonly group = new THREE.Group()
  private sections: Float32Array
  private meshes: THREE.InstancedMesh[] = []
  private shownAt = -Infinity

  constructor(buffer: ArrayBuffer) {
    // Header: section count, number of looks, then each look's vertex count.
    const [count, looks] = new Uint32Array(buffer, 0, 2)
    const counts = Array.from(new Uint32Array(buffer, 8, looks))
    let offset = 8 + looks * 4
    this.sections = new Float32Array(buffer, offset, count * 6)
    offset += count * 24
    const capacity = Math.ceil(((BEHIND_M + AHEAD_M) / 2.5) * 2) + 8
    const material = new THREE.MeshLambertMaterial({ map: bannerTexture(), vertexColors: true })
    for (const verts of counts) {
      const take = (size: number): THREE.BufferAttribute => {
        const view = new Float32Array(buffer, offset, verts * size)
        offset += verts * size * 4
        return new THREE.BufferAttribute(view, size)
      }
      const geo = new THREE.BufferGeometry()
      geo.setAttribute('position', take(3))
      geo.setAttribute('normal', take(3))
      geo.setAttribute('color', take(3))
      geo.setAttribute('uv', take(2))
      const mesh = new THREE.InstancedMesh(geo, material, capacity)
      mesh.count = 0
      // The instances move with the rider; the base geometry's bounds say nothing about them.
      mesh.frustumCulled = false
      this.meshes.push(mesh)
      this.group.add(mesh)
    }
  }

  /** Shows the sections around `dist` metres along the route. */
  update(dist: number): void {
    if (Math.abs(dist - this.shownAt) < STEP_M) return
    this.shownAt = dist
    const s = this.sections
    const n = s.length / 6
    // First section within reach (they are in order along the route).
    let lo = 0
    let hi = n
    while (lo < hi) {
      const mid = (lo + hi) >> 1
      if (s[mid * 6 + 4] < dist - BEHIND_M) lo = mid + 1
      else hi = mid
    }
    const holder = new THREE.Object3D()
    const filled = this.meshes.map(() => 0)
    for (let i = lo; i < n && s[i * 6 + 4] < dist + AHEAD_M; i++) {
      const look = s[i * 6 + 5]
      const mesh = this.meshes[look]
      if (!mesh || filled[look] >= mesh.instanceMatrix.count) continue
      holder.position.set(s[i * 6], s[i * 6 + 1], s[i * 6 + 2])
      holder.rotation.set(0, s[i * 6 + 3], 0)
      holder.updateMatrix()
      mesh.setMatrixAt(filled[look]++, holder.matrix)
    }
    this.meshes.forEach((mesh, k) => {
      mesh.count = filled[k]
      mesh.instanceMatrix.needsUpdate = true
    })
  }
}
