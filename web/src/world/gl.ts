import * as THREE from 'three'
import { api } from '../bridge'

/** What the 3D view asks for, best first. Some older Macs refuse the first and accept a plainer one. */
const ATTEMPTS: { label: string; antialias: boolean; powerPreference: WebGLPowerPreference }[] = [
  { label: 'high-performance, antialiased', antialias: true, powerPreference: 'high-performance' },
  { label: 'default GPU, antialiased', antialias: true, powerPreference: 'default' },
  { label: 'default GPU, no antialiasing', antialias: false, powerPreference: 'default' },
  { label: 'low-power GPU, no antialiasing', antialias: false, powerPreference: 'low-power' }
]

const log = (message: string): void => void api.log(`3d: ${message}`).catch(() => undefined)

/**
 * The 3D view's renderer, trying plainer settings when the first are refused. Each refusal is
 * written to the diagnostics log with the reason the web view gives, and so is the GPU that
 * was used, so a report says what happened.
 */
export function createRenderer(canvas: HTMLCanvasElement): THREE.WebGLRenderer {
  const reasons: string[] = []
  for (const attempt of ATTEMPTS) {
    // The web view says why it refused in this event, which fires before getContext returns null.
    let reason = ''
    const onError = (e: Event): void => {
      reason = (e as WebGLContextEvent).statusMessage
    }
    canvas.addEventListener('webglcontextcreationerror', onError)
    try {
      const renderer = new THREE.WebGLRenderer({ canvas, antialias: attempt.antialias, powerPreference: attempt.powerPreference })
      log(`WebGL 2 context: ${attempt.label}; ${gpu(renderer.getContext())}`)
      return renderer
    } catch (err) {
      const why = reason || (err instanceof Error ? err.message : String(err))
      reasons.push(`${attempt.label}: ${why}`)
      log(`WebGL 2 refused (${attempt.label}): ${why}`)
    } finally {
      canvas.removeEventListener('webglcontextcreationerror', onError)
    }
  }
  log(`no WebGL 2 context. WebGL 1 on a test canvas: ${webgl1()}. ${navigator.userAgent}`)
  throw new Error(
    "The 3D view couldn't start: this computer's web view refused a WebGL 2 context. " +
      'Please send a diagnostics report (Settings → Diagnostics) so we can see why. ' +
      `(${reasons.join('; ')})`
  )
}

/** The GPU and driver behind a context, when the web view says. */
function gpu(gl: WebGLRenderingContext | WebGL2RenderingContext): string {
  const info = gl.getExtension('WEBGL_debug_renderer_info')
  const vendor = info ? gl.getParameter(info.UNMASKED_VENDOR_WEBGL) : gl.getParameter(gl.VENDOR)
  const renderer = info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER)
  return `${vendor} ${renderer} (${gl.getParameter(gl.VERSION)})`
}

/** Whether WebGL 1 works here at all, on a canvas of its own: it tells a WebGL 2 problem from no WebGL. */
function webgl1(): string {
  try {
    const gl = document.createElement('canvas').getContext('webgl')
    if (!gl) return 'refused'
    const result = `works, ${gpu(gl)}`
    gl.getExtension('WEBGL_lose_context')?.loseContext()
    return result
  } catch (err) {
    return `failed: ${err instanceof Error ? err.message : String(err)}`
  }
}
