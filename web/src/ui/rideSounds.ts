import * as sound from '../core/sound'
import type { RideState, Settings } from '../types'

/** A step this short gets no countdown: it would be beeping the whole time. */
const COUNTDOWN_MIN_STEP_S = 8

/**
 * Watches the ride's state as it arrives (20 times a second) and plays a sound when something
 * happens: the last seconds of a workout step, a new step, a summit, a best, a lap, the finish,
 * a sensor dropping out. Remembers what it has already announced, so nothing plays twice.
 */
export class RideSounds {
  private first = true
  private step = -1
  private stepLength = 0
  private beeped = Infinity
  private hadWorkout = false
  /** Climb and segment results already announced. */
  private announced = new Set<string>()
  private lap = 1
  private finished = false
  private noSensors = false

  constructor(private settings: () => Settings['sound']) {}

  update(s: RideState): void {
    const cfg = this.settings()
    sound.setVolume(cfg.volume)
    // The first state sets the starting point: nothing is announced for how the ride began.
    if (this.first) {
      this.first = false
      this.step = s.workout?.index ?? -1
      this.stepLength = s.workout?.stepLeftS ?? 0
      this.hadWorkout = s.workout !== null
      for (const key of [climbKey(s), segmentKey(s)]) if (key) this.announced.add(key)
      this.lap = s.lap
      this.finished = s.finished
      this.noSensors = s.noSensors
      return
    }
    const quiet = cfg.volume <= 0

    // Workout: a beep in each of the last three seconds of a step, and a tone as the next starts.
    if (s.workout) {
      if (s.workout.index !== this.step) {
        if (this.step >= 0 && cfg.workout && !quiet) sound.stepStart()
        this.step = s.workout.index
        this.stepLength = s.workout.stepLeftS
        this.beeped = Infinity
      }
      const left = Math.ceil(s.workout.stepLeftS)
      if (left >= 1 && left <= 3 && left < this.beeped && !s.paused && this.stepLength >= COUNTDOWN_MIN_STEP_S) {
        this.beeped = left
        if (cfg.workout && !quiet) sound.countdown()
      }
      this.hadWorkout = true
    } else if (this.hadWorkout) {
      // The last step is done.
      this.hadWorkout = false
      if (cfg.workout && !quiet) sound.chime()
    }

    const moments = cfg.moments && !quiet
    // The finish outranks everything else that happens on the same line.
    if (s.finished && !this.finished) {
      this.finished = true
      if (moments) sound.fanfare(true)
      return
    }

    // A summit: the fanfare, then the bells too if it was a best.
    const climb = climbKey(s)
    if (climb && !this.announced.has(climb) && s.climbResult) {
      this.announced.add(climb)
      const r = s.climbResult
      if (moments) {
        sound.fanfare()
        if (r.previousBest !== null && r.timeS < r.previousBest) sound.personalBest(sound.FANFARE_S)
      }
    }

    // A timed segment from a connected service (a climb's own timing is the summit above).
    const segment = segmentKey(s)
    if (segment && !this.announced.has(segment) && s.segmentResult) {
      this.announced.add(segment)
      const r = s.segmentResult
      if (moments) {
        if (r.previousBest !== null && r.timeS < r.previousBest) sound.personalBest()
        else sound.chime()
      }
    }

    if (s.lap > this.lap && moments) sound.lap()
    this.lap = s.lap

    if (s.noSensors && !this.noSensors && moments) sound.warning()
    this.noSensors = s.noSensors
  }
}

/** Identifies a climb result, so the same summit isn't celebrated twice. */
function climbKey(s: RideState): string {
  return s.climbResult ? `${s.climbResult.index}:${s.climbResult.timeS}` : ''
}

function segmentKey(s: RideState): string {
  const r = s.segmentResult
  return r && r.source === 'remote' ? `${r.id}:${r.timeS}` : ''
}
