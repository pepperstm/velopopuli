//! Turning a diagnostics report into a GitHub issue: the project's "new issue" page,
//! opened in the browser with a title and the automatic summary filled in. The rider
//! signs in there as usual and drags the saved report file onto the page.

/// Where reports go. Forks change this.
pub const REPO: &str = "pepperstm/velopopuli";
/// Issue bodies are capped by GitHub at 65,536 characters.
const BODY_LIMIT: usize = 60_000;

/// Issue title from the rider's note (its first line), or a generic one.
pub fn title(note: &str) -> String {
    let first = note.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let mut t: String = first.chars().take(90).collect();
    if first.chars().count() > 90 {
        t.push('…');
    }
    if t.is_empty() {
        "Diagnostics report".into()
    } else {
        format!("Diagnostics: {t}")
    }
}

fn body(note: &str, summary: &str) -> String {
    let mut b = String::new();
    b.push_str("### What happened\n\n");
    b.push_str(if note.trim().is_empty() { "_No description given._" } else { note.trim() });
    b.push_str("\n\n### Automatic summary\n\n```\n");
    let room = BODY_LIMIT.saturating_sub(b.len() + 400);
    let mut s: String = summary.chars().take(room).collect();
    if s.len() < summary.len() {
        s.push_str("\n… (cut short; see the full report)");
    }
    b.push_str(&s);
    b.push_str("\n```\n\n");
    b.push_str(&format!("_Sent from VeloPopuli {} with the diagnostics reporter._\n", env!("CARGO_PKG_VERSION")));
    b
}

/// GitHub's "new issue" page with the title and summary filled in, for attaching the saved file by hand.
pub fn issue_url(note: &str, summary: &str) -> String {
    // Browsers and GitHub cope with addresses up to about 8 KB.
    let mut b = body(note, summary);
    b.push_str("\n**Full report:** _drag the saved diagnostics file here_\n");
    let mut encoded = encode(&b);
    while encoded.len() > 7000 && b.len() > 200 {
        b = format!("{}\n…\n\n**Full report:** _drag the saved diagnostics file here_\n", b.chars().take(b.chars().count() * 3 / 4).collect::<String>());
        encoded = encode(&b);
    }
    format!("https://github.com/{REPO}/issues/new?labels=diagnostics&title={}&body={encoded}", encode(&title(note)))
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_urls() {
        assert_eq!(title(""), "Diagnostics report");
        assert_eq!(title("\n  Vortex drops out\nmore"), "Diagnostics: Vortex drops out");
        let url = issue_url("Power 0 & drops", &"x".repeat(20_000));
        assert!(url.starts_with("https://github.com/pepperstm/velopopuli/issues/new?labels=diagnostics&title=Diagnostics%3A%20Power%200%20%26%20drops&body="));
        assert!(url.len() < 7600, "{}", url.len());
        assert!(url.contains("drag%20the%20saved%20diagnostics%20file%20here"));
    }

    #[test]
    fn body_fits_github_limit() {
        let b = body("note", &"y".repeat(200_000));
        assert!(b.len() <= BODY_LIMIT);
        assert!(b.contains("cut short"));
    }
}
