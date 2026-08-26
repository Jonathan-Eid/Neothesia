//! Chord tone, or something else? For "something else", a small set of
//! textbook rules based on how the note is approached and left, and how
//! metrically strong its beat is.

use music_theory::{Chord, pitch_class};

use crate::{
    model::{NoteId, Score},
    schema::{BeatStrength, NoteRole},
};

pub fn beat_strength(score: &Score, id: NoteId) -> BeatStrength {
    let note = score.get(id);
    let (_, beat) = score.position_of(note.start);

    // Beat 1 is strong; other integer beats are medium; anything off the
    // beat is weak. `beat` is 0-based (offset in quarter-note-equivalents
    // from the top of the measure), so beat 1 in 1-based counting is 0.0 here.
    let fraction = beat.fract();
    if beat < 0.05 {
        BeatStrength::Strong
    } else if fraction.min(1.0 - fraction) < 0.05 {
        BeatStrength::Medium
    } else {
        BeatStrength::Weak
    }
}

/// Step, in signed semitones, from `from` to `to`; `None` for either endpoint
/// missing.
fn interval_between(score: &Score, from: Option<NoteId>, to: NoteId) -> Option<i32> {
    let from = from?;
    Some(score.get(to).pitch as i32 - score.get(from).pitch as i32)
}

pub fn classify_role(score: &Score, anchor: NoteId, chord: &Chord) -> NoteRole {
    let anchor_note = score.get(anchor);
    let anchor_pc = pitch_class(anchor_note.pitch);
    let root_relative = (anchor_pc + 12 - chord.root) % 12;

    let is_chord_tone =
        chord.shape().intervals.contains(&root_relative) || chord.tensions.contains(&root_relative);

    if is_chord_tone {
        return NoteRole::ChordTone {
            interval: music_theory::interval_short(root_relative),
        };
    }

    let (prev, next) = score.same_track_neighbors(anchor);
    let into = interval_between(score, prev, anchor);
    let out_of = next.map(|n| score.get(n).pitch as i32 - anchor_note.pitch as i32);

    let strength = beat_strength(score, anchor);
    let is_step = |i: i32| i.abs() <= 2 && i != 0;
    let is_leap = |i: i32| i.abs() > 2;

    match (into, out_of, strength) {
        (Some(a), Some(b), BeatStrength::Weak)
            if is_step(a) && is_step(b) && a.signum() == b.signum() =>
        {
            NoteRole::PassingTone
        }
        (Some(a), Some(b), BeatStrength::Weak)
            if is_step(a) && is_step(b) && a.signum() != b.signum() =>
        {
            NoteRole::NeighborTone
        }
        (Some(a), Some(b), _) if is_leap(a) && is_step(b) => NoteRole::Appoggiatura,
        (Some(0), Some(b), strength) if strength != BeatStrength::Weak && is_step(b) && b < 0 => {
            NoteRole::Suspension
        }
        _ => NoteRole::Unclassified,
    }
}

/// The looser check used when validating an LLM's proposed chord against the
/// note that escalated to it: accept a chord tone, or a note that plausibly
/// approaches/leaves it like a non-chord tone. Rejecting on bare pitch-class
/// membership alone would throw out exactly the answers this whole system
/// exists to get right - the escalated notes are disproportionately the
/// embellishing ones.
pub fn plausibly_belongs_to(score: &Score, anchor: NoteId, chord: &Chord) -> bool {
    !matches!(classify_role(score, anchor, chord), NoteRole::Unclassified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AnalyzedNote;
    use std::{sync::Arc, time::Duration};

    fn note(id: usize, track: usize, start_ms: u64, pitch: u8) -> AnalyzedNote {
        AnalyzedNote {
            id: NoteId(id),
            start: Duration::from_millis(start_ms),
            end: Duration::from_millis(start_ms + 200),
            pitch,
            track_id: track,
            track_name: None,
        }
    }

    fn score_with_4_4(notes: Vec<AnalyzedNote>) -> Score {
        // One measure per 800ms (4 quarters at 200ms each), 4/4 throughout.
        Score {
            notes,
            measures: Arc::from([
                Duration::ZERO,
                Duration::from_millis(800),
                Duration::from_millis(1600),
            ]),
            signature_track: midi_file::SignatureTrack::default(),
        }
    }

    fn c_major() -> Chord {
        music_theory::detect(&[60, 64, 67]).unwrap()
    }

    #[test]
    fn chord_tones_are_named_by_interval() {
        let s = score_with_4_4(vec![note(0, 0, 0, 64)]);
        let role = classify_role(&s, NoteId(0), &c_major());
        assert!(matches!(role, NoteRole::ChordTone { interval: "3" }));
    }

    #[test]
    fn passing_tone_between_two_chord_tones() {
        // C4 (chord tone, beat 1) -> D4 (passing, off the beat) -> E4 (chord tone, beat 2)
        let s = score_with_4_4(vec![
            note(0, 0, 0, 60),
            note(1, 0, 100, 62),
            note(2, 0, 200, 64),
        ]);
        let role = classify_role(&s, NoteId(1), &c_major());
        assert!(matches!(role, NoteRole::PassingTone));
    }

    #[test]
    fn neighbor_tone_turns_back() {
        // E4 (beat 1) -> F4 (neighbor, off the beat) -> E4 (beat 2)
        let s = score_with_4_4(vec![
            note(0, 0, 0, 64),
            note(1, 0, 100, 65),
            note(2, 0, 200, 64),
        ]);
        let role = classify_role(&s, NoteId(1), &c_major());
        assert!(matches!(role, NoteRole::NeighborTone));
    }
}
