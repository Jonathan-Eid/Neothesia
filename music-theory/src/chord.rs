//! Chord recognition.
//!
//! Notes are matched against a table of chord shapes at all twelve roots. The
//! best fit wins, which means a missing fifth or an extra tension does not stop
//! a chord from being named, it just costs the candidate some points.

use crate::{interval_short, note_name, pitch_class, pitch_class_set};

/// The triad a chord is built on, which is what decides the case of a roman numeral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Triad {
    Major,
    Minor,
    Diminished,
    Augmented,
    Suspended,
    Power,
}

impl Triad {
    pub fn is_lowercase(&self) -> bool {
        matches!(self, Triad::Minor | Triad::Diminished)
    }
}

pub struct ChordShape {
    /// Suffix written after the root, eg. the `m7` of `Dm7`.
    pub symbol: &'static str,
    /// Spoken name, eg. `minor 7th`.
    pub name: &'static str,
    /// Semitones above the root.
    pub intervals: &'static [u8],
    pub triad: Triad,
}

macro_rules! shape {
    ($symbol:expr, $name:expr, $intervals:expr, $triad:expr) => {
        ChordShape {
            symbol: $symbol,
            name: $name,
            intervals: &$intervals,
            triad: $triad,
        }
    };
}

pub static SHAPES: &[ChordShape] = &[
    shape!("5", "power chord", [0, 7], Triad::Power),
    shape!("", "major", [0, 4, 7], Triad::Major),
    shape!("m", "minor", [0, 3, 7], Triad::Minor),
    shape!("dim", "diminished", [0, 3, 6], Triad::Diminished),
    shape!("aug", "augmented", [0, 4, 8], Triad::Augmented),
    shape!("sus2", "suspended 2nd", [0, 2, 7], Triad::Suspended),
    shape!("sus4", "suspended 4th", [0, 5, 7], Triad::Suspended),
    shape!("6", "major 6th", [0, 4, 7, 9], Triad::Major),
    shape!("m6", "minor 6th", [0, 3, 7, 9], Triad::Minor),
    shape!("7", "dominant 7th", [0, 4, 7, 10], Triad::Major),
    shape!("maj7", "major 7th", [0, 4, 7, 11], Triad::Major),
    shape!("m7", "minor 7th", [0, 3, 7, 10], Triad::Minor),
    shape!("mMaj7", "minor major 7th", [0, 3, 7, 11], Triad::Minor),
    shape!("m7b5", "half diminished", [0, 3, 6, 10], Triad::Diminished),
    shape!("dim7", "diminished 7th", [0, 3, 6, 9], Triad::Diminished),
    shape!("7#5", "augmented 7th", [0, 4, 8, 10], Triad::Augmented),
    shape!(
        "7sus4",
        "dominant 7th sus4",
        [0, 5, 7, 10],
        Triad::Suspended
    ),
    shape!("add9", "added 9th", [0, 2, 4, 7], Triad::Major),
    shape!("madd9", "minor added 9th", [0, 2, 3, 7], Triad::Minor),
    shape!("6/9", "six nine", [0, 2, 4, 7, 9], Triad::Major),
    shape!("9", "dominant 9th", [0, 2, 4, 7, 10], Triad::Major),
    shape!("maj9", "major 9th", [0, 2, 4, 7, 11], Triad::Major),
    shape!("m9", "minor 9th", [0, 2, 3, 7, 10], Triad::Minor),
    shape!("7b9", "dominant 7th flat 9", [0, 1, 4, 7, 10], Triad::Major),
    shape!("m11", "minor 11th", [0, 2, 3, 5, 7, 10], Triad::Minor),
    shape!("11", "dominant 11th", [0, 2, 5, 7, 10], Triad::Suspended),
    shape!("13", "dominant 13th", [0, 2, 4, 7, 9, 10], Triad::Major),
    shape!(
        "maj7#11",
        "lydian major 7th",
        [0, 4, 6, 7, 11],
        Triad::Major
    ),
];

#[derive(Debug, Clone)]
pub struct Chord {
    /// Pitch class of the root.
    pub root: u8,
    /// Pitch class of the lowest sounding note.
    pub bass: u8,
    shape: usize,
    /// Pitch classes that are sounding but are not part of the shape.
    pub tensions: Vec<u8>,
    /// The notes this was deduced from, low to high.
    pub notes: Vec<u8>,
    /// Raw match score from `detect()`. Only meaningful relative to other
    /// `Chord::confidence` values, never as an absolute quality measure.
    pub confidence: i32,
}

impl Chord {
    pub fn shape(&self) -> &'static ChordShape {
        &SHAPES[self.shape]
    }

    pub fn triad(&self) -> Triad {
        self.shape().triad
    }

    /// True for shapes that want to resolve, ie. the ones built on a dominant 7th.
    pub fn is_dominant(&self) -> bool {
        let intervals = self.shape().intervals;
        intervals.contains(&10) && intervals.contains(&4)
    }

    pub fn is_inverted(&self) -> bool {
        self.bass != self.root
    }

    /// Written symbol, eg. `Cmaj7/E`.
    pub fn symbol(&self, flats: bool) -> String {
        let mut out = format!("{}{}", note_name(self.root, flats), self.shape().symbol);

        for tension in self.tensions.iter() {
            out.push('(');
            out.push_str(interval_short(*tension));
            out.push(')');
        }

        if self.is_inverted() {
            out.push('/');
            out.push_str(note_name(self.bass, flats));
        }

        out
    }

    /// Spoken name, eg. `C major 7th, 1st inversion`.
    pub fn description(&self, flats: bool) -> String {
        let mut out = format!("{} {}", note_name(self.root, flats), self.shape().name);

        if let Some(inversion) = self.inversion() {
            let ordinal = ["root position", "1st inv", "2nd inv", "3rd inv", "4th inv"];
            out.push_str(", ");
            out.push_str(ordinal[inversion.min(4)]);
        }

        out
    }

    /// How many chord tones sit below the root, `None` when the bass is foreign.
    pub fn inversion(&self) -> Option<usize> {
        let from_root = (self.bass + 12 - self.root) % 12;
        self.shape()
            .intervals
            .iter()
            .position(|interval| *interval == from_root)
    }

    /// Every sounding pitch class with its interval above the root, low to high.
    pub fn intervals(&self) -> Vec<(u8, &'static str)> {
        let mut seen = Vec::new();

        for note in self.notes.iter() {
            let from_root = (pitch_class(*note) + 12 - self.root) % 12;

            if !seen.iter().any(|(interval, _)| *interval == from_root) {
                seen.push((from_root, interval_short(from_root)));
            }
        }

        seen
    }
}

/// Names the harmony of a set of sounding notes.
pub fn detect(notes: &[u8]) -> Option<Chord> {
    let mut notes: Vec<u8> = notes.to_vec();
    notes.sort_unstable();
    notes.dedup();

    if notes.len() < 2 {
        return None;
    }

    let sounding = pitch_class_set(&notes);
    let bass = pitch_class(notes[0]);

    let mut best: Option<(i32, Chord)> = None;

    for root in 0..12u8 {
        for (id, shape) in SHAPES.iter().enumerate() {
            let template = shape
                .intervals
                .iter()
                .fold(0u16, |set, interval| set | 1 << ((root + interval) % 12));

            let matched = (sounding & template).count_ones() as i32;
            let missing = (template & !sounding).count_ones() as i32;
            let extra = (sounding & !template).count_ones() as i32;

            // Two notes in common is the least that can suggest a chord.
            if matched < 2 {
                continue;
            }

            // The root itself carries a lot of weight, a shape that has to
            // invent it is a weak guess.
            let has_root = sounding & 1 << root != 0;

            let mut score = matched * 10 - missing * 8 - extra * 7;
            score += if has_root { 4 } else { -6 };
            score += if root == bass { 3 } else { 0 };
            // Prefer the simpler shape when everything else ties.
            score -= id as i32 / 8;

            if best.as_ref().is_none_or(|(top, _)| score > *top) {
                let tensions = (0..12u8)
                    .filter(|pc| sounding & 1 << pc != 0 && template & 1 << pc == 0)
                    .map(|pc| (pc + 12 - root) % 12)
                    .collect();

                best = Some((
                    score,
                    Chord {
                        root,
                        bass,
                        shape: id,
                        tensions,
                        notes: notes.clone(),
                        confidence: score,
                    },
                ));
            }
        }
    }

    best.map(|(_, chord)| chord)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(notes: &[u8]) -> String {
        detect(notes).unwrap().symbol(false)
    }

    #[test]
    fn triads() {
        assert_eq!(symbol(&[60, 64, 67]), "C");
        assert_eq!(symbol(&[60, 63, 67]), "Cm");
        assert_eq!(symbol(&[60, 63, 66]), "Cdim");
        assert_eq!(symbol(&[60, 64, 68]), "Caug");
        assert_eq!(symbol(&[60, 65, 67]), "Csus4");
        assert_eq!(symbol(&[60, 67]), "C5");
    }

    #[test]
    fn sevenths() {
        assert_eq!(symbol(&[60, 64, 67, 71]), "Cmaj7");
        assert_eq!(symbol(&[60, 64, 67, 70]), "C7");
        assert_eq!(symbol(&[60, 63, 67, 70]), "Cm7");
        assert_eq!(symbol(&[60, 63, 66, 70]), "Cm7b5");
        assert_eq!(symbol(&[60, 63, 66, 69]), "Cdim7");
    }

    #[test]
    fn extended() {
        assert_eq!(symbol(&[60, 64, 67, 70, 74]), "C9");
        assert_eq!(symbol(&[60, 64, 67, 71, 74]), "Cmaj9");
        assert_eq!(symbol(&[60, 63, 67, 70, 74]), "Cm9");
        assert_eq!(symbol(&[60, 64, 67, 74]), "Cadd9");
    }

    #[test]
    fn inversions() {
        // First inversion of C major, E in the bass.
        assert_eq!(symbol(&[64, 67, 72]), "C/E");
        // Second inversion.
        assert_eq!(symbol(&[67, 72, 76]), "C/G");
    }

    #[test]
    fn missing_fifth() {
        // Rootless voicings are common, a missing fifth should not confuse it.
        assert_eq!(symbol(&[60, 64, 70]), "C7");
        assert_eq!(symbol(&[60, 64, 71]), "Cmaj7");
    }

    #[test]
    fn spelling_follows_the_key() {
        assert_eq!(detect(&[63, 67, 70]).unwrap().symbol(true), "Eb");
        assert_eq!(detect(&[63, 67, 70]).unwrap().symbol(false), "D#");
    }

    #[test]
    fn inversion_index() {
        assert_eq!(detect(&[60, 64, 67]).unwrap().inversion(), Some(0));
        assert_eq!(detect(&[64, 67, 72]).unwrap().inversion(), Some(1));
    }

    #[test]
    fn a_single_note_is_not_a_chord() {
        assert!(detect(&[60]).is_none());
        assert!(detect(&[]).is_none());
    }
}
