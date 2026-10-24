import * as THREE from 'three'

/** How wet the roads are and how much snow lies, 0–1, shared by every weathered material. */
export const weatherUniforms = { wet: { value: 0 }, snow: { value: 0 } }

/**
 * Lets the weather show on a material: lying snow whitens whatever faces the sky enough
 * (`snowFrom`: the upward share of a surface's normal where snow starts to settle, or null
 * for none), and rain darkens it (by `wets` at most: roads a lot, the ground a little).
 * Keeps any shader changes the material already has.
 */
export function weathered<M extends THREE.Material>(material: M, wets: number, snowFrom: number | null): M {
  const before = material.onBeforeCompile.bind(material)
  const key = material.customProgramCacheKey.bind(material)
  material.onBeforeCompile = (shader, renderer) => {
    before(shader, renderer)
    shader.uniforms.wet = weatherUniforms.wet
    shader.uniforms.snow = weatherUniforms.snow
    shader.vertexShader = shader.vertexShader
      .replace('#include <common>', '#include <common>\nvarying float vUp;')
      .replace('#include <beginnormal_vertex>', '#include <beginnormal_vertex>\nvUp = normalize(mat3(modelMatrix) * objectNormal).y;')
    const snow =
      snowFrom === null
        ? ''
        : `diffuseColor.rgb = mix(diffuseColor.rgb, vec3(0.92, 0.94, 0.97), snow * smoothstep(${snowFrom.toFixed(2)}, ${(snowFrom + 0.3).toFixed(2)}, vUp));`
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', '#include <common>\nvarying float vUp;\nuniform float wet;\nuniform float snow;')
      // After the colour is settled (whatever another change made of it), before lighting.
      .replace('#include <alphamap_fragment>', `diffuseColor.rgb *= 1.0 - ${wets.toFixed(2)} * wet;\n${snow}\n#include <alphamap_fragment>`)
  }
  material.customProgramCacheKey = () => `${key()}|weathered-${wets}-${snowFrom}`
  return material
}
