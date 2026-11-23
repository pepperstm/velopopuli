/**
 * The app's sounds, made on the fly with the Web Audio API: no files to download or license.
 * Brass for the summit fanfare, bells for bests and laps, plain beeps for workout countdowns.
 */

let ctx: AudioContext | null = null
let master: GainNode | null = null
let level = 0.6

/** The audio graph, made on first use. Null where the page can't make sound. */
function out(): { ctx: AudioContext; master: GainNode } | null {
  if (!ctx) {
    try {
      ctx = new AudioContext()
    } catch {
      return null
    }
    // A gentle limiter, so chords and drums together never clip.
    const limiter = ctx.createDynamicsCompressor()
    limiter.threshold.value = -10
    limiter.knee.value = 8
    limiter.ratio.value = 6
    limiter.attack.value = 0.003
    limiter.release.value = 0.2
    master = ctx.createGain()
    master.gain.value = level
    master.connect(limiter).connect(ctx.destination)
  }
  // A page may start with its audio suspended until it's used.
  if (ctx.state === 'suspended') ctx.resume().catch(() => undefined)
  return { ctx, master: master! }
}

/** Sets the overall volume, 0 to 1. */
export function setVolume(volume: number): void {
  level = Math.max(0, Math.min(1, volume))
  if (master && ctx) master.gain.setTargetAtTime(level, ctx.currentTime, 0.02)
}

/** A note's frequency from its MIDI number (60 is middle C). */
const hz = (midi: number): number => 440 * 2 ** ((midi - 69) / 12)

/** An envelope on `param`: up to `peak` in `attack`, held, then down over `release`. */
function envelope(param: AudioParam, at: number, peak: number, attack: number, hold: number, release: number): void {
  param.setValueAtTime(0.0001, at)
  param.exponentialRampToValueAtTime(peak, at + attack)
  param.setValueAtTime(peak, at + attack + hold)
  param.exponentialRampToValueAtTime(0.0001, at + attack + hold + release)
}

/** A simple tone: a beep or a click. */
function tone(at: number, freq: number, length: number, gain: number, type: OscillatorType = 'triangle'): void {
  const a = out()
  if (!a) return
  const osc = a.ctx.createOscillator()
  const amp = a.ctx.createGain()
  osc.type = type
  osc.frequency.value = freq
  envelope(amp.gain, at, gain, 0.005, Math.max(0, length - 0.06), 0.05)
  osc.connect(amp).connect(a.master)
  osc.start(at)
  osc.stop(at + length + 0.1)
}

/** A bell: a bright fundamental with an inharmonic partial that fades first. */
function bell(at: number, midi: number, gain: number, ring = 1.2): void {
  const a = out()
  if (!a) return
  for (const [ratio, share, decay] of [
    [1, 1, ring],
    [2.76, 0.35, ring * 0.45],
    [5.4, 0.12, ring * 0.25]
  ]) {
    const osc = a.ctx.createOscillator()
    const amp = a.ctx.createGain()
    osc.type = 'sine'
    osc.frequency.value = hz(midi) * ratio
    envelope(amp.gain, at, gain * share, 0.004, 0, decay)
    osc.connect(amp).connect(a.master)
    osc.start(at)
    osc.stop(at + decay + 0.1)
  }
}

/**
 * A brass note: two slightly detuned saws and a square an octave down, through a filter that
 * opens as the note speaks (the brassy "blat") and settles, with vibrato on long notes.
 */
function brass(at: number, midi: number, length: number, gain: number, into?: AudioNode): void {
  const a = out()
  if (!a) return
  const filter = a.ctx.createBiquadFilter()
  filter.type = 'lowpass'
  filter.Q.value = 1.5
  filter.frequency.setValueAtTime(500, at)
  filter.frequency.exponentialRampToValueAtTime(3800, at + 0.06)
  filter.frequency.exponentialRampToValueAtTime(2200, at + 0.25)
  const amp = a.ctx.createGain()
  envelope(amp.gain, at, gain, 0.035, Math.max(0, length - 0.1), 0.12)
  filter.connect(amp).connect(into ?? a.master)
  const vibrato = a.ctx.createOscillator()
  const depth = a.ctx.createGain()
  vibrato.frequency.value = 5.5
  depth.gain.setValueAtTime(0, at)
  depth.gain.linearRampToValueAtTime(length > 0.5 ? hz(midi) * 0.006 : 0, at + Math.min(0.4, length))
  vibrato.connect(depth)
  const voices: [OscillatorType, number, number, number][] = [
    ['sawtooth', 1, -6, 0.5],
    ['sawtooth', 1, 6, 0.5],
    ['square', 0.5, 0, 0.18]
  ]
  for (const [type, ratio, detune, share] of voices) {
    const osc = a.ctx.createOscillator()
    const voice = a.ctx.createGain()
    osc.type = type
    osc.frequency.value = hz(midi) * ratio
    osc.detune.value = detune
    depth.connect(osc.frequency)
    voice.gain.value = share
    osc.connect(voice).connect(filter)
    osc.start(at)
    osc.stop(at + length + 0.2)
  }
  vibrato.start(at)
  vibrato.stop(at + length + 0.2)
}

/** A held brass note that starts a little back and swells, for the fanfare's closing chords. */
function swell(at: number, midi: number, length: number, gain: number): void {
  const a = out()
  if (!a) return
  // The note itself at a steady level, through a gain that grows from 70% to full.
  const grow = a.ctx.createGain()
  grow.gain.setValueAtTime(0.7, at)
  grow.gain.linearRampToValueAtTime(1, at + length * 0.85)
  grow.connect(a.master)
  brass(at, midi, length, gain, grow)
}

/** Shaped noise, for drums and cymbals. */
function noise(at: number, length: number, gain: number, filterType: BiquadFilterType, freq: number): void {
  const a = out()
  if (!a) return
  const frames = Math.ceil(a.ctx.sampleRate * length)
  const buffer = a.ctx.createBuffer(1, frames, a.ctx.sampleRate)
  const data = buffer.getChannelData(0)
  for (let i = 0; i < frames; i++) data[i] = Math.random() * 2 - 1
  const src = a.ctx.createBufferSource()
  src.buffer = buffer
  const filter = a.ctx.createBiquadFilter()
  filter.type = filterType
  filter.frequency.value = freq
  const amp = a.ctx.createGain()
  envelope(amp.gain, at, gain, 0.003, 0, length)
  src.connect(filter).connect(amp).connect(a.master)
  src.start(at)
}

/** A timpani stroke: a falling low sine and a soft thud. */
function timpani(at: number, midi: number, gain: number): void {
  const a = out()
  if (!a) return
  const osc = a.ctx.createOscillator()
  const amp = a.ctx.createGain()
  osc.type = 'sine'
  osc.frequency.setValueAtTime(hz(midi) * 1.5, at)
  osc.frequency.exponentialRampToValueAtTime(hz(midi), at + 0.08)
  envelope(amp.gain, at, gain, 0.004, 0.02, 0.9)
  osc.connect(amp).connect(a.master)
  osc.start(at)
  osc.stop(at + 1.1)
  noise(at, 0.12, gain * 0.35, 'lowpass', 400)
}

const now = (): number => out()?.ctx.currentTime ?? 0

/** The last seconds of a workout step: a short beep each second. */
export function countdown(): void {
  tone(now(), 880, 0.14, 0.7)
}

/** A new workout step starts: a longer, higher beep. */
export function stepStart(): void {
  tone(now(), 1320, 0.36, 0.75)
}

/** A lap done: two bells. */
export function lap(): void {
  const t = now()
  bell(t, 88, 0.3)
  bell(t + 0.14, 93, 0.3)
}

/** A timed segment done, without a best: a soft three-note chime. */
export function chime(): void {
  const t = now()
  ;[84, 88, 91].forEach((m, i) => bell(t + i * 0.12, m, 0.22, 0.9))
}

/** A new personal best: a quick sparkle of bells up two octaves. */
export function personalBest(delay = 0): void {
  const t = now() + delay
  ;[84, 88, 91, 96, 100, 103, 108].forEach((m, i) => bell(t + i * 0.07, m, 0.2, 1.4))
}

/** Something's wrong (a sensor dropped out): a soft falling two-note tone. */
export function warning(): void {
  const t = now()
  tone(t, hz(69), 0.22, 0.25, 'sine')
  tone(t + 0.24, hz(64), 0.34, 0.25, 'sine')
}

/** A tiny click, as feedback for a handlebar-controller press. */
export function tick(): void {
  tone(now(), 2400, 0.03, 0.3, 'square')
}

/** A snare drum stroke: a crack of bright noise. */
function snare(at: number, gain: number): void {
  noise(at, 0.09, gain, 'bandpass', 2600)
  noise(at, 0.05, gain * 0.6, 'highpass', 5000)
}

/** A cymbal crash: a long wash of bright noise with a hard edge. */
function crash(at: number, gain: number, length = 2.4): void {
  noise(at, length, gain, 'highpass', 5200)
  noise(at, 0.5, gain * 0.8, 'bandpass', 3200)
}

/** The whole section hits one chord together (MIDI notes), the low notes softer. */
function stab(at: number, notes: number[], length: number, gain: number, held = false): void {
  for (const m of notes) {
    const level = m >= 72 ? gain : m >= 60 ? gain * 0.75 : gain * 0.55
    if (held) swell(at, m, length, level)
    else brass(at, m, length, level)
  }
}

/** A major chord on `root` (MIDI), spread across four octaves for the full section. */
const big = (root: number): number[] => [root - 24, root - 12, root - 5, root, root + 4, root + 7, root + 12, root + 16, root + 19, root + 24]

/** A long held chord: swelling brass, a trumpet trill on top, a cymbal crash, and a timpani roll
 * that builds to a last "ta-DAA" from the whole section. Returns when it ends. */
function finale(at: number, root: number, hold: number, gain: number): number {
  stab(at, big(root), hold, gain, true)
  // The top trumpet trills between the octave and the note above it, quickening.
  for (let k = 0, x = at + 0.25; x < at + hold - 0.35; k++) {
    const step = Math.max(0.055, 0.11 - k * 0.004)
    brass(x, root + 24 + (k % 2 === 0 ? 0 : 2), step * 0.9, gain * 0.5)
    x += step
  }
  crash(at, 0.34, hold + 0.8)
  for (let x = at; x < at + hold - 0.2; x += 0.055) timpani(x, root - 24, 0.12 + ((x - at) / hold) * 0.36)
  // The stinger: a short hit, then the last chord, cut clean, with a final crash.
  const sting = at + hold
  stab(sting, big(root), 0.12, gain * 1.1)
  timpani(sting, root - 24, 0.7)
  snare(sting, 0.4)
  stab(sting + 0.22, big(root), 0.55, gain * 1.2)
  timpani(sting + 0.22, root - 29, 0.8)
  snare(sting + 0.22, 0.45)
  crash(sting + 0.22, 0.4, 1.8)
  return sting + 0.8
}

/**
 * The summit fanfare, as bold as it gets: a snare roll builds into a brass call (G G G, up to C)
 * doubled across octaves, a rising answer stabbed by the whole section, then a huge swelling C
 * major chord with a trumpet trill, cymbals and a timpani roll, ending in a "ta-DAA" stinger.
 * About four seconds. `grand`, for the end of a ride, adds the heroic ending: the chords on
 * A flat and B flat, then home to an even bigger C.
 */
export function fanfare(grand = false): void {
  const t = now() + 0.05
  const g = 0.17
  // A snare roll that builds.
  const roll = 0.45
  for (let x = 0; x < roll; x += 0.035) snare(t + x, 0.08 + (x / roll) * 0.22)
  // The call: G G G, then up to a held C, the whole section on each note.
  const call = t + roll
  const beat = 0.12
  for (let i = 0; i < 3; i++) {
    stab(call + i * beat, [43, 55, 67, 79], beat * 0.8, g)
    timpani(call + i * beat, 43, 0.45)
    snare(call + i * beat, 0.3)
  }
  const c = call + 3 * beat
  stab(c, [48, 55, 60, 64, 72, 76, 84], 0.36, g)
  timpani(c, 36, 0.6)
  crash(c, 0.22, 0.8)
  // The answer: up through E and G, each a chord, driving into the finale.
  const up = c + 0.4
  stab(up, [52, 60, 64, 67, 76, 88], 0.16, g * 1.1)
  stab(up + 0.17, [55, 62, 67, 71, 79, 91], 0.16, g * 1.15)
  timpani(up, 40, 0.5)
  timpani(up + 0.17, 43, 0.55)
  snare(up, 0.3)
  snare(up + 0.17, 0.35)
  const end = finale(up + 0.36, 60, 1.3, g)
  if (grand) {
    // The heroic ending: A flat, B flat, then C, each bigger than the last.
    const flat6 = end + 0.15
    stab(flat6, big(56), 0.5, g * 1.05, true)
    timpani(flat6, 32, 0.7)
    crash(flat6, 0.3, 0.9)
    const flat7 = flat6 + 0.6
    stab(flat7, big(58), 0.5, g * 1.1, true)
    timpani(flat7, 34, 0.75)
    crash(flat7, 0.32, 0.9)
    for (let x = flat7 + 0.3; x < flat7 + 0.6; x += 0.03) snare(x, 0.15 + ((x - flat7 - 0.3) / 0.3) * 0.25)
    finale(flat7 + 0.6, 60, 2.0, g * 1.15)
  }
}

/** How long `fanfare()` takes, in seconds, so a best can follow it. */
export const FANFARE_S = 4.1
