//! Harmonic and rhythmic analysis of what is sounding right now.
//!
//! Everything in here works on plain midi note numbers, so it is equally happy
//! with a midi file, a MusicXML score, or keys pressed on a controller.

pub mod chord;
pub mod key;
pub mod meter;

pub use chord::{Chord, Triad, detect};
pub use key::Key;
pub use meter::{Meter, rhythm_name};

pub const SHARP_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
pub const FLAT_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];

/// Interval names, by distance in semitones.
pub const INTERVAL_NAMES: [&str; 12] = [
    "unison",
    "minor 2nd",
    "major 2nd",
    "minor 3rd",
    "major 3rd",
    "perfect 4th",
    "tritone",
    "perfect 5th",
    "minor 6th",
    "major 6th",
    "minor 7th",
    "major 7th",
];

/// Short interval names, the ones you would write in an analysis.
pub const INTERVAL_SHORT: [&str; 12] = [
    "1", "b2", "2", "b3", "3", "4", "#4", "5", "b6", "6", "b7", "7",
];

pub fn pitch_class(note: u8) -> u8 {
    note % 12
}

/// Octave in scientific pitch notation, so middle C (60) is C4.
pub fn octave(note: u8) -> i32 {
    note as i32 / 12 - 1
}

pub fn note_name(note: u8, flats: bool) -> &'static str {
    let names = if flats { FLAT_NAMES } else { SHARP_NAMES };
    names[pitch_class(note) as usize]
}

/// Name with the octave attached, eg. `Eb4`.
pub fn note_name_with_octave(note: u8, flats: bool) -> String {
    format!("{}{}", note_name(note, flats), octave(note))
}

pub fn interval_name(semitones: u8) -> &'static str {
    INTERVAL_NAMES[(semitones % 12) as usize]
}

pub fn interval_short(semitones: u8) -> &'static str {
    INTERVAL_SHORT[(semitones % 12) as usize]
}

/// Set of pitch classes, as a bit per class.
pub(crate) fn pitch_class_set(notes: &[u8]) -> u16 {
    notes
        .iter()
        .fold(0u16, |set, note| set | 1 << pitch_class(*note))
}
