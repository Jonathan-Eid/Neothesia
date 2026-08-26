//! A flattened view of a [`midi_file::MidiFile`]: every non-drum note, from
//! every track, in one timeline. This is the boundary the rest of the crate
//! works against, so the algorithm never has to reason about tracks directly.

use std::{sync::Arc, time::Duration};

use midi_file::{MidiFile, SignatureTrack};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoteId(pub usize);

#[derive(Debug, Clone)]
pub struct AnalyzedNote {
    pub id: NoteId,
    pub start: Duration,
    pub end: Duration,
    pub pitch: u8,
    pub track_id: usize,
    pub track_name: Option<String>,
}

pub struct Score {
    /// All non-drum notes, from every track, sorted by (start, pitch).
    pub notes: Vec<AnalyzedNote>,
    pub measures: Arc<[Duration]>,
    pub signature_track: SignatureTrack,
}

impl Score {
    pub fn from_midi_file(file: &MidiFile) -> Self {
        let mut notes: Vec<AnalyzedNote> = file
            .tracks
            .iter()
            .filter(|track| track.has_other_than_drums)
            .flat_map(|track| {
                track
                    .notes
                    .iter()
                    .filter(|note| note.channel != 9)
                    .map(|note| AnalyzedNote {
                        // Placeholder id, assigned for real once the full list is sorted.
                        id: NoteId(0),
                        start: note.start,
                        end: note.end,
                        pitch: note.note,
                        track_id: track.track_id,
                        track_name: track.name.clone(),
                    })
            })
            .collect();

        notes.sort_by_key(|note| (note.start, note.pitch));
        for (id, note) in notes.iter_mut().enumerate() {
            note.id = NoteId(id);
        }

        Self {
            notes,
            measures: file.measures.clone(),
            signature_track: file.signature_track.clone(),
        }
    }

    pub fn get(&self, id: NoteId) -> &AnalyzedNote {
        &self.notes[id.0]
    }

    pub fn len(&self) -> usize {
        self.notes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// Ids of notes overlapping `at`, across every track.
    pub fn sounding_at(&self, at: Duration) -> Vec<NoteId> {
        self.notes
            .iter()
            .filter(|note| note.start <= at && note.end > at)
            .map(|note| note.id)
            .collect()
    }

    /// Whether any note (in any track) is sounding anywhere in `[start, end)`.
    pub fn any_sounding_in(&self, start: Duration, end: Duration) -> bool {
        self.notes
            .iter()
            .any(|note| note.start < end && note.end > start)
    }

    /// Measure number (1-based) and offset within it, in quarter notes.
    pub fn position_of(&self, at: Duration) -> (usize, f32) {
        let id = match self.measures.binary_search(&at) {
            Ok(id) => id,
            Err(id) => id.saturating_sub(1),
        };

        let Some(start) = self.measures.get(id) else {
            return (1, 0.0);
        };

        let signature = self.signature_track.time_signature_at(start);
        let len = self
            .measures
            .get(id + 1)
            .map(|end| end.saturating_sub(*start));

        let beat = len
            .map(|len| {
                let progress =
                    at.saturating_sub(*start).as_secs_f32() / len.as_secs_f32().max(0.001);
                progress * signature.numerator as f32
            })
            .unwrap_or(0.0);

        (id + 1, beat)
    }

    /// The previous and next note in the same track as `id`, if any - the
    /// melodic neighbors used for non-chord-tone classification.
    pub fn same_track_neighbors(&self, id: NoteId) -> (Option<NoteId>, Option<NoteId>) {
        let anchor = self.get(id);

        let prev = self.notes[..id.0]
            .iter()
            .rev()
            .find(|note| note.track_id == anchor.track_id)
            .map(|note| note.id);

        let next = self.notes[id.0 + 1..]
            .iter()
            .find(|note| note.track_id == anchor.track_id)
            .map(|note| note.id);

        (prev, next)
    }
}
