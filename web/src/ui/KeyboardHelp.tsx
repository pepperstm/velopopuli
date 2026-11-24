import { useEffect } from 'react'
import { api } from '../state'
import { keyName, type ShortcutSection } from './shortcuts'

const WIKI = 'https://github.com/pepperstm/velopopuli/wiki/Keyboard-shortcuts'

/** The keyboard shortcuts that work right now, with the controller buttons that do the same. */
export function KeyboardHelp(props: { sections: ShortcutSection[]; onClose: () => void; className?: string }) {
  const { onClose } = props
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      e.stopPropagation()
      onClose()
    }
    window.addEventListener('keydown', onKey, { capture: true })
    return () => window.removeEventListener('keydown', onKey, { capture: true })
  }, [onClose])
  const controllers = props.sections.some((s) => s.rows.some((r) => r.controller))
  return (
    <div className={props.className ?? 'modal-backdrop'} onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal keyboard-help" role="dialog" aria-label="Keyboard shortcuts">
        <h2>Keyboard shortcuts</h2>
        <table>
          {props.sections.map((section) => (
            <tbody key={section.title}>
              {props.sections.length > 1 && (
                <tr>
                  <th colSpan={controllers ? 3 : 2}>{section.title}</th>
                </tr>
              )}
              {section.rows.map((row) => (
                <tr key={row.label}>
                  <td className="keys">
                    {row.keys.length ? (
                      row.keys.map((k, i) => (
                        <span key={k}>
                          {i > 0 && ' '}
                          <kbd>{keyName(k)}</kbd>
                        </span>
                      ))
                    ) : (
                      <span className="muted">Button</span>
                    )}
                  </td>
                  <td>{row.label}</td>
                  {controllers && <td className="muted controller">{row.controller ?? ''}</td>}
                </tr>
              ))}
            </tbody>
          ))}
        </table>
        {controllers && <p className="muted small">The last column is the handlebar-controller button that does the same.</p>}
        <p className="muted small">
          Change the gear keys under Settings → Gears. More in the{' '}
          <a
            href="#"
            onClick={(e) => {
              e.preventDefault()
              void api.openExternal(WIKI)
            }}
          >
            Keyboard shortcuts guide
          </a>
          .
        </p>
        <div className="row">
          <button className="primary" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  )
}
