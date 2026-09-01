//! Post-insert correction capture: noticing what the user changed about the
//! text una just pasted, so it can become a training pair.
//!
//! Two independent pieces, both pure so they can be tested without a GUI or
//! an Objective-C runtime:
//!
//! - [`edited_region_text`] — the accessibility path. Given the field's text
//!   right after the paste, its text now, and the character range the paste
//!   occupied, it recovers what the pasted span has *become*, and refuses
//!   (returns `None`) when the edit landed outside that span. Editing the
//!   sentence you wrote yourself two lines down must never be mistaken for
//!   correcting a dictation.
//! - [`EditCounter`] — the keystroke path, used where accessibility cannot
//!   read the field (terminals, canvas editors). It tracks how long the
//!   pasted span currently is by counting the keys pressed since the paste,
//!   and marks itself invalid the moment the user does something that breaks
//!   the count (clicks elsewhere, arrows around, undoes). That length is only
//!   ever used to decide how many characters to replace when writing a
//!   correction back; when it is invalid, the correction is still recorded,
//!   it just isn't pasted back.

use serde::{Deserialize, Serialize};

/// A character range inside a text field. Offsets are `char` counts, not
/// bytes and not UTF-16 code units — the platform layer converts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub start: usize,
    pub len: usize,
}

impl Region {
    pub fn new(start: usize, len: usize) -> Self {
        Self { start, len }
    }

    pub fn end(&self) -> usize {
        self.start + self.len
    }
}

/// What the pasted region has become, or `None` when the change doesn't
/// belong to it.
///
/// `before` is the whole field as it read immediately after the paste,
/// `after` is the whole field now, and `region` is where the paste landed in
/// `before`. Returns `Some("")` when the region was deleted outright — that
/// is a real signal (the user threw the dictation away), not a failure.
pub fn edited_region_text(before: &str, after: &str, region: Region) -> Option<String> {
    let b: Vec<char> = before.chars().collect();
    let a: Vec<char> = after.chars().collect();
    if region.end() > b.len() {
        return None; // stale snapshot; the region can't be located
    }
    if b == a {
        return Some(b[region.start..region.end()].iter().collect());
    }

    // The changed span is whatever sits between the common prefix and the
    // common suffix. Both are clamped so they cannot overlap in the shorter
    // string (e.g. "abc" -> "abbc" has prefix 2 and suffix 2, which would
    // double-count the 'b').
    let prefix = b
        .iter()
        .zip(a.iter())
        .take_while(|(x, y)| x == y)
        .count();
    let max_suffix = b.len().min(a.len()) - prefix;
    let suffix = b
        .iter()
        .rev()
        .zip(a.iter().rev())
        .take_while(|(x, y)| x == y)
        .count()
        .min(max_suffix);

    let changed_start = prefix;
    let changed_end_before = b.len() - suffix;
    let changed_end_after = a.len() - suffix;

    // The edit has to live entirely inside the pasted span. Anything else is
    // the user working on their own text.
    if changed_start < region.start || changed_end_before > region.end() {
        return None;
    }

    let head: String = b[region.start..changed_start].iter().collect();
    let middle: String = a[changed_start..changed_end_after].iter().collect();
    let tail: String = b[changed_end_before..region.end()].iter().collect();
    Some(format!("{head}{middle}{tail}"))
}

/// A key press, as far as correction tracking cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKey {
    /// A character was typed (any printable key, including space and return).
    Insert,
    /// Backspace / forward delete.
    Delete,
    /// Something that moves the caret or otherwise breaks the running count:
    /// arrows, Home/End, page keys, a mouse click, ⌘Z, ⌘A, a paste of the
    /// user's own.
    Navigate,
}

/// Tracks the pasted span's current length from keystrokes alone.
///
/// Only meaningful while `valid` — one caret move and the count no longer
/// describes anything, because the next backspace might be deleting text
/// somewhere else entirely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditCounter {
    len: usize,
    edits: usize,
    valid: bool,
}

impl EditCounter {
    /// Start tracking a freshly pasted span of `pasted_len` characters.
    pub fn new(pasted_len: usize) -> Self {
        Self {
            len: pasted_len,
            edits: 0,
            valid: true,
        }
    }

    pub fn apply(&mut self, key: EditKey) {
        if !self.valid {
            return;
        }
        match key {
            EditKey::Insert => {
                self.len += 1;
                self.edits += 1;
            }
            EditKey::Delete => {
                // Deleting past the start of the span means the user is now
                // eating their own text; the span is no longer identifiable.
                if self.len == 0 {
                    self.valid = false;
                    return;
                }
                self.len -= 1;
                self.edits += 1;
            }
            EditKey::Navigate => self.valid = false,
        }
    }

    /// Current length of the pasted span, or `None` once the count is void.
    pub fn len(&self) -> Option<usize> {
        self.valid.then_some(self.len)
    }

    /// Whether the span still contains anything.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether any edit key has landed since the paste.
    pub fn touched(&self) -> bool {
        self.edits > 0
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }
}

/// The action to report for a captured correction, mirroring the server's
/// `corrections.action` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// The paste survived untouched.
    Accepted,
    /// The user changed it; the new text is the training target.
    Edited,
    /// The user deleted it outright — don't train on this utterance.
    Excluded,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Edited => "edited",
            Self::Excluded => "excluded",
        }
    }
}

/// Classify what happened to a pasted dictation.
pub fn classify(inserted: &str, current: &str) -> Action {
    if current.trim().is_empty() {
        Action::Excluded
    } else if current == inserted {
        Action::Accepted
    } else {
        Action::Edited
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASTED: &str = "let's deploy the cluster";

    /// Field held only the paste; a word inside it was fixed.
    #[test]
    fn word_replaced_inside_region() {
        let before = PASTED;
        let after = "let's deploy the Kubernetes cluster";
        let region = Region::new(0, before.chars().count());
        assert_eq!(
            edited_region_text(before, after, region).as_deref(),
            Some("let's deploy the Kubernetes cluster")
        );
    }

    /// The paste landed mid-document; the surrounding prose must come back
    /// out of the region text.
    #[test]
    fn region_offset_within_surrounding_text() {
        let before = "Hi there. let's deploy the cluster Thanks!";
        let after = "Hi there. let's deploy the Kubernetes cluster Thanks!";
        let region = Region::new(10, PASTED.chars().count());
        assert_eq!(
            edited_region_text(before, after, region).as_deref(),
            Some("let's deploy the Kubernetes cluster")
        );
    }

    /// Editing text the user wrote themselves is not a correction.
    #[test]
    fn edit_outside_region_is_rejected() {
        let before = "Hi there. let's deploy the cluster Thanks!";
        let after = "Hi you. let's deploy the cluster Thanks!";
        let region = Region::new(10, PASTED.chars().count());
        assert_eq!(edited_region_text(before, after, region), None);
    }

    /// An edit that starts inside the region but runs past its end is
    /// ambiguous — the user is rewriting across the boundary.
    #[test]
    fn edit_spanning_region_boundary_is_rejected() {
        let before = "let's deploy the cluster and then go home";
        let after = "let's deploy the fleet immediately";
        let region = Region::new(0, PASTED.chars().count());
        assert_eq!(edited_region_text(before, after, region), None);
    }

    #[test]
    fn unchanged_field_returns_region_verbatim() {
        let before = "Hi. let's deploy the cluster";
        let region = Region::new(4, PASTED.chars().count());
        assert_eq!(
            edited_region_text(before, before, region).as_deref(),
            Some(PASTED)
        );
    }

    #[test]
    fn deleted_region_reports_empty() {
        let before = "Hi. let's deploy the cluster";
        let after = "Hi. ";
        let region = Region::new(4, PASTED.chars().count());
        assert_eq!(edited_region_text(before, after, region).as_deref(), Some(""));
    }

    /// Insertion adjacent to a repeated character must not double-count the
    /// prefix and suffix.
    #[test]
    fn overlapping_prefix_and_suffix() {
        let region = Region::new(0, 3);
        assert_eq!(
            edited_region_text("abc", "abbc", region).as_deref(),
            Some("abbc")
        );
    }

    #[test]
    fn multibyte_text_uses_char_offsets() {
        let before = "héllo wörld";
        let after = "héllo wonderful wörld";
        let region = Region::new(0, before.chars().count());
        assert_eq!(
            edited_region_text(before, after, region).as_deref(),
            Some("héllo wonderful wörld")
        );
    }

    /// A snapshot from before an earlier edit can leave the region past the
    /// end of the text; bail instead of panicking on the slice.
    #[test]
    fn region_past_end_of_before_is_rejected() {
        assert_eq!(edited_region_text("short", "short", Region::new(0, 99)), None);
    }

    #[test]
    fn counter_tracks_length_through_edits() {
        let mut c = EditCounter::new(10);
        c.apply(EditKey::Delete);
        c.apply(EditKey::Delete);
        c.apply(EditKey::Insert);
        assert_eq!(c.len(), Some(9));
        assert!(c.touched());
    }

    #[test]
    fn counter_voids_on_navigation() {
        let mut c = EditCounter::new(10);
        c.apply(EditKey::Delete);
        c.apply(EditKey::Navigate);
        c.apply(EditKey::Delete);
        assert_eq!(c.len(), None);
        assert!(!c.is_valid());
    }

    /// Backspacing past the start of the paste means the user is deleting
    /// their own text now — the span stops being identifiable.
    #[test]
    fn counter_voids_when_deleting_past_the_span() {
        let mut c = EditCounter::new(1);
        c.apply(EditKey::Delete);
        assert_eq!(c.len(), Some(0));
        c.apply(EditKey::Delete);
        assert_eq!(c.len(), None);
    }

    #[test]
    fn classify_covers_the_three_outcomes() {
        assert_eq!(classify("hello", "hello"), Action::Accepted);
        assert_eq!(classify("hello", "hullo"), Action::Edited);
        assert_eq!(classify("hello", "   "), Action::Excluded);
    }
}
