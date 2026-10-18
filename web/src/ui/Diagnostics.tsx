import { useState } from 'react'
import { errorText } from '../bridge'
import { api } from '../state'

/** A button that saves a diagnostics report, after asking what went wrong. */
export function ReportButton({
  label = 'Report a problem…',
  className = 'ghost',
  prompt = ''
}: {
  label?: string
  className?: string
  prompt?: string
}): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const [note, setNote] = useState(prompt)
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState('')

  const save = async (toGitHub: boolean): Promise<void> => {
    setBusy(true)
    setResult('')
    try {
      const path = toGitHub ? await api.diag.issue(note) : await api.diag.report(note)
      if (path && toGitHub) {
        setResult(`Saved to ${path}. GitHub's new-issue page is open with the summary filled in: drag the file (shown in Finder) onto it, then press Create.`)
      } else if (path) {
        setResult(`Saved to ${path}. Attach this file to your bug report, or send it to whoever is helping you.`)
      }
    } catch (err) {
      setResult(errorText(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <button className={className} onClick={() => setOpen(true)}>
        {label}
      </button>
      {open && (
        <div className="modal-backdrop">
          <div className="modal">
            <h2>Save a diagnostics report</h2>
            <p className="muted small">
              One text file with what your sensors and trainer did over the last couple of days: connections, drop-outs, the data they sent and what was sent to
              them, plus a summary at the top. It has no account details, tokens or locations.
            </p>
            <label className="field stack">
              <span>What happened? (optional, but it helps)</span>
              <textarea
                rows={4}
                value={note}
                placeholder="e.g. Tacx Vortex over Bluetooth: power dropped to 0 every few minutes while I kept pedalling."
                onChange={(e) => setNote(e.target.value)}
              />
            </label>
            <p className="muted small">GitHub issues are public: anyone can read the summary and the attached file.</p>
            {result && <p className="small">{result}</p>}
            <div className="row">
              <button className="primary" disabled={busy} onClick={() => void save(true)}>
                {busy ? 'Saving…' : 'Report on GitHub…'}
              </button>
              <button disabled={busy} onClick={() => void save(false)}>
                Just save the report…
              </button>
              <button className="ghost" onClick={() => setOpen(false)}>
                Close
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  )
}
