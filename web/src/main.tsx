import { createRoot } from 'react-dom/client'
import { App } from './App'
import { api } from './bridge'
import { rememberedTheme } from './state'
import './styles.css'
import './glass.css'

// Surface UI errors in the terminal the app was started from.
window.addEventListener('error', (e) => void api.log(`${e.message} (${e.filename}:${e.lineno})`))
window.addEventListener('unhandledrejection', (e) => void api.log(`unhandled rejection: ${e.reason}`))

// Start in the last theme, so there's no flash of the other one while the settings load.
document.documentElement.dataset.theme = rememberedTheme()

createRoot(document.getElementById('root')!).render(<App />)
