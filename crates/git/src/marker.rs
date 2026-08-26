//! Generated-commit markers for bidirectional sync (M12).
//!
//! When RepoSync writes a commit to a *mirror* repository (the destination of a
//! migration, or the public side of a two-way sync), it embeds a trailer in the
//! commit message naming the source commit it was derived from:
//!
//! ```text
//! RepoSync-Generated: <source-commit-id>
//! ```
//!
//! On a reverse sync, parsing this trailer lets us recognize our own mirrored
//! output and refuse to re-import it — the core of loop prevention.

use reposync_core::CommitId;

/// Trailer key embedded in generated (mirror) commit messages.
pub const GENERATED_TRAILER: &str = "RepoSync-Generated";

/// Returns `message` with a `RepoSync-Generated: <source>` trailer appended.
#[must_use]
pub fn mark_generated(message: &str, source: &CommitId) -> String {
    format!("{message}\n\n{GENERATED_TRAILER}: {}", source.as_str())
}

/// If `message` is a generated (mirror) commit, returns the source commit id it
/// was derived from.
#[must_use]
pub fn generated_source(message: &str) -> Option<CommitId> {
    let prefix = format!("{GENERATED_TRAILER}:");
    for line in message.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            let id = rest.trim();
            if !id.is_empty() {
                return CommitId::new(id).ok();
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_source_id() {
        let id = CommitId::new("abc123").unwrap();
        let marked = mark_generated("subject\n\nbody", &id);
        assert!(marked.contains("RepoSync-Generated: abc123"));
        assert_eq!(generated_source(&marked), Some(id));
    }

    #[test]
    fn returns_none_for_plain_messages() {
        assert_eq!(generated_source("just a normal commit"), None);
        assert_eq!(generated_source(""), None);
    }
}
