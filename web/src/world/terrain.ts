import * as THREE from 'three'
import { weathered } from './weathered'
import { mergeGeometries } from 'three/examples/jsm/utils/BufferGeometryUtils.js'
import { api } from '../bridge'
import type { WorldLayout } from '../types'

// The landscape is computed in Rust, off the render thread, and arrives as
// ready-made vertex buffers: square chunks around the rider plus one coarse
// sheet for the distant mountains. This class only decides what to ask for and
// uploads the answers to the GPU.

const MAX_IN_FLIGHT = 6

function buildTreeGeometry(): THREE.BufferGeometry {
  const paint = (g: THREE.BufferGeometry, r: number, gr: number, b: number): THREE.BufferGeometry => {
    const n = g.getAttribute('position').count
    const colors = new Float32Array(n * 3)
    for (let i = 0; i < n; i++) colors.set([r, gr, b], i * 3)
    g.setAttribute('color', new THREE.BufferAttribute(colors, 3))
    return g.toNonIndexed()
  }
  const trunk = paint(new THREE.CylinderGeometry(0.22, 0.3, 1.8, 5).translate(0, 0.9, 0), 0.32, 0.22, 0.14)
  const lower = paint(new THREE.ConeGeometry(2.1, 4.2, 7).translate(0, 3.6, 0), 0.13, 0.36, 0.17)
  const upper = paint(new THREE.ConeGeometry(1.5, 3.6, 7).translate(0, 6.0, 0), 0.16, 0.42, 0.2)
  return mergeGeometries([trunk, lower, upper])!
}

/**
 * One window bay of one storey. It multiplies the wall colour, so it is white
 * except for the window itself; the bottom-left corner is plain wall, which
 * roofs and gables point at.
 */
export function windowTexture(): THREE.CanvasTexture {
  const canvas = document.createElement('canvas')
  canvas.width = 64
  canvas.height = 64
  const ctx = canvas.getContext('2d')!
  ctx.fillStyle = '#ffffff'
  ctx.fillRect(0, 0, 64, 64)
  ctx.fillStyle = '#e2ddd4' // frame and sill
  ctx.fillRect(19, 13, 26, 36)
  ctx.fillStyle = '#3b4656' // glass
  ctx.fillRect(22, 16, 20, 29)
  ctx.fillStyle = '#566477' // a hint of sky reflected in the upper pane
  ctx.fillRect(22, 16, 20, 9)
  ctx.fillStyle = '#e2ddd4'
  ctx.fillRect(31, 16, 2, 29)
  const texture = new THREE.CanvasTexture(canvas)
  texture.colorSpace = THREE.SRGBColorSpace
  texture.wrapS = texture.wrapT = THREE.RepeatWrapping
  texture.anisotropy = 4
  return texture
}

/**
 * Surface of the roads around the route. The left three-eighths is tarmac with
 * a broken centre line, for main roads (their edge lines are geometry); the rest
 * is blank, for everything whose colour comes from its vertices alone.
 */
export function waysTexture(): THREE.CanvasTexture {
  const canvas = document.createElement('canvas')
  canvas.width = 128
  canvas.height = 64
  const ctx = canvas.getContext('2d')!
  ctx.fillStyle = '#ffffff'
  ctx.fillRect(0, 0, 128, 64)
  // The tarmac runs on past the road's edge, so that distant (blurred) roads don't pick up the blank part.
  ctx.fillStyle = '#3d3f45'
  ctx.fillRect(0, 0, 80, 64)
  ctx.fillStyle = '#e8e8e2'
  ctx.fillRect(23, 0, 2, 22)
  const texture = new THREE.CanvasTexture(canvas)
  texture.colorSpace = THREE.SRGBColorSpace
  texture.wrapT = THREE.RepeatWrapping
  texture.wrapS = THREE.ClampToEdgeWrapping
  texture.anisotropy = 8
  return texture
}

function gridIndices(cells: number): Uint16Array {
  const row = cells + 1
  const idx: number[] = []
  for (let j = 0; j < cells; j++) {
    for (let i = 0; i < cells; i++) {
      const a = j * row + i
      idx.push(a, a + row, a + 1, a + 1, a + row, a + row + 1)
    }
  }
  return new Uint16Array(idx)
}

interface GridReply {
  positions: Float32Array
  normals: Float32Array
  colors: Float32Array
  trees: Float32Array
  /** Building triangles: positions, normals and colours (three floats per vertex), then UVs (two). */
  buildings: [Float32Array, Float32Array, Float32Array, Float32Array] | null
  /** Water triangles: positions and colours. */
  water: [Float32Array, Float32Array] | null
  /** Triangles of the roads around the route: positions, normals, colours and UVs. */
  roads: [Float32Array, Float32Array, Float32Array, Float32Array] | null
}

/**
 * Splits a mesh reply: `[ground vertices, trees, building vertices, water vertices,
 * road vertices]`, then ground positions, normals and colours; trees; building
 * positions, normals, colours and UVs; water positions and colours; road positions,
 * normals, colours and UVs.
 */
function decodeGrid(buffer: ArrayBuffer): GridReply {
  const [verts, trees, walls, water, roads] = new Uint32Array(buffer, 0, 5)
  let offset = 20
  const take = (length: number): Float32Array => {
    const view = new Float32Array(buffer, offset, length)
    offset += length * 4
    return view
  }
  return {
    positions: take(verts * 3),
    normals: take(verts * 3),
    colors: take(verts * 3),
    trees: take(trees * 6),
    buildings: walls ? [take(walls * 3), take(walls * 3), take(walls * 3), take(walls * 2)] : null,
    water: water ? [take(water * 3), take(water * 3)] : null,
    roads: roads ? [take(roads * 3), take(roads * 3), take(roads * 3), take(roads * 2)] : null
  }
}

interface Chunk {
  mesh: THREE.Mesh
  trees: THREE.InstancedMesh | null
  buildings: THREE.Mesh | null
  water: THREE.Mesh | null
  roads: THREE.Mesh | null
  /** Vertex positions, kept for ground-height lookups. */
  positions: Float32Array
  /** Imagery on the ground: its size in pixels (0 for none yet, −1 for none to be had), and whether a request is out. */
  imagery: { size: number; busy: boolean; material: THREE.MeshLambertMaterial | null }
}

/** Chunk image sizes (as `world::imagery`): fine within `NEAR_RING` chunks of the rider, coarse beyond. */
const NEAR_PX = 256
const FAR_PX = 128
const NEAR_RING = 2
/** Imagery requests out at once: few, so the nearest chunks get theirs first. */
const IMAGERY_IN_FLIGHT = 3

/**
 * Fine mottling, repeated every few metres over imagery, so the ground close by has grain
 * even where the imagery is coarse (Sentinel-2 is 10 m a pixel). Mid-grey on average.
 */
function detailTexture(): THREE.Texture {
  const size = 128
  const data = new Uint8Array(size * size * 4)
  let seed = 7
  const rand = (): number => (seed = (seed * 16807) % 2147483647) / 2147483647
  for (let i = 0; i < size * size; i++) {
    const v = 128 + (rand() - 0.5) * 70
    data.set([v, v, v, 255], i * 4)
  }
  const t = new THREE.DataTexture(data, size, size, THREE.RGBAFormat)
  t.wrapS = t.wrapT = THREE.RepeatWrapping
  t.magFilter = THREE.LinearFilter
  t.minFilter = THREE.LinearMipmapLinearFilter
  t.generateMipmaps = true
  t.needsUpdate = true
  return t
}

/** Imagery is a little dull beside the painted world; lifted by this much. */
const IMAGERY_GAIN = 1.15
/** Detail repeats per chunk side (240 m): every 4 m. */
const DETAIL_REPEAT = 60

/** How much rain darkens the ground, and roads (which shine darker still when wet). */
const GROUND_WET = 0.12
/** Snow settles on ground sloping less than about 50°, and covers it below about 25°. */
const GROUND_SNOW = 0.62
export const ROAD_WET = 0.35

/**
 * Ground with imagery on it. A photo taken from above smears down a cliff, so where the
 * ground is steep it gives way to the ground's own colours (`steep`: 0 flat, 1 sheer).
 */
function imageryMaterial(texture: THREE.Texture, detail: THREE.Texture): THREE.MeshLambertMaterial {
  const m = new THREE.MeshLambertMaterial({ map: texture, vertexColors: true })
  m.onBeforeCompile = (shader) => {
    shader.uniforms.detail = { value: detail }
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', '#include <common>\nattribute float steep;\nvarying float vSteep;')
      .replace('#include <begin_vertex>', '#include <begin_vertex>\nvSteep = steep;')
    shader.fragmentShader = shader.fragmentShader.replace('#include <common>', '#include <common>\nvarying float vSteep;\nuniform sampler2D detail;').replace(
      '#include <color_fragment>',
      `diffuseColor.rgb *= ${IMAGERY_GAIN.toFixed(2)} * (0.5 + texture2D(detail, vMapUv * ${DETAIL_REPEAT.toFixed(1)}).r);
      diffuseColor.rgb = mix(diffuseColor.rgb, vColor.rgb, vSteep);`
    )
  }
  m.customProgramCacheKey = () => 'imagery'
  return weathered(m, GROUND_WET, GROUND_SNOW)
}

/** Splits a `terrain_imagery` reply into its credits and pixels; `null` if there is no image. */
function decodeImagery(buf: ArrayBuffer): { size: number; credits: string[]; rgba: Uint8Array } | null {
  if (buf.byteLength < 8) return null
  const [size, text] = new Uint32Array(buf, 0, 2)
  const credits = new TextDecoder()
    .decode(new Uint8Array(buf, 8, text))
    .split('\n')
    .filter(Boolean)
  const start = 8 + Math.ceil(text / 4) * 4
  return { size, credits, rgba: new Uint8Array(buf, start, size * size * 4) }
}

const key = (cx: number, cz: number): string => `${cx},${cz}`

export class Terrain {
  readonly group = new THREE.Group()
  private chunks = new Map<string, Chunk>()
  private pending = new Set<string>()
  private indices: Uint16Array
  private ground = weathered(new THREE.MeshLambertMaterial({ vertexColors: true }), GROUND_WET, GROUND_SNOW)
  private treeGeometry = buildTreeGeometry()
  private treeMaterial = weathered(new THREE.MeshLambertMaterial({ vertexColors: true, flatShading: true }), 0, 0.15)
  private buildingMaterial = weathered(new THREE.MeshLambertMaterial({ vertexColors: true, map: windowTexture() }), 0.08, 0.4)
  // Slightly see-through, with a sheen where the sun catches it.
  private waterMaterial = new THREE.MeshPhongMaterial({ vertexColors: true, transparent: true, opacity: 0.86, shininess: 90, specular: 0x8899aa })
  // Roads lie on the ground and are painted over it, lesser ways first: they are drawn after
  // everything solid and leave the depth buffer alone, so they can't flicker against the
  // ground or against each other where they meet.
  private roadMaterial = weathered(
    new THREE.MeshLambertMaterial({
      vertexColors: true,
      map: waysTexture(),
      depthWrite: false,
      polygonOffset: true,
      polygonOffsetFactor: -2,
      polygonOffsetUnits: -4
    }),
    ROAD_WET,
    null
  )
  private far: THREE.Mesh
  private farAt = ''
  private farBusy = false
  private centre = { cx: 0, cz: 0 }
  private disposed = false
  private credits = new Set<string>()
  private detail = detailTexture()
  private imageryBusy = 0

  /** `onCredits` hears the imagery credits whenever one is added. */
  constructor(
    private layout: WorldLayout,
    private onCredits: (credits: string[]) => void = () => undefined
  ) {
    this.indices = gridIndices(layout.cells)
    const farGeo = new THREE.BufferGeometry()
    const farVerts = (layout.farCells + 1) ** 2
    for (const name of ['position', 'normal', 'color']) farGeo.setAttribute(name, new THREE.BufferAttribute(new Float32Array(farVerts * 3), 3))
    farGeo.setIndex(Array.from(gridIndices(layout.farCells)))
    this.far = new THREE.Mesh(farGeo, this.ground)
    this.far.frustumCulled = false
    this.far.visible = false
    this.group.add(this.far)
  }

  /** Loads the landscape immediately around a point and resolves once it is all in place. */
  async prime(x: number, z: number): Promise<void> {
    const { chunk } = this.layout
    const cx = Math.floor(x / chunk)
    const cz = Math.floor(z / chunk)
    this.centre = { cx, cz }
    const jobs: Promise<void>[] = [this.loadFar(x, z)]
    for (let dx = -2; dx <= 2; dx++) for (let dz = -2; dz <= 2; dz++) jobs.push(this.load(cx + dx, cz + dz))
    await Promise.all(jobs)
  }

  /** Requests whatever is missing around (x, z), nearest first, and drops what is far behind. */
  update(x: number, z: number): void {
    const { chunk, viewChunks: view } = this.layout
    const cx = Math.floor(x / chunk)
    const cz = Math.floor(z / chunk)
    this.centre = { cx, cz }
    void this.loadFar(x, z)

    outer: for (let ring = 0; ring <= view; ring++) {
      for (let dx = -ring; dx <= ring; dx++) {
        for (let dz = -ring; dz <= ring; dz++) {
          if (Math.max(Math.abs(dx), Math.abs(dz)) !== ring || dx * dx + dz * dz > (view + 0.5) ** 2) continue
          if (this.pending.size >= MAX_IN_FLIGHT) break outer
          const k = key(cx + dx, cz + dz)
          if (!this.chunks.has(k) && !this.pending.has(k)) void this.load(cx + dx, cz + dz)
        }
      }
    }
    for (const [k, c] of this.chunks) {
      const [kx, kz] = k.split(',').map(Number)
      if (Math.abs(kx - cx) > view + 2 || Math.abs(kz - cz) > view + 2) {
        this.release(c)
        this.chunks.delete(k)
      }
    }
    this.pumpImagery()
  }

  /** Starts imagery for the nearest chunks that lack it, or have only the coarse kind up close. */
  private pumpImagery(): void {
    while (this.imageryBusy < IMAGERY_IN_FLIGHT) {
      let best: { cx: number; cz: number; c: Chunk; d: number } | null = null
      for (const [k, c] of this.chunks) {
        if (c.imagery.busy || c.imagery.size < 0) continue
        const [cx, cz] = k.split(',').map(Number)
        if (c.imagery.size !== 0 && !(c.imagery.size < NEAR_PX && this.near(cx, cz))) continue
        const d = Math.hypot(cx - this.centre.cx, cz - this.centre.cz)
        if (!best || d < best.d) best = { cx, cz, c, d }
      }
      if (!best) return
      void this.loadImagery(best.cx, best.cz, best.c)
    }
  }

  private near(cx: number, cz: number): boolean {
    return Math.max(Math.abs(cx - this.centre.cx), Math.abs(cz - this.centre.cz)) <= NEAR_RING
  }

  /** Puts imagery on a chunk's ground, sharper if it is near; keeps its own colours if there is none. */
  private async loadImagery(cx: number, cz: number, c: Chunk): Promise<void> {
    if (c.imagery.busy) return
    c.imagery.busy = true
    this.imageryBusy++
    try {
      const size = this.near(cx, cz) ? NEAR_PX : FAR_PX
      const img = decodeImagery(await api.ride.imagery(cx, cz, size))
      // Nothing to be had here (imagery off, or offline): the ground keeps its own colours.
      if (!img) {
        if (c.imagery.size === 0) c.imagery.size = -1
        return
      }
      // Gone while it was fetched.
      if (this.disposed || this.chunks.get(key(cx, cz)) !== c) return
      const texture = new THREE.DataTexture(img.rgba, img.size, img.size, THREE.RGBAFormat)
      texture.colorSpace = THREE.SRGBColorSpace
      texture.magFilter = THREE.LinearFilter
      texture.minFilter = THREE.LinearMipmapLinearFilter
      texture.generateMipmaps = true
      texture.anisotropy = 8
      texture.wrapS = texture.wrapT = THREE.ClampToEdgeWrapping
      texture.needsUpdate = true
      const geo = c.mesh.geometry
      if (!geo.getAttribute('uv')) {
        // The image spans the chunk exactly, rows running along +z.
        const { chunk } = this.layout
        const p = c.positions
        const normals = geo.getAttribute('normal').array
        const count = p.length / 3
        const uv = new Float32Array(count * 2)
        const steep = new Float32Array(count)
        for (let i = 0; i < count; i++) {
          uv[i * 2] = p[i * 3] / chunk - cx
          uv[i * 2 + 1] = p[i * 3 + 2] / chunk - cz
          // From about 30° the photo gives way, and is gone by about 50°.
          steep[i] = THREE.MathUtils.smoothstep(1 - normals[i * 3 + 1] ** 2, 0.25, 0.6)
        }
        geo.setAttribute('uv', new THREE.BufferAttribute(uv, 2))
        geo.setAttribute('steep', new THREE.BufferAttribute(steep, 1))
      }
      const old = c.imagery.material
      c.imagery.material = imageryMaterial(texture, this.detail)
      c.mesh.material = c.imagery.material
      if (old) {
        old.map?.dispose()
        old.dispose()
      }
      c.imagery.size = img.size
      const before = this.credits.size
      for (const credit of img.credits) this.credits.add(credit)
      if (this.credits.size !== before) this.onCredits([...this.credits])
    } catch (err) {
      if (!this.disposed) void api.log(`imagery ${cx},${cz} failed: ${err}`)
    } finally {
      c.imagery.busy = false
      this.imageryBusy--
    }
  }

  private async load(cx: number, cz: number): Promise<void> {
    const k = key(cx, cz)
    if (this.chunks.has(k) || this.pending.has(k)) return
    this.pending.add(k)
    try {
      const data = decodeGrid(await api.ride.chunk(cx, cz))
      // By the time it arrives the rider may have moved on, or the ride may be over.
      const view = this.layout.viewChunks + 2
      if (this.disposed || Math.abs(cx - this.centre.cx) > view || Math.abs(cz - this.centre.cz) > view) return
      const geo = new THREE.BufferGeometry()
      geo.setAttribute('position', new THREE.BufferAttribute(data.positions, 3))
      geo.setAttribute('normal', new THREE.BufferAttribute(data.normals, 3))
      geo.setAttribute('color', new THREE.BufferAttribute(data.colors, 3))
      // Each chunk gets its own index attribute: sharing one would free its GPU buffer
      // for every chunk the first time any chunk is disposed.
      geo.setIndex(new THREE.BufferAttribute(this.indices, 1))
      geo.computeBoundingSphere()
      const mesh = new THREE.Mesh(geo, this.ground)
      this.group.add(mesh)

      let trees: THREE.InstancedMesh | null = null
      const count = data.trees.length / 6
      if (count) {
        trees = new THREE.InstancedMesh(this.treeGeometry, this.treeMaterial, count)
        const holder = new THREE.Object3D()
        for (let i = 0; i < count; i++) {
          const [x, y, z, yaw, scale, tall] = data.trees.subarray(i * 6, i * 6 + 6)
          holder.position.set(x, y, z)
          holder.rotation.set(0, yaw, 0)
          holder.scale.set(scale, tall, scale)
          holder.updateMatrix()
          trees.setMatrixAt(i, holder.matrix)
        }
        trees.instanceMatrix.needsUpdate = true
        trees.computeBoundingSphere()
        this.group.add(trees)
      }
      let buildings: THREE.Mesh | null = null
      if (data.buildings) {
        const walls = new THREE.BufferGeometry()
        walls.setAttribute('position', new THREE.BufferAttribute(data.buildings[0], 3))
        walls.setAttribute('normal', new THREE.BufferAttribute(data.buildings[1], 3))
        walls.setAttribute('color', new THREE.BufferAttribute(data.buildings[2], 3))
        walls.setAttribute('uv', new THREE.BufferAttribute(data.buildings[3], 2))
        walls.computeBoundingSphere()
        buildings = new THREE.Mesh(walls, this.buildingMaterial)
        this.group.add(buildings)
      }
      let water: THREE.Mesh | null = null
      if (data.water) {
        const surface = new THREE.BufferGeometry()
        surface.setAttribute('position', new THREE.BufferAttribute(data.water[0], 3))
        surface.setAttribute('color', new THREE.BufferAttribute(data.water[1], 3))
        // Still water is as good as level; one upward normal serves every vertex.
        const up = new Float32Array(data.water[0].length)
        for (let i = 1; i < up.length; i += 3) up[i] = 1
        surface.setAttribute('normal', new THREE.BufferAttribute(up, 3))
        surface.computeBoundingSphere()
        water = new THREE.Mesh(surface, this.waterMaterial)
        this.group.add(water)
      }
      let roads: THREE.Mesh | null = null
      if (data.roads) {
        const ways = new THREE.BufferGeometry()
        ways.setAttribute('position', new THREE.BufferAttribute(data.roads[0], 3))
        ways.setAttribute('normal', new THREE.BufferAttribute(data.roads[1], 3))
        ways.setAttribute('color', new THREE.BufferAttribute(data.roads[2], 3))
        ways.setAttribute('uv', new THREE.BufferAttribute(data.roads[3], 2))
        ways.computeBoundingSphere()
        roads = new THREE.Mesh(ways, this.roadMaterial)
        roads.renderOrder = 1
        this.group.add(roads)
      }
      const chunk: Chunk = { mesh, trees, buildings, water, roads, positions: data.positions, imagery: { size: 0, busy: false, material: null } }
      this.chunks.set(k, chunk)
    } catch (err) {
      if (!this.disposed) void api.log(`terrain chunk ${k} failed: ${err}`)
    } finally {
      this.pending.delete(k)
    }
  }

  private async loadFar(x: number, z: number): Promise<void> {
    const cell = this.layout.farCell
    // Snap the sheet to its own grid so its vertices don't swim as the rider moves.
    const cx = Math.round(x / cell) * cell
    const cz = Math.round(z / cell) * cell
    if (this.farBusy || this.farAt === key(cx, cz)) return
    this.farBusy = true
    try {
      const data = decodeGrid(await api.ride.far(cx, cz))
      if (this.disposed) return
      const geo = this.far.geometry
      for (const [name, values] of [
        ['position', data.positions],
        ['normal', data.normals],
        ['color', data.colors]
      ] as const) {
        const attr = geo.getAttribute(name) as THREE.BufferAttribute
        ;(attr.array as Float32Array).set(values)
        attr.needsUpdate = true
      }
      this.far.visible = true
      this.farAt = key(cx, cz)
    } catch (err) {
      if (!this.disposed) void api.log(`far terrain failed: ${err}`)
    } finally {
      this.farBusy = false
    }
  }

  /** Ground height under a point, from the loaded chunk there; `null` if that chunk isn't in yet. */
  heightAt(x: number, z: number): number | null {
    const { chunk, cells } = this.layout
    const cx = Math.floor(x / chunk)
    const cz = Math.floor(z / chunk)
    const c = this.chunks.get(key(cx, cz))
    if (!c) return null
    const fx = (x / chunk - cx) * cells
    const fz = (z / chunk - cz) * cells
    const i = Math.min(cells - 1, Math.floor(fx))
    const j = Math.min(cells - 1, Math.floor(fz))
    const u = fx - i
    const v = fz - j
    const h = (ii: number, jj: number): number => c.positions[(jj * (cells + 1) + ii) * 3 + 1]
    return (h(i, j) * (1 - u) + h(i + 1, j) * u) * (1 - v) + (h(i, j + 1) * (1 - u) + h(i + 1, j + 1) * u) * v
  }

  private release(c: Chunk): void {
    this.group.remove(c.mesh)
    c.mesh.geometry.dispose()
    if (c.imagery.material) {
      c.imagery.material.map?.dispose()
      c.imagery.material.dispose()
      c.imagery.material = null
    }
    if (c.trees) {
      this.group.remove(c.trees)
      c.trees.dispose()
    }
    if (c.buildings) {
      this.group.remove(c.buildings)
      c.buildings.geometry.dispose()
    }
    if (c.water) {
      this.group.remove(c.water)
      c.water.geometry.dispose()
    }
    if (c.roads) {
      this.group.remove(c.roads)
      c.roads.geometry.dispose()
    }
  }

  dispose(): void {
    this.disposed = true
    for (const c of this.chunks.values()) this.release(c)
    this.chunks.clear()
    this.far.geometry.dispose()
    this.ground.dispose()
    this.detail.dispose()
    this.treeGeometry.dispose()
    this.treeMaterial.dispose()
    this.buildingMaterial.map?.dispose()
    this.buildingMaterial.dispose()
    this.waterMaterial.dispose()
    this.roadMaterial.map?.dispose()
    this.roadMaterial.dispose()
  }
}
