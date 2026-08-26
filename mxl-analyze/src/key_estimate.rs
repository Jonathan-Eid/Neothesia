//! The key/scale in force at a point in the piece: the written signature if
//! there is one, otherwise a guess from what's actually being played nearby -
//! a *local* guess, not a whole-song average, so modal color over a static
//! harmony isn't smoothed away.

use std::time::Duration;

use midi_file::SignatureTrack;
use music_theory::Key;

use crate::model::Score;

pub const LOCAL_WINDOW: Duration = Duration::from_secs(6); // ~2 measures at a moderate tempo

pub fn local_key(score: &Score, signature_track: &SignatureTrack, at: Duration) -> Key {
    let half = LOCAL_WINDOW / 2;
    let lo = at.saturating_sub(half);
    let hi = at + half;

    let mut weights = [0.0f32; 12];
    for note in score.notes.iter().filter(|n| n.start < hi && n.end > lo) {
        let overlap_start = note.start.max(lo);
        let overlap_end = note.end.min(hi);
        let duration = overlap_end.saturating_sub(overlap_start).as_secs_f32();
        weights[(note.pitch % 12) as usize] += duration;
    }

    match signature_track.key_signature_at(&at) {
        // A key signature only fixes the pitch collection (how many
        // sharps/flats) - it's shared by a major key and its relative minor,
        // and MusicXML routinely leaves that choice unstated. Let the notes
        // decide, unless there's nothing nearby to judge by.
        Some(signature) if weights.iter().any(|w| *w > 0.0) => {
            Key::resolve_mode(signature.fifths, &weights)
        }
        Some(signature) => Key::from_fifths(signature.fifths, signature.minor),
        None => Key::estimate(&weights),
    }
}
