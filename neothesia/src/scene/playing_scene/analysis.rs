//! Reads the song as music theory: what is sounding, what harmony it makes,
//! where that sits in the key, and how it lines up with the meter.
//!
//! Everything here is derived from the file, so it works the same for a midi
//! file and for a MusicXML score.

use std::time::Duration;

use midi_file::MidiNote;
use music_theory::{Chord, Key, Meter, note_name_with_octave, rhythm_name};

use crate::song::{PlayerConfig, Song};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Left,
    Right,
}

/// How the notes of a song are split between the two hands.
enum HandSplit {
    /// Piano scores keep the hands on separate staves, which become tracks.
    Tracks { left: Vec<usize> },
    /// Everything else gets split by register.
    Register { split: u8 },
}

impl HandSplit {
    fn hand_of(&self, note: &MidiNote) -> Hand {
        match self {
            HandSplit::Tracks { left } => {
                if left.contains(&note.track_id) {
                    Hand::Left
                } else {
                    Hand::Right
                }
            }
            HandSplit::Register { split } => {
                if note.note < *split {
                    Hand::Left
                } else {
                    Hand::Right
                }
            }
        }
    }
}

struct IndexedNote {
    start: Duration,
    end: Duration,
    note: u8,
    hand: Hand,
    track_id: usize,
    track_color_id: usize,
    channel: u8,
}

/// Everything about a song that does not change while it plays.
pub struct SongAnalysis {
    notes: Vec<IndexedNote>,
    /// Longest note in the song, the window we have to look back over.
    longest: Duration,
    /// Key of the whole song, used when the file has no key signature.
    estimated_key: Key,
    onsets: Vec<Duration>,
    left_onsets: Vec<Duration>,
    right_onsets: Vec<Duration>,
}

impl SongAnalysis {
    pub fn new(song: &Song, hide_muted: bool) -> Self {
        // Only the tracks taking part in the performance, so dropping the
        // vocals also drops them out of the harmony.
        let audible: Vec<&midi_file::MidiTrack> = song
            .file
            .tracks
            .iter()
            .filter(|track| !track.notes.is_empty())
            .filter(|track| !(track.has_drums && !track.has_other_than_drums))
            .filter(|track| song.config.is_active(track.track_id, hide_muted))
            .collect();

        let split = Self::split_hands(&audible);

        let mut notes: Vec<IndexedNote> = audible
            .iter()
            .flat_map(|track| track.notes.iter())
            .filter(|note| note.channel != 9)
            .map(|note| IndexedNote {
                start: note.start,
                end: note.end,
                note: note.note,
                hand: split.hand_of(note),
                track_id: note.track_id,
                track_color_id: note.track_color_id,
                channel: note.channel,
            })
            .collect();

        notes.sort_by_key(|note| (note.start, note.note));

        let longest = notes
            .iter()
            .map(|note| note.end.saturating_sub(note.start))
            .max()
            .unwrap_or_default();

        let onsets = Self::collect_onsets(&notes, None);
        let left_onsets = Self::collect_onsets(&notes, Some(Hand::Left));
        let right_onsets = Self::collect_onsets(&notes, Some(Hand::Right));

        // How much each pitch class is used, weighted by how long it sounds.
        let mut weights = [0.0f32; 12];
        for note in notes.iter() {
            weights[(note.note % 12) as usize] += note.end.saturating_sub(note.start).as_secs_f32();
        }

        Self {
            notes,
            longest,
            estimated_key: Key::estimate(&weights),
            onsets,
            left_onsets,
            right_onsets,
        }
    }

    /// Two melodic tracks are a piano score, so the tracks are the hands.
    /// Anything else gets split at middle C.
    fn split_hands(tracks: &[&midi_file::MidiTrack]) -> HandSplit {
        const MIDDLE_C: u8 = 60;

        if tracks.len() < 2 {
            return HandSplit::Register { split: MIDDLE_C };
        }

        let mut medians: Vec<(usize, u8)> = tracks
            .iter()
            .map(|track| {
                let mut pitches: Vec<u8> = track.notes.iter().map(|note| note.note).collect();
                pitches.sort_unstable();
                (track.track_id, pitches[pitches.len() / 2])
            })
            .collect();

        medians.sort_by_key(|(_, median)| *median);

        if tracks.len() == 2 {
            return HandSplit::Tracks {
                left: vec![medians[0].0],
            };
        }

        // Several tracks: everything sitting below middle C is the left hand.
        let left: Vec<usize> = medians
            .iter()
            .filter(|(_, median)| *median < MIDDLE_C)
            .map(|(track_id, _)| *track_id)
            .collect();

        if left.is_empty() || left.len() == tracks.len() {
            HandSplit::Register { split: MIDDLE_C }
        } else {
            HandSplit::Tracks { left }
        }
    }

    fn collect_onsets(notes: &[IndexedNote], hand: Option<Hand>) -> Vec<Duration> {
        let mut onsets: Vec<Duration> = notes
            .iter()
            .filter(|note| hand.is_none_or(|hand| note.hand == hand))
            .map(|note| note.start)
            .collect();

        onsets.dedup();
        onsets
    }

    /// Notes sounding at a point in time.
    fn sounding(&self, time: Duration) -> impl Iterator<Item = &IndexedNote> {
        // A note can only be sounding if it started within the longest note's
        // reach, which keeps this a small window instead of a full scan.
        let from = time.saturating_sub(self.longest);
        let start = self.notes.partition_point(|note| note.start < from);
        let end = self.notes.partition_point(|note| note.start <= time);

        self.notes[start..end]
            .iter()
            .filter(move |note| note.end > time)
    }

    /// The notes to sound and light up when jumping straight to a point in time.
    pub fn notes_at(&self, song: &Song, time: Duration) -> Vec<SoundingNote> {
        self.sounding(time)
            .filter(|note| {
                song.config
                    .tracks
                    .get(note.track_id)
                    .is_some_and(|track| track.player == PlayerConfig::Auto && track.visible)
            })
            .map(|note| SoundingNote {
                key: note.note,
                channel: note.channel,
                track_color_id: note.track_color_id,
            })
            .collect()
    }

    pub fn onsets(&self, hand: Option<Hand>) -> &[Duration] {
        match hand {
            None => &self.onsets,
            Some(Hand::Left) => &self.left_onsets,
            Some(Hand::Right) => &self.right_onsets,
        }
    }

    /// Onset right after `time`, for stepping through a song note by note.
    pub fn next_onset(&self, hand: Option<Hand>, time: Duration) -> Option<Duration> {
        let onsets = self.onsets(hand);
        // A little slack, so that stepping does not get stuck on the note we
        // just landed on.
        let time = time + Duration::from_millis(2);

        onsets
            .get(onsets.partition_point(|onset| *onset <= time))
            .copied()
    }

    pub fn previous_onset(&self, hand: Option<Hand>, time: Duration) -> Option<Duration> {
        let onsets = self.onsets(hand);
        let time = time.saturating_sub(Duration::from_millis(2));

        onsets
            .get(
                onsets
                    .partition_point(|onset| *onset < time)
                    .checked_sub(1)?,
            )
            .copied()
    }

    pub fn analyse(&self, song: &Song, time: Duration) -> Snapshot {
        let signatures = &song.file.signature_track;

        // A key signature is shared by a major key and its relative minor, and
        // files are careless about saying which, so the notes get a vote.
        let key = match signatures.key_signature_at(&time) {
            Some(signature) => {
                let relative_minor = Key::from_fifths(signature.fifths, true);

                if signature.minor
                    || (self.estimated_key.minor
                        && self.estimated_key.tonic == relative_minor.tonic)
                {
                    relative_minor
                } else {
                    Key::from_fifths(signature.fifths, false)
                }
            }
            None => self.estimated_key,
        };

        let time_signature = signatures.time_signature_at(&time);
        let meter = Meter::new(time_signature.numerator, time_signature.denominator);

        let sounding: Vec<&IndexedNote> = self.sounding(time).collect();

        let of_hand = |hand: Hand| -> Vec<u8> {
            let mut notes: Vec<u8> = sounding
                .iter()
                .filter(|note| note.hand == hand)
                .map(|note| note.note)
                .collect();
            notes.sort_unstable();
            notes.dedup();
            notes
        };

        let left_notes = of_hand(Hand::Left);
        let right_notes = of_hand(Hand::Right);
        let all: Vec<u8> = left_notes
            .iter()
            .chain(right_notes.iter())
            .copied()
            .collect();

        let (measure, beat, measure_len) = self.position(song, time);

        // A beat is the note value the meter counts in.
        let quarter = measure_len
            .map(|len| len.as_secs_f32() / meter.quarters().max(0.001))
            .unwrap_or(0.5);

        // The note that just started is the one worth naming.
        let newest = sounding.iter().max_by_key(|note| note.start);
        let rhythm = newest
            .map(|note| {
                let quarters =
                    note.end.saturating_sub(note.start).as_secs_f32() / quarter.max(0.001);
                rhythm_name(quarters)
            })
            .unwrap_or("");

        let chord = music_theory::detect(&all);
        let roman = chord
            .as_ref()
            .map(|chord| key.roman(chord))
            .unwrap_or_default();
        let mode = chord
            .as_ref()
            .map(|chord| key.mode_of_degree(key.degree(chord.root).0))
            .unwrap_or("");

        Snapshot {
            key,
            meter,
            measure,
            beat,
            left: HandSnapshot::new(&key, left_notes),
            right: HandSnapshot::new(&key, right_notes),
            roman,
            mode,
            rhythm,
            chord,
        }
    }

    /// Measure number, beat within it, and how long the measure lasts.
    fn position(&self, song: &Song, time: Duration) -> (usize, f32, Option<Duration>) {
        let measures = &song.file.measures;

        let id = match measures.binary_search(&time) {
            Ok(id) => id,
            Err(id) => id.saturating_sub(1),
        };

        let Some(start) = measures.get(id) else {
            return (1, 1.0, None);
        };

        let signature = song.file.signature_track.time_signature_at(start);
        let len = measures.get(id + 1).map(|end| end.saturating_sub(*start));

        let beat = len
            .map(|len| {
                let progress =
                    time.saturating_sub(*start).as_secs_f32() / len.as_secs_f32().max(0.001);
                1.0 + progress * signature.numerator as f32
            })
            .unwrap_or(1.0);

        (id + 1, beat, len)
    }
}

pub struct SoundingNote {
    pub key: u8,
    pub channel: u8,
    pub track_color_id: usize,
}

/// What one hand is playing.
pub struct HandSnapshot {
    pub notes: Vec<u8>,
    pub note_names: String,
    pub symbol: String,
    pub roman: String,
    pub intervals: String,
}

impl HandSnapshot {
    fn new(key: &Key, notes: Vec<u8>) -> Self {
        let flats = key.prefers_flats();

        let note_names = notes
            .iter()
            .map(|note| note_name_with_octave(*note, flats))
            .collect::<Vec<_>>()
            .join(" ");

        let chord = music_theory::detect(&notes);

        let symbol = chord
            .as_ref()
            .map(|chord| chord.symbol(flats))
            .unwrap_or_else(|| {
                // A lone note is not a chord, but it is still worth naming.
                notes
                    .first()
                    .map(|note| note_name_with_octave(*note, flats))
                    .unwrap_or_default()
            });

        let roman = chord
            .as_ref()
            .map(|chord| key.roman(chord))
            .unwrap_or_default();

        let intervals = chord
            .as_ref()
            .map(|chord| {
                chord
                    .intervals()
                    .iter()
                    .map(|(_, name)| *name)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();

        Self {
            notes,
            note_names,
            symbol,
            roman,
            intervals,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
}

/// The state of the music at one point in time.
pub struct Snapshot {
    pub key: Key,
    pub meter: Meter,
    pub measure: usize,
    pub beat: f32,
    pub left: HandSnapshot,
    pub right: HandSnapshot,
    /// Harmony of both hands together.
    pub chord: Option<Chord>,
    pub roman: String,
    pub mode: &'static str,
    pub rhythm: &'static str,
}

impl Snapshot {
    pub fn symbol(&self) -> String {
        self.chord
            .as_ref()
            .map(|chord| chord.symbol(self.key.prefers_flats()))
            .unwrap_or_default()
    }

    /// Names of everything sounding, for when there is no chord to name.
    pub fn sounding_names(&self) -> String {
        let flats = self.key.prefers_flats();

        let mut notes: Vec<u8> = self
            .left
            .notes
            .iter()
            .chain(self.right.notes.iter())
            .copied()
            .collect();
        notes.sort_unstable();

        notes
            .iter()
            .map(|note| note_name_with_octave(*note, flats))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn description(&self) -> String {
        self.chord
            .as_ref()
            .map(|chord| chord.description(self.key.prefers_flats()))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analysis(song: &Song) -> SongAnalysis {
        SongAnalysis::new(song, true)
    }

    fn song() -> Song {
        // Two staves, so the hands come from the tracks. Measure one is
        // repeated, and carries a C minor key signature.
        let file = midi_file::MidiFile::new("../midi-file/test-assets/test.musicxml").unwrap();
        Song::new(file)
    }

    #[test]
    fn hands_come_from_the_staves() {
        let song = song();
        let analysis = analysis(&song);

        // Half way through the second beat: a held bass note under a chord.
        let snapshot = analysis.analyse(&song, Duration::from_millis(1500));

        assert_eq!(snapshot.left.notes, [48]);
        assert_eq!(snapshot.right.notes, [75, 79]);
    }

    #[test]
    fn key_comes_from_the_signature() {
        let song = song();
        let analysis = analysis(&song);

        let snapshot = analysis.analyse(&song, Duration::from_millis(500));

        assert_eq!(snapshot.key.name(), "C minor");
        assert_eq!(snapshot.meter.label(), "4/4");
        assert_eq!(snapshot.measure, 1);
    }

    #[test]
    fn harmony_is_named_and_placed_in_the_key() {
        let song = song();
        let analysis = analysis(&song);

        // C3 in the left hand, C5 in the right: a bare octave, then the chord.
        let snapshot = analysis.analyse(&song, Duration::from_millis(1500));

        // C, E♭ and G sounding together is the tonic of C minor.
        assert_eq!(snapshot.symbol(), "Cm");
        assert_eq!(snapshot.roman, "i");
    }

    #[test]
    fn stepping_walks_the_onsets() {
        let song = song();
        let analysis = analysis(&song);

        let zero = Duration::ZERO;
        let first = analysis.next_onset(None, zero).unwrap();
        assert_eq!(first.as_millis(), 1000);

        // The left hand holds one whole note per measure.
        let left = analysis.next_onset(Some(Hand::Left), zero).unwrap();
        assert_eq!(left.as_millis(), 2000);

        // And back again.
        let back = analysis.previous_onset(None, first).unwrap();
        assert_eq!(back.as_millis(), 0);
    }
}
