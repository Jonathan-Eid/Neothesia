//! Groups unresolved notes that share the same open harmonic question into
//! one span, so an arpeggiated run that fails to resolve becomes one thing to
//! ask about, not one per note.

use crate::model::NoteId;

pub struct Span {
    pub note_ids: Vec<NoteId>,
}

/// `unresolved` is every unresolved anchor paired with the window its
/// expansion produced (its best partial candidate's window, even though that
/// candidate wasn't confident enough to resolve it). Anchors merge into one
/// span when they're adjacent in time (nothing resolved sits between them)
/// and their windows share at least one note - the concrete stand-in for
/// "the same open question."
pub fn merge_spans(unresolved: &[(NoteId, Vec<NoteId>)]) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut last_window: Option<&Vec<NoteId>> = None;
    let mut last_id: Option<NoteId> = None;

    for (id, window) in unresolved {
        let joins_previous = last_id.is_some_and(|last| are_adjacent(last, *id, unresolved))
            && last_window.is_some_and(|last| window.iter().any(|w| last.contains(w)));

        if joins_previous {
            spans.last_mut().unwrap().note_ids.push(*id);
        } else {
            spans.push(Span {
                note_ids: vec![*id],
            });
        }

        last_window = Some(window);
        last_id = Some(*id);
    }

    spans
}

/// Whether `a` and `b` are consecutive entries in `unresolved`'s own ordering
/// (the caller is expected to pass `unresolved` already sorted by note
/// position, so "adjacent in `unresolved`" means "nothing resolved between
/// them").
fn are_adjacent(a: NoteId, b: NoteId, unresolved: &[(NoteId, Vec<NoteId>)]) -> bool {
    let Some(pos_a) = unresolved.iter().position(|(id, _)| *id == a) else {
        return false;
    };
    unresolved.get(pos_a + 1).map(|(id, _)| *id) == Some(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_windows_merge() {
        let unresolved = vec![
            (NoteId(1), vec![NoteId(0), NoteId(1), NoteId(2)]),
            (NoteId(2), vec![NoteId(1), NoteId(2), NoteId(3)]),
        ];
        let spans = merge_spans(&unresolved);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].note_ids, vec![NoteId(1), NoteId(2)]);
    }

    #[test]
    fn disjoint_windows_stay_separate() {
        let unresolved = vec![
            (NoteId(1), vec![NoteId(0), NoteId(1)]),
            (NoteId(10), vec![NoteId(9), NoteId(10)]),
        ];
        let spans = merge_spans(&unresolved);
        assert_eq!(spans.len(), 2);
    }
}
