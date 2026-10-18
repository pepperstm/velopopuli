import * as THREE from 'three'

/**
 * Textures for the route's road, one per look (`Surface as u8` in Rust: asphalt, concrete,
 * cobbles, gravel, dirt; then 5, tarmac without a centre line, for anything below a main road). Across: the gravel shoulder on the outer 20/256ths
 * each side (stretched down the embankment), the surface between. Along: 12 m per repeat.
 */
export const SURFACES = ['asphalt', 'concrete', 'cobbles', 'gravel', 'dirt', 'lane'] as const

/** Deterministic noise, so both renderers get the same picture. */
function random(seed: number): () => number {
  let s = seed >>> 0
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0
    return s / 4294967296
  }
}

function canvas(w: number, h: number): [HTMLCanvasElement, CanvasRenderingContext2D] {
  const c = document.createElement('canvas')
  c.width = w
  c.height = h
  return [c, c.getContext('2d')!]
}

/** Speckle of `count` dots in shades `[lo, lo + spread]` over the surface between the shoulders. */
function speckle(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  rnd: () => number,
  count: number,
  rgb: [number, number, number],
  spread: number,
  alpha: number,
  size: number
): void {
  const edge = (w * 20) / 256
  for (let i = 0; i < count; i++) {
    const k = rnd() * spread - spread / 2
    ctx.fillStyle = `rgba(${rgb[0] + k},${rgb[1] + k},${rgb[2] + k},${alpha})`
    ctx.fillRect(edge + rnd() * (w - 2 * edge), rnd() * h, size, size)
  }
}

function shoulders(ctx: CanvasRenderingContext2D, w: number, h: number, colour: string): void {
  ctx.fillStyle = colour
  ctx.fillRect(0, 0, w, h)
}

function draw(surface: number): HTMLCanvasElement {
  // The same grain on tarmac with and without a centre line, so the change shows only in the paint.
  const rnd = random(17 + (SURFACES[surface] === 'lane' ? 0 : surface) * 101)
  switch (SURFACES[surface] ?? 'asphalt') {
    case 'concrete': {
      // Pale slabs with a joint across every 4 m and one down the middle.
      const [c, ctx] = canvas(256, 256)
      shoulders(ctx, 256, 256, '#6d6552')
      ctx.fillStyle = '#9a978f'
      ctx.fillRect(20, 0, 216, 256)
      speckle(ctx, 256, 256, rnd, 2200, [150, 148, 140], 34, 0.45, 2)
      ctx.fillStyle = 'rgba(60,58,54,0.75)'
      for (let y = 0; y < 256; y += 256 / 3) ctx.fillRect(20, Math.round(y), 216, 2)
      ctx.fillRect(127, 0, 2, 256)
      ctx.fillStyle = '#e8e8e2'
      ctx.fillRect(25, 0, 3, 256)
      ctx.fillRect(228, 0, 3, 256)
      return c
    }
    case 'cobbles': {
      // Rows of setts, about 0.2 m across and 0.15 m along, in staggered courses.
      const [c, ctx] = canvas(512, 1024)
      shoulders(ctx, 512, 1024, '#6d6552')
      const left = 40
      const width = 432
      ctx.fillStyle = '#3c3a36' // the gaps between the stones
      ctx.fillRect(left, 0, width, 1024)
      const across = width / ((6.5 / 0.2) | 0)
      const along = 1024 / 80
      for (let row = 0; row < 80; row++) {
        const shift = row % 2 ? across / 2 : 0
        for (let x = left - shift; x < left + width; x += across) {
          const k = rnd()
          const r = 112 + k * 46
          ctx.fillStyle = `rgb(${r},${r - 6 - rnd() * 10},${r - 16 - rnd() * 14})`
          const x0 = Math.max(left, x + 1.2)
          const x1 = Math.min(left + width, x + across - 1.2)
          if (x1 > x0) ctx.fillRect(x0, row * along + 1.2, x1 - x0, along - 2.4)
          // A worn, lighter top to each stone.
          ctx.fillStyle = 'rgba(255,255,255,0.08)'
          if (x1 > x0 + 2) ctx.fillRect(x0 + 1, row * along + 2, x1 - x0 - 2, (along - 4) / 2)
        }
      }
      return c
    }
    case 'gravel': {
      // Pale compacted gravel, loose stones, and two darker wheel tracks.
      const [c, ctx] = canvas(256, 256)
      shoulders(ctx, 256, 256, '#6f7448')
      ctx.fillStyle = '#a8977a'
      ctx.fillRect(20, 0, 216, 256)
      ctx.fillStyle = 'rgba(110,96,74,0.35)'
      ctx.fillRect(62, 0, 34, 256)
      ctx.fillRect(160, 0, 34, 256)
      speckle(ctx, 256, 256, rnd, 4200, [150, 136, 112], 70, 0.6, 2)
      speckle(ctx, 256, 256, rnd, 500, [200, 190, 172], 30, 0.7, 1)
      return c
    }
    case 'dirt': {
      // Brown earth with ruts and grass creeping in at the edges and down the middle.
      const [c, ctx] = canvas(256, 256)
      shoulders(ctx, 256, 256, '#5f7a3c')
      ctx.fillStyle = '#7d6247'
      ctx.fillRect(20, 0, 216, 256)
      ctx.fillStyle = 'rgba(70,52,36,0.5)'
      ctx.fillRect(60, 0, 36, 256)
      ctx.fillRect(160, 0, 36, 256)
      speckle(ctx, 256, 256, rnd, 3200, [120, 94, 68], 50, 0.55, 2)
      ctx.fillStyle = 'rgba(92,122,60,0.55)'
      for (let i = 0; i < 700; i++) {
        const side = rnd()
        const x = side < 0.35 ? 20 + rnd() * 26 : side < 0.7 ? 210 + rnd() * 26 : 112 + rnd() * 32
        ctx.fillRect(x, rnd() * 256, 2, 3)
      }
      return c
    }
    default: {
      // Tarmac with an edge line each side and, on a main road, a broken line down the middle.
      const [c, ctx] = canvas(256, 256)
      shoulders(ctx, 256, 256, '#6d6552')
      ctx.fillStyle = '#3d3f45'
      ctx.fillRect(20, 0, 216, 256)
      // Speckle so the surface visibly moves at speed.
      speckle(ctx, 256, 256, rnd, 2600, [70, 70, 74], 40, 0.5, 2)
      ctx.fillStyle = '#e8e8e2'
      ctx.fillRect(25, 0, 4, 256)
      ctx.fillRect(227, 0, 4, 256)
      if (SURFACES[surface] !== 'lane') ctx.fillRect(126, 0, 4, 128)
      return c
    }
  }
}

const cache = new Map<number, THREE.CanvasTexture>()

export function surfaceTexture(surface: number): THREE.CanvasTexture {
  const hit = cache.get(surface)
  if (hit) return hit
  const texture = new THREE.CanvasTexture(draw(surface))
  texture.colorSpace = THREE.SRGBColorSpace
  texture.wrapT = THREE.RepeatWrapping
  texture.wrapS = THREE.ClampToEdgeWrapping
  texture.anisotropy = 8
  cache.set(surface, texture)
  return texture
}

/** `[first sample, one past the last, surface]` for each stretch of one surface. */
export function surfaceRuns(surfaces: ArrayLike<number>): [number, number, number][] {
  const runs: [number, number, number][] = []
  for (let i = 0; i < surfaces.length; i++) {
    const last = runs[runs.length - 1]
    if (last && last[2] === surfaces[i]) last[1] = i + 1
    else runs.push([i, i + 1, surfaces[i]])
  }
  return runs
}
