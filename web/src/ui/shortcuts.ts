import type { Settings } from '../types'

/**
 * Every keyboard shortcut, in one place: the ride screen's key handling reads `rideKeyAction`,
 * and the help panel lists `rideShortcuts`, so the two can't drift apart.
 */

/** What a key does on the ride screen. */
export type RideKeyAction = 'shiftHarder' | 'shiftEasier' | 'pause' | 'camera' | 'overlay' | 'harder' | 'easier' | 'help'

/** One row of the help panel. */
export interface Shortcut {
  /** The keys, as `KeyboardEvent.key` values (shown nicely by `keyName`). Empty: no key, only a button or controller. */
  keys: string[]
  /** What it does. */
  label: string
  /** The handlebar-controller button that does the same, if there is one. */
  controller?: string
}

export interface ShortcutSection {
  title: string
  rows: Shortcut[]
}

/** The fixed ride keys; the gear keys are the rider's own, from Settings. */
const RIDE_KEYS: Record<Exclude<RideKeyAction, 'shiftHarder' | 'shiftEasier'>, string[]> = {
  pause: [' '],
  camera: ['c', 'C'],
  overlay: ['o', 'O'],
  harder: ['ArrowUp'],
  easier: ['ArrowDown'],
  help: ['?']
}

/** The ride action for a key, or null. The rider's own gear keys come first, so any key can be used. */
export function rideKeyAction(key: string, settings: Settings): RideKeyAction | null {
  const gears = settings.gears
  if (gears.mode === 'virtual' && gears.keysHarder.includes(key)) return 'shiftHarder'
  if (gears.mode === 'virtual' && gears.keysEasier.includes(key)) return 'shiftEasier'
  for (const [action, keys] of Object.entries(RIDE_KEYS)) if (keys.includes(key)) return action as RideKeyAction
  return null
}

/** What the ride is doing, so ↑/↓ are described for what they do right now. */
export interface RideContext {
  workout: boolean
  erg: boolean
  demo: boolean
}

/** The ride screen's shortcuts that work right now. */
export function rideShortcuts(settings: Settings, ride: RideContext | null): ShortcutSection {
  const gears = settings.gears
  const rows: Shortcut[] = []
  if (gears.mode === 'virtual') {
    rows.push({ keys: gears.keysHarder, label: 'Harder gear', controller: 'Shift up' })
    rows.push({ keys: gears.keysEasier, label: 'Easier gear', controller: 'Shift down' })
  }
  const upDown = !ride
    ? ['Harder: more workout intensity, ERG watts or demo effort', 'Easier: less of the same']
    : ride.workout
      ? ['Workout intensity up 5%', 'Workout intensity down 5%']
      : ride.erg
        ? ['ERG target up 10 W', 'ERG target down 10 W']
        : ride.demo
          ? ['Demo rider effort up 10 W', 'Demo rider effort down 10 W']
          : null
  if (upDown) {
    rows.push({ keys: RIDE_KEYS.harder, label: upDown[0], controller: 'Increase difficulty' })
    rows.push({ keys: RIDE_KEYS.easier, label: upDown[1], controller: 'Decrease difficulty' })
  }
  rows.push({ keys: RIDE_KEYS.pause, label: 'Pause or resume', controller: 'Pause / Resume' })
  rows.push({ keys: RIDE_KEYS.camera.slice(0, 1), label: 'Change the camera', controller: 'Camera view' })
  rows.push({ keys: RIDE_KEYS.overlay.slice(0, 1), label: 'Overlay mode: float over another app' })
  if (!ride || ride.workout) rows.push({ keys: [], label: 'Skip to the next workout step', controller: 'Skip interval' })
  rows.push({ keys: RIDE_KEYS.help, label: 'Show or hide this list' })
  return { title: 'Riding', rows }
}

const mac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent)

/** The route planner's shortcuts (Explore → plan a route). */
export function plannerShortcuts(): ShortcutSection {
  return {
    title: 'Planning a route',
    rows: [
      { keys: [mac ? '⌘Z' : 'Ctrl+Z'], label: 'Undo' },
      { keys: [mac ? '⇧⌘Z' : 'Ctrl+Shift+Z'], label: 'Redo' },
      { keys: ['Enter'], label: 'Search for a place, in the search box' }
    ]
  }
}

/** Shortcuts that work in any window. */
export function generalShortcuts(): ShortcutSection {
  return { title: 'Anywhere', rows: [{ keys: ['Escape'], label: 'Close a window like this one' }] }
}

const NAMES: Record<string, string> = {
  ' ': 'Space',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  PageUp: 'Page Up',
  PageDown: 'Page Down',
  Escape: 'Esc'
}

/** A key as the rider sees it on the keyboard. */
export function keyName(key: string): string {
  return NAMES[key] ?? (key.length === 1 ? key.toUpperCase() : key)
}
