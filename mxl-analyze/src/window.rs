//! Resolves a note's harmony: first by looking at what's sounding at the same
//! instant, then, if that's not enough, by widening a time window outward
//! until either a confident chord emerges or a real limit is hit.

use std::time::Duration;

use music_theory::{Chord, pitch_class};

use crate::model::{NoteId, Score};

/// Below this, a match is too weak to trust - see the doc comment on
/// `Chord::confidence` for what the number means. A complete triad with its
/// root in the bass scores 37; a bare, unconfirmed dyad tops out at 24.
pub const MIN_CONFIDENCE: i32 = 26;

pub struct ExpandCap {
    pub max_notes: usize,
    pub max_duration: Duration,
}

impl Default for ExpandCap {
    fn default() -> Self {
        Self {
            max_notes: 16,
            max_duration: Duration::from_secs(8),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpandOutcome {
    Resolved,
    StoppedAtSilence,
    HitCap,
}

pub struct ExpandResult {
    pub outcome: ExpandOutcome,
    pub best_chord: Option<Chord>,
    pub window_note_ids: Vec<NoteId>,
}

/// Step A: the notes overlapping the anchor's own interval, across every
/// track. Resolves the common case - a chord written as a simultaneous stack -
/// without needing to look any further.
pub fn resolve_vertical(score: &Score, anchor: NoteId) -> Option<(Chord, Vec<NoteId>)> {
    let note = score.get(anchor);
    let window_ids = score.sounding_at(note.start);

    if window_ids.len() < 2 {
        return None;
    }

    let pitches: Vec<u8> = window_ids.iter().map(|id| score.get(*id).pitch).collect();
    let chord = music_theory::detect(&pitches)?;

    if chord.confidence >= MIN_CONFIDENCE {
        Some((chord, window_ids))
    } else {
        None
    }
}

/// Step B: grow a window outward from the anchor, nearest-neighbor-first,
/// re-testing after every addition, until it resolves, hits a genuine
/// silence, or exhausts its budget.
pub fn expand_window(score: &Score, anchor: NoteId, cap: &ExpandCap) -> ExpandResult {
    let anchor_note = score.get(anchor);

    let mut window: Vec<NoteId> = score.sounding_at(anchor_note.start);
    if window.is_empty() {
        window.push(anchor);
    }

    // The time span the window currently covers - grows as notes are added.
    let mut window_lo = window.iter().map(|id| score.get(*id).start).min().unwrap();
    let mut window_hi = window.iter().map(|id| score.get(*id).end).max().unwrap();

    let mut left = window.iter().map(|id| id.0).min().unwrap();
    let mut right = window.iter().map(|id| id.0).max().unwrap();
    let mut left_frozen = left == 0;
    let mut right_frozen = right + 1 >= score.len();

    let mut best_chord: Option<Chord> = None;
    let mut best_window: Vec<NoteId> = window.clone();

    loop {
        let mut pitches: Vec<u8> = window.iter().map(|id| score.get(*id).pitch).collect();
        pitches.sort_unstable();
        pitches.dedup_by_key(|p| pitch_class(*p));

        if let Some(chord) = music_theory::detect(&pitches) {
            let better = best_chord
                .as_ref()
                .is_none_or(|best| chord.confidence > best.confidence);
            if better {
                best_window = window.clone();
                best_chord = Some(chord);
            }

            if best_chord
                .as_ref()
                .is_some_and(|c| c.confidence >= MIN_CONFIDENCE)
            {
                return ExpandResult {
                    outcome: ExpandOutcome::Resolved,
                    best_chord,
                    window_note_ids: best_window,
                };
            }
        }

        if left_frozen && right_frozen {
            return ExpandResult {
                outcome: ExpandOutcome::StoppedAtSilence,
                best_chord,
                window_note_ids: best_window,
            };
        }

        if window.len() >= cap.max_notes || window_hi.saturating_sub(window_lo) >= cap.max_duration
        {
            return ExpandResult {
                outcome: ExpandOutcome::HitCap,
                best_chord,
                window_note_ids: best_window,
            };
        }

        // Nearest-neighbor-first: whichever side's next candidate sits closer
        // to the window advances. A genuine rest (nothing sounding, in any
        // track, in the gap between the window edge and the candidate) stops
        // that side rather than being crossed.
        let left_gap =
            (!left_frozen && left > 0).then(|| window_lo.saturating_sub(score.notes[left - 1].end));
        let right_gap = (!right_frozen && right + 1 < score.len())
            .then(|| score.notes[right + 1].start.saturating_sub(window_hi));

        let advance_left = match (left_gap, right_gap) {
            (Some(lg), Some(rg)) => lg <= rg,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => {
                left_frozen = true;
                right_frozen = true;
                continue;
            }
        };

        if advance_left {
            let candidate = &score.notes[left - 1];
            let no_gap = candidate.end >= window_lo;
            if no_gap || score.any_sounding_in(candidate.end, window_lo) {
                left -= 1;
                window.push(candidate.id);
                window_lo = window_lo.min(candidate.start);
            } else {
                left_frozen = true;
            }
        } else {
            let candidate = &score.notes[right + 1];
            let no_gap = candidate.start <= window_hi;
            if no_gap || score.any_sounding_in(window_hi, candidate.start) {
                right += 1;
                window.push(candidate.id);
                window_hi = window_hi.max(candidate.end);
            } else {
                right_frozen = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AnalyzedNote;
    use std::sync::Arc;

    fn note(id: usize, start_ms: u64, end_ms: u64, pitch: u8) -> AnalyzedNote {
        AnalyzedNote {
            id: NoteId(id),
            start: Duration::from_millis(start_ms),
            end: Duration::from_millis(end_ms),
            pitch,
            track_id: 0,
            track_name: None,
        }
    }

    fn score(notes: Vec<AnalyzedNote>) -> Score {
        Score {
            notes,
            measures: Arc::from([Duration::ZERO]),
            signature_track: midi_file::SignatureTrack::default(),
        }
    }

    #[test]
    fn vertical_resolves_a_simultaneous_triad() {
        let s = score(vec![
            note(0, 0, 1000, 60),
            note(1, 0, 1000, 64),
            note(2, 0, 1000, 67),
        ]);

        let (chord, window) = resolve_vertical(&s, NoteId(0)).unwrap();
        assert_eq!(chord.symbol(false), "C");
        assert_eq!(window.len(), 3);
    }

    #[test]
    fn a_lone_note_does_not_resolve_vertically() {
        let s = score(vec![note(0, 0, 1000, 60)]);
        assert!(resolve_vertical(&s, NoteId(0)).is_none());
    }

    #[test]
    fn expansion_resolves_a_monophonic_arpeggio() {
        // C4 E4 G4 C5, one at a time, no overlap: a broken C major chord.
        let s = score(vec![
            note(0, 0, 250, 60),
            note(1, 250, 500, 64),
            note(2, 500, 750, 67),
            note(3, 750, 1000, 72),
        ]);

        let result = expand_window(&s, NoteId(0), &ExpandCap::default());
        assert_eq!(result.outcome, ExpandOutcome::Resolved);
        assert_eq!(result.best_chord.unwrap().symbol(false), "C");
    }

    #[test]
    fn a_real_gap_stops_expansion() {
        // Two isolated notes, a full second of silence between them - nothing
        // to accumulate into a chord.
        let s = score(vec![note(0, 0, 200, 60), note(1, 1200, 1400, 64)]);

        let result = expand_window(&s, NoteId(0), &ExpandCap::default());
        assert_eq!(result.outcome, ExpandOutcome::StoppedAtSilence);
        assert!(result.best_chord.is_none());
    }

    #[test]
    fn an_unresolvable_run_hits_the_cap() {
        // A chromatic crawl: never settles into a clean triad no matter how
        // far the window grows.
        let mut notes = Vec::new();
        for i in 0..20u8 {
            let start = i as u64 * 100;
            notes.push(note(i as usize, start, start + 100, 60 + i));
        }
        let s = score(notes);

        let cap = ExpandCap {
            max_notes: 8,
            max_duration: Duration::from_secs(10),
        };
        let result = expand_window(&s, NoteId(10), &cap);
        assert_eq!(result.outcome, ExpandOutcome::HitCap);
    }
}
