//! Keys, scale degrees, and roman numeral analysis.

use crate::{
    chord::{Chord, Triad},
    note_name, pitch_class,
};

/// Semitones above the tonic, for the major and the natural minor scale.
const MAJOR: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];

const NUMERALS: [&str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];

/// The mode you get by starting the scale on each degree.
const MAJOR_MODES: [&str; 7] = [
    "Ionian",
    "Dorian",
    "Phrygian",
    "Lydian",
    "Mixolydian",
    "Aeolian",
    "Locrian",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// Pitch class of the tonic.
    pub tonic: u8,
    pub minor: bool,
    /// Position on the circle of fifths, decides sharp or flat spelling.
    pub fifths: i8,
}

impl Default for Key {
    fn default() -> Self {
        Self::from_fifths(0, false)
    }
}

impl Key {
    pub fn from_fifths(fifths: i8, minor: bool) -> Self {
        let major_tonic = (fifths as i32 * 7).rem_euclid(12) as u8;
        let tonic = if minor {
            (major_tonic + 9) % 12
        } else {
            major_tonic
        };

        Self {
            tonic,
            minor,
            fifths,
        }
    }

    /// Flats read better in flat keys, sharps in sharp keys.
    pub fn prefers_flats(&self) -> bool {
        self.fifths < 0
    }

    pub fn name(&self) -> String {
        format!(
            "{} {}",
            note_name(self.tonic, self.prefers_flats()),
            if self.minor { "minor" } else { "major" }
        )
    }

    pub fn steps(&self) -> [u8; 7] {
        if self.minor { MINOR } else { MAJOR }
    }

    /// Pitch classes of the scale.
    pub fn scale(&self) -> [u8; 7] {
        self.steps().map(|step| (self.tonic + step) % 12)
    }

    pub fn scale_names(&self) -> Vec<&'static str> {
        let flats = self.prefers_flats();
        self.scale()
            .iter()
            .map(|pc| note_name(*pc, flats))
            .collect()
    }

    /// Scale degree of a pitch class, with the accidental needed to reach it.
    pub fn degree(&self, pc: u8) -> (usize, i8) {
        let pc = pitch_class(pc);
        let scale = self.scale();

        if let Some(id) = scale.iter().position(|step| *step == pc) {
            return (id, 0);
        }

        // Chromatic roots are written as an alteration of a scale degree,
        // flattened where possible, which is how they are usually spelled.
        if let Some(id) = scale.iter().position(|step| *step == (pc + 1) % 12) {
            return (id, -1);
        }

        let id = scale
            .iter()
            .position(|step| *step == (pc + 11) % 12)
            .unwrap_or(0);

        (id, 1)
    }

    pub fn is_diatonic(&self, pc: u8) -> bool {
        self.degree(pc).1 == 0
    }

    /// Mode implied by a chord sitting on this degree, eg. `Mixolydian` for V.
    pub fn mode_of_degree(&self, degree: usize) -> &'static str {
        // The minor scale is the major one started three degrees later.
        let offset = if self.minor { 5 } else { 0 };
        MAJOR_MODES[(degree + offset) % 7]
    }

    /// Roman numeral of a chord in this key, eg. `V7` or `bVII`.
    pub fn roman(&self, chord: &Chord) -> String {
        let (degree, accidental) = self.degree(chord.root);

        let mut out = String::new();

        // A dominant chord that is foreign to the key is usually borrowed to
        // point at another degree, which is worth spelling out.
        let borrowed = chord.notes.iter().any(|note| !self.is_diatonic(*note));

        if chord.is_dominant() && borrowed {
            let target = (chord.root + 5) % 12;

            if self.is_diatonic(target) && target != self.tonic {
                let (target_degree, _) = self.degree(target);
                let target_quality = self.triad_of_degree(target_degree);

                let numeral = if target_quality.is_lowercase() {
                    NUMERALS[target_degree].to_lowercase()
                } else {
                    NUMERALS[target_degree].to_string()
                };

                return format!("V7/{numeral}");
            }
        }

        match accidental {
            -1 => out.push('b'),
            1 => out.push('#'),
            _ => {}
        }

        let numeral = NUMERALS[degree];
        if chord.triad().is_lowercase() {
            out.push_str(&numeral.to_lowercase());
        } else {
            out.push_str(numeral);
        }

        out.push_str(figure(chord));

        out
    }

    /// Triad quality the key builds on a degree.
    pub fn triad_of_degree(&self, degree: usize) -> Triad {
        let scale = self.scale();
        let root = scale[degree % 7];
        let third = scale[(degree + 2) % 7];
        let fifth = scale[(degree + 4) % 7];

        let third = (third + 12 - root) % 12;
        let fifth = (fifth + 12 - root) % 12;

        match (third, fifth) {
            (4, 7) => Triad::Major,
            (3, 7) => Triad::Minor,
            (3, 6) => Triad::Diminished,
            (4, 8) => Triad::Augmented,
            _ => Triad::Major,
        }
    }

    /// Best fitting key for a set of notes, for files that carry no key signature.
    ///
    /// Uses the Krumhansl-Schmuckler profiles, correlating how much each pitch
    /// class is used against how much each key is expected to use it.
    pub fn estimate(weights: &[f32; 12]) -> Key {
        const MAJOR_PROFILE: [f32; 12] = [
            6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
        ];
        const MINOR_PROFILE: [f32; 12] = [
            6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
        ];

        let mut best = (f32::MIN, Key::default());

        for tonic in 0..12u8 {
            for minor in [false, true] {
                let profile = if minor { MINOR_PROFILE } else { MAJOR_PROFILE };

                let score: f32 = (0..12)
                    .map(|pc| {
                        let step = (pc + 12 - tonic as usize) % 12;
                        weights[pc] * profile[step]
                    })
                    .sum();

                if score > best.0 {
                    best = (score, Key::of(tonic, minor));
                }
            }
        }

        best.1
    }

    /// Key with a given tonic, spelled the way that key usually is.
    pub fn of(tonic: u8, minor: bool) -> Key {
        // Walk the circle of fifths to find the signature this tonic belongs to.
        let fifths = (-7..=7)
            .find(|fifths| Key::from_fifths(*fifths, minor).tonic == pitch_class(tonic))
            .unwrap_or(0);

        Key::from_fifths(fifths, minor)
    }
}

/// The figure written after a numeral, eg. the `7` of `V7`.
fn figure(chord: &Chord) -> &'static str {
    match chord.shape().symbol {
        "" | "5" => "",
        "m" => "",
        "dim" => "°",
        "aug" => "+",
        "sus2" => "sus2",
        "sus4" => "sus4",
        "6" | "m6" => "6",
        "7" => "7",
        "maj7" => "maj7",
        "m7" => "7",
        "mMaj7" => "maj7",
        "m7b5" => "ø7",
        "dim7" => "°7",
        "7#5" => "+7",
        "7sus4" => "7sus4",
        "add9" | "madd9" => "add9",
        "6/9" => "6/9",
        "9" | "m9" => "9",
        "maj9" => "maj9",
        "7b9" => "7b9",
        "11" | "m11" => "11",
        "13" => "13",
        "maj7#11" => "maj7#11",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::detect;

    fn roman(key: Key, notes: &[u8]) -> String {
        key.roman(&detect(notes).unwrap())
    }

    #[test]
    fn key_from_signature() {
        assert_eq!(Key::from_fifths(0, false).name(), "C major");
        assert_eq!(Key::from_fifths(1, false).name(), "G major");
        assert_eq!(Key::from_fifths(-1, false).name(), "F major");
        assert_eq!(Key::from_fifths(-3, true).name(), "C minor");
        assert_eq!(Key::from_fifths(0, true).name(), "A minor");
        assert_eq!(Key::from_fifths(4, true).name(), "C# minor");
    }

    #[test]
    fn diatonic_numerals_in_c_major() {
        let c = Key::from_fifths(0, false);

        assert_eq!(roman(c, &[60, 64, 67]), "I");
        assert_eq!(roman(c, &[62, 65, 69]), "ii");
        assert_eq!(roman(c, &[64, 67, 71]), "iii");
        assert_eq!(roman(c, &[65, 69, 72]), "IV");
        assert_eq!(roman(c, &[67, 71, 74]), "V");
        assert_eq!(roman(c, &[69, 72, 76]), "vi");
        assert_eq!(roman(c, &[71, 74, 77]), "vii°");
    }

    #[test]
    fn sevenths_keep_their_figure() {
        let c = Key::from_fifths(0, false);

        assert_eq!(roman(c, &[67, 71, 74, 77]), "V7");
        assert_eq!(roman(c, &[60, 64, 67, 71]), "Imaj7");
        assert_eq!(roman(c, &[62, 65, 69, 72]), "ii7");
        assert_eq!(roman(c, &[71, 74, 77, 81]), "viiø7");
    }

    #[test]
    fn the_same_pattern_transposes() {
        let c = Key::from_fifths(0, false);
        let g = Key::from_fifths(1, false);

        // I V vi IV in C, then the very same thing in G.
        let in_c = [
            roman(c, &[60, 64, 67]),
            roman(c, &[67, 71, 74]),
            roman(c, &[69, 72, 76]),
            roman(c, &[65, 69, 72]),
        ];
        let in_g = [
            roman(g, &[67, 71, 74]),
            roman(g, &[62, 66, 69]),
            roman(g, &[64, 67, 71]),
            roman(g, &[60, 64, 67]),
        ];

        assert_eq!(in_c, ["I", "V", "vi", "IV"]);
        assert_eq!(in_g, in_c);
    }

    #[test]
    fn borrowed_chords_get_an_accidental() {
        let c = Key::from_fifths(0, false);

        // bVII, the classic rock chord.
        assert_eq!(roman(c, &[58, 62, 65]), "bVII");
        // bVI.
        assert_eq!(roman(c, &[56, 60, 63]), "bVI");
    }

    #[test]
    fn secondary_dominants() {
        let c = Key::from_fifths(0, false);

        // A7 wants to resolve to Dm, the ii of C.
        assert_eq!(roman(c, &[69, 73, 76, 79]), "V7/ii");
        // D7 points at G, the V.
        assert_eq!(roman(c, &[62, 66, 69, 72]), "V7/V");
    }

    #[test]
    fn modes_of_degrees() {
        let c = Key::from_fifths(0, false);

        assert_eq!(c.mode_of_degree(0), "Ionian");
        assert_eq!(c.mode_of_degree(4), "Mixolydian");

        let a_minor = Key::from_fifths(0, true);
        assert_eq!(a_minor.mode_of_degree(0), "Aeolian");
    }

    #[test]
    fn estimating_a_key_without_a_signature() {
        // A stretch of C major.
        let mut weights = [0.0; 12];
        for (pc, weight) in [
            (0, 8.0),
            (2, 4.0),
            (4, 6.0),
            (5, 4.0),
            (7, 7.0),
            (9, 3.0),
            (11, 2.0),
        ] {
            weights[pc] = weight;
        }

        assert_eq!(Key::estimate(&weights).name(), "C major");
    }

    #[test]
    fn scale_spelling_follows_the_signature() {
        assert_eq!(
            Key::from_fifths(-3, true).scale_names(),
            ["C", "D", "Eb", "F", "G", "Ab", "Bb"]
        );
    }
}
