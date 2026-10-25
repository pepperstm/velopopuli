import * as THREE from 'three'

/** Floats per rotor in the structures buffer: hub x, y, z, facing x, z, radius, phase. */
const STRIDE = 7
/** Tip speed, m/s: big rotors turn slowly, small ones quickly, up to `MAX_SPIN` rad/s. */
const TIP_SPEED = 65
const MAX_SPIN = 2.2
/** Turbines start turning at about 3 m/s of wind and reach full speed at about 12. */
const CUT_IN = 3
const RATED = 12

/** One rotor of radius 1 facing +z: three tapering blades round the hub. */
function rotorGeometry(): THREE.BufferGeometry {
  const pos: number[] = []
  for (let k = 0; k < 3; k++) {
    const a = (k * Math.PI * 2) / 3
    const [dx, dy] = [Math.cos(a), Math.sin(a)]
    const [ax, ay] = [-dy, dx]
    const p = (d: number, w: number): number[] => [dx * d + ax * w, dy * d + ay * w, 0]
    const [r0, r1, t0, t1] = [p(0.04, -0.045), p(1, -0.012), p(1, 0.012), p(0.04, 0.045)]
    pos.push(...r0, ...r1, ...t0, ...r0, ...t0, ...t1)
  }
  const geo = new THREE.BufferGeometry()
  geo.setAttribute('position', new THREE.Float32BufferAttribute(pos, 3))
  geo.computeVertexNormals()
  return geo
}

/** The wind turbines' rotors, turning. Their towers are part of the structures mesh. */
export class Rotors {
  readonly mesh: THREE.InstancedMesh
  private readonly base: THREE.Matrix4[] = []
  private readonly hub: THREE.Vector3[] = []
  private readonly radius: number[] = []
  private readonly phase: number[] = []
  private readonly spin: number[] = []
  private readonly angle: number[] = []

  constructor(data: Float32Array) {
    const count = Math.floor(data.length / STRIDE)
    this.mesh = new THREE.InstancedMesh(rotorGeometry(), new THREE.MeshLambertMaterial({ color: 0xe8eaea, side: THREE.DoubleSide }), count)
    for (let i = 0; i < count; i++) {
      const [x, y, z, fx, fz, radius, phase] = data.subarray(i * STRIDE, (i + 1) * STRIDE)
      this.hub.push(new THREE.Vector3(x, y, z))
      this.radius.push(radius)
      this.base.push(new THREE.Matrix4())
      this.place(i, fx, fz)
      this.angle.push(phase)
      this.spin.push(Math.min(MAX_SPIN, TIP_SPEED / Math.max(radius, 1)))
    }
    this.mesh.frustumCulled = false
    this.update(0)
  }

  private place(i: number, fx: number, fz: number): void {
    const r = this.radius[i]
    this.base[i]
      .makeTranslation(this.hub[i].x, this.hub[i].y, this.hub[i].z)
      .multiply(new THREE.Matrix4().makeRotationY(Math.atan2(fx, fz)))
      .multiply(new THREE.Matrix4().makeScale(r, r, r))
  }

  /**
   * Turns the rotors into the real wind (`from`, degrees clockwise from north) and sets their
   * speed by it: still below the cut-in speed, full speed from about 12 m/s.
   */
  setWind(speed: number, from: number): void {
    const a = (from * Math.PI) / 180
    const share = speed < CUT_IN ? 0 : Math.min(1, (speed - CUT_IN) / (RATED - CUT_IN) + 0.2)
    for (let i = 0; i < this.base.length; i++) {
      this.place(i, Math.sin(a), -Math.cos(a))
      this.spin[i] = Math.min(MAX_SPIN, TIP_SPEED / Math.max(this.radius[i], 1)) * share
    }
    this.update(0)
  }

  update(dt: number): void {
    const turn = new THREE.Matrix4()
    const m = new THREE.Matrix4()
    for (let i = 0; i < this.base.length; i++) {
      this.angle[i] += dt * this.spin[i]
      turn.makeRotationZ(this.angle[i])
      this.mesh.setMatrixAt(i, m.multiplyMatrices(this.base[i], turn))
    }
    this.mesh.instanceMatrix.needsUpdate = true
  }
}
