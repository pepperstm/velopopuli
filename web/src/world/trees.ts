import * as THREE from 'three'
import { api } from '../bridge'

// Trees are drawn in 3D near the camera and as sprites further out: a picture of the same
// model, turned to face the camera, two triangles instead of a hundred. The pictures are
// rendered here from the models once, into one atlas the native renderer shares.

/** As `SPRITES_FROM` and `SPRITE_SPREAD` in `world/trees.rs`: where trees turn into sprites
 * (metres from the camera along the ground), and how far either way each tree's own
 * distance strays, so the switch has no visible line. */
export const SPRITES_FROM = 200
const SPRITE_SPREAD = 30

/** Pixels per kind in the atlas: one cell each, side by side. */
const CELL_W = 256
const CELL_H = 512

/** The GLSL for each tree's own switching distance, from where it stands. Must match `sprite_from` in the native shader. */
const SPRITE_FROM_GLSL = /* glsl */ `
float spriteFrom(vec3 at) {
  return ${SPRITES_FROM.toFixed(1)} + (fract(sin(dot(at.xz, vec2(12.9898, 78.233))) * 43758.5453) - 0.5) * ${SPRITE_SPREAD.toFixed(1)};
}`

export interface TreeKit {
  /** The 3D model of each kind. */
  models: THREE.BufferGeometry[]
  /** Each kind's sprite: a quad over its frame, textured from its cell of the atlas. */
  sprites: THREE.BufferGeometry[]
  /** The pictures, as sRGB pixels (also handed to the native renderer). */
  atlas: HTMLCanvasElement
  texture: THREE.CanvasTexture
}

/** Loads the models (built in Rust, see `world/trees.rs`) and renders their sprites. */
export async function loadTrees(renderer: THREE.WebGLRenderer): Promise<TreeKit> {
  const buffer = await api.ride.trees()
  const [kinds] = new Uint32Array(buffer, 0, 1)
  const counts = Array.from(new Uint32Array(buffer, 4, kinds))
  let offset = 4 + kinds * 4
  const frames = Array.from({ length: kinds }, (_, k) => new Float32Array(buffer, offset + k * 12, 3))
  offset += kinds * 12
  const models = counts.map((verts) => {
    const geo = new THREE.BufferGeometry()
    for (const name of ['position', 'normal', 'color']) {
      geo.setAttribute(name, new THREE.BufferAttribute(new Float32Array(buffer, offset, verts * 3), 3))
      offset += verts * 12
    }
    return geo
  })
  const sprites = frames.map(([half, bottom, top], k) => {
    const geo = new THREE.BufferGeometry()
    const [u0, u1] = [k / kinds, (k + 1) / kinds]
    geo.setAttribute(
      'position',
      new THREE.BufferAttribute(new Float32Array([-half, bottom, 0, half, bottom, 0, half, top, 0, -half, bottom, 0, half, top, 0, -half, top, 0]), 3)
    )
    geo.setAttribute('uv', new THREE.BufferAttribute(new Float32Array([u0, 0, u1, 0, u1, 1, u0, 0, u1, 1, u0, 1]), 2))
    geo.setAttribute('normal', new THREE.BufferAttribute(new Float32Array(18).fill(0), 3))
    return geo
  })
  const atlas = renderAtlas(renderer, models, frames)
  const texture = new THREE.CanvasTexture(atlas)
  texture.colorSpace = THREE.SRGBColorSpace
  texture.anisotropy = 4
  return { models, sprites, atlas, texture }
}

/**
 * Draws each model side-on into its cell, fitted to its frame. The pictures carry a little
 * shading of their own (lighter towards the sun's usual side and on top) so they keep some
 * depth; the scene's light then falls on them like on any other surface.
 */
function renderAtlas(renderer: THREE.WebGLRenderer, models: THREE.BufferGeometry[], frames: Float32Array[]): HTMLCanvasElement {
  const width = CELL_W * models.length
  const target = new THREE.WebGLRenderTarget(width, CELL_H)
  const material = new THREE.MeshBasicMaterial({ vertexColors: true, side: THREE.DoubleSide })
  material.onBeforeCompile = (shader) => {
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', '#include <common>\nvarying float vShade;')
      .replace(
        '#include <begin_vertex>',
        '#include <begin_vertex>\nvShade = 0.72 + 0.38 * max(dot(normalize(normal), normalize(vec3(0.35, 0.75, 0.55))), 0.0) + 0.1 * normal.y;'
      )
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', '#include <common>\nvarying float vShade;')
      // The cell holds sRGB pixels, as every other texture does.
      .replace(
        '#include <dithering_fragment>',
        '#include <dithering_fragment>\ngl_FragColor.rgb = pow(clamp(gl_FragColor.rgb * vShade, 0.0, 1.0), vec3(1.0 / 2.2));'
      )
  }
  const scene = new THREE.Scene()
  const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, -100, 100)
  const before = { target: renderer.getRenderTarget(), colour: renderer.getClearColor(new THREE.Color()), alpha: renderer.getClearAlpha() }
  // A see-through green, so the edges blend into leaves rather than black as the picture shrinks.
  renderer.setClearColor(0x3d5a30, 0)
  // Each render clears what it draws over, so each kind is kept to its own cell.
  target.scissorTest = true
  models.forEach((geo, k) => {
    const [half, bottom, top] = frames[k]
    camera.left = -half
    camera.right = half
    camera.bottom = bottom
    camera.top = top
    camera.updateProjectionMatrix()
    const mesh = new THREE.Mesh(geo, material)
    scene.add(mesh)
    target.viewport.set(k * CELL_W, 0, CELL_W, CELL_H)
    target.scissor.set(k * CELL_W, 0, CELL_W, CELL_H)
    renderer.setRenderTarget(target)
    renderer.render(scene, camera)
    scene.remove(mesh)
  })
  const pixels = new Uint8Array(width * CELL_H * 4)
  renderer.readRenderTargetPixels(target, 0, 0, width, CELL_H, pixels)
  renderer.setRenderTarget(before.target)
  renderer.setClearColor(before.colour, before.alpha)
  target.dispose()
  material.dispose()

  // The GPU's rows run bottom up; a canvas's top down.
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = CELL_H
  const ctx = canvas.getContext('2d')!
  const image = ctx.createImageData(width, CELL_H)
  for (let row = 0; row < CELL_H; row++) image.data.set(pixels.subarray(row * width * 4, (row + 1) * width * 4), (CELL_H - 1 - row) * width * 4)
  ctx.putImageData(image, 0, 0)
  return canvas
}

/** The 3D trees' material: draws only the trees nearer than their switching distance. */
export function treeMaterial(): THREE.MeshLambertMaterial {
  const material = new THREE.MeshLambertMaterial({ vertexColors: true, flatShading: true })
  material.onBeforeCompile = (shader) => {
    shader.vertexShader = shader.vertexShader.replace('#include <common>', `#include <common>\n${SPRITE_FROM_GLSL}`).replace(
      '#include <project_vertex>',
      `#include <project_vertex>
vec3 treeAt = (modelMatrix * instanceMatrix * vec4(0.0, 0.0, 0.0, 1.0)).xyz;
if (distance(treeAt.xz, cameraPosition.xz) > spriteFrom(treeAt)) gl_Position = vec4(2.0, 2.0, 2.0, 1.0);`
    )
  }
  material.customProgramCacheKey = () => 'trees-near'
  return material
}

/**
 * The sprites' material: each quad stands at its tree, turned about the vertical to face the
 * camera and sized by the tree's scale, and only beyond its switching distance. It is lit as
 * if facing the camera and a little upwards, much as a crown's surfaces are on average.
 */
export function spriteMaterial(texture: THREE.Texture): THREE.MeshLambertMaterial {
  const material = new THREE.MeshLambertMaterial({ map: texture, alphaTest: 0.5, side: THREE.DoubleSide })
  material.onBeforeCompile = (shader) => {
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', `#include <common>\n${SPRITE_FROM_GLSL}`)
      .replace(
        '#include <normal_vertex>',
        `vec3 spriteAt = (modelMatrix * instanceMatrix * vec4(0.0, 0.0, 0.0, 1.0)).xyz;
vec2 toCamera = cameraPosition.xz - spriteAt.xz;
vec2 facing = normalize(toCamera + vec2(1e-4, 0.0));
vec3 spriteRight = vec3(facing.y, 0.0, -facing.x);
vec2 spriteScale = vec2(length(instanceMatrix[0].xyz), length(instanceMatrix[1].xyz));
transformedNormal = normalize(mat3(viewMatrix) * vec3(facing.x * 0.6, 0.8, facing.y * 0.6));
#include <normal_vertex>`
      )
      .replace(
        '#include <project_vertex>',
        `vec4 mvPosition = viewMatrix * vec4(spriteAt + spriteRight * position.x * spriteScale.x + vec3(0.0, position.y * spriteScale.y, 0.0), 1.0);
gl_Position = projectionMatrix * mvPosition;
if (length(toCamera) <= spriteFrom(spriteAt)) gl_Position = vec4(2.0, 2.0, 2.0, 1.0);`
      )
  }
  material.customProgramCacheKey = () => 'trees-far'
  return material
}
