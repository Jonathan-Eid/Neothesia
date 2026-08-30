mod file;
pub mod musicxml;
pub mod playback;
pub mod program_track;
pub mod signature_track;
pub mod tempo_track;
mod track;

pub use file::*;
pub use midly;
pub use playback::*;
pub use signature_track::{KeySignature, SignatureTrack, TimeSignature};
pub use track::*;

/// Which staff a track is written on, as declared by the MusicXML score.
/// `None` for a plain midi file, which carries no such information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clef {
    Treble,
    Bass,
    Alto,
    Percussion,
}

/// A note's written note value (`<type>` in MusicXML) - what shape it would
/// be engraved with, independent of how many augmentation dots lengthen it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteValue {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

impl NoteValue {
    /// How many flags a lone (unbeamed) note of this value gets.
    pub fn flags(self) -> u8 {
        match self {
            NoteValue::Whole | NoteValue::Half | NoteValue::Quarter => 0,
            NoteValue::Eighth => 1,
            NoteValue::Sixteenth => 2,
            NoteValue::ThirtySecond => 3,
            NoteValue::SixtyFourth => 4,
        }
    }
}

/// Where a note sits in a beamed group (`<beam number="1">` in MusicXML).
/// `forward hook`/`backward hook` both collapse into `Continue` - a
/// dedicated renderer would draw them as a stub instead of a full beam
/// segment, but a plain sheet view can treat them the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeamState {
    Begin,
    Continue,
    End,
}

/// A note exactly as written in the score, kept separately from the
/// flattened midi key/timing so that, e.g., a written C#4 is never confused
/// with its enharmonic twin Db4 (same key, different line on the staff), and
/// a quarter note tied into a half isn't drawn as one fabricated whole note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotationInfo {
    /// 'A'..='G'
    pub step: char,
    /// -2 (double flat) ..= 2 (double sharp)
    pub alter: i8,
    pub octave: i32,
    /// `None` when the score didn't spell out a `<type>` for this note -
    /// callers should fall back to inferring a shape from the actual
    /// sounding duration.
    pub value: Option<NoteValue>,
    /// Number of `<dot/>` augmentation dots.
    pub dots: u8,
    /// `None` when this note isn't part of a beamed group.
    pub beam: Option<BeamState>,
}

pub static INSTRUMENT_NAMES: [&str; 128] = [
    "Acoustic Grand Piano",
    "Bright Acoustic Piano",
    "Electric Grand Piano",
    "Honky-tonk Piano",
    "Electric Piano 1",
    "Electric Piano 2",
    "Harpsichord",
    "Clavi",
    "Celesta",
    "Glockenspiel",
    "Music Box",
    "Vibraphone",
    "Marimba",
    "Xylophone",
    "Tubular Bells",
    "Dulcimer",
    "Drawbar Organ",
    "Percussive Organ",
    "Rock Organ",
    "Church Organ",
    "Reed Organ",
    "Accordion",
    "Harmonica",
    "Tango Accordion",
    "Acoustic Guitar (nylon)",
    "Acoustic Guitar (steel)",
    "Electric Guitar (jazz)",
    "Electric Guitar (clean)",
    "Electric Guitar (muted)",
    "Overdriven Guitar",
    "Distortion Guitar",
    "Guitar harmonics",
    "Acoustic Bass",
    "Electric Bass (finger)",
    "Electric Bass (pick)",
    "Fretless Bass",
    "Slap Bass 1",
    "Slap Bass 2",
    "Synth Bass 1",
    "Synth Bass 2",
    "Violin",
    "Viola",
    "Cello",
    "Contrabass",
    "Tremolo Strings",
    "Pizzicato Strings",
    "Orchestral Harp",
    "Timpani",
    "String Ensemble 1",
    "String Ensemble 2",
    "SynthStrings 1",
    "SynthStrings 2",
    "Choir Aahs",
    "Voice Oohs",
    "Synth Voice",
    "Orchestra Hit",
    "Trumpet",
    "Trombone",
    "Tuba",
    "Muted Trumpet",
    "French Horn",
    "Brass Section",
    "SynthBrass 1",
    "SynthBrass 2",
    "Soprano Sax",
    "Alto Sax",
    "Tenor Sax",
    "Baritone Sax",
    "Oboe",
    "English Horn",
    "Bassoon",
    "Clarinet",
    "Piccolo",
    "Flute",
    "Recorder",
    "Pan Flute",
    "Blown Bottle",
    "Shakuhachi",
    "Whistle",
    "Ocarina",
    "Lead 1 (square)",
    "Lead 2 (sawtooth)",
    "Lead 3 (calliope)",
    "Lead 4 (chiff)",
    "Lead 5 (charang)",
    "Lead 6 (voice)",
    "Lead 7 (fifths)",
    "Lead 8 (bass + lead)",
    "Pad 1 (new age)",
    "Pad 2 (warm)",
    "Pad 3 (polysynth)",
    "Pad 4 (choir)",
    "Pad 5 (bowed)",
    "Pad 6 (metallic)",
    "Pad 7 (halo)",
    "Pad 8 (sweep)",
    "FX 1 (rain)",
    "FX 2 (soundtrack)",
    "FX 3 (crystal)",
    "FX 4 (atmosphere)",
    "FX 5 (brightness)",
    "FX 6 (goblins)",
    "FX 7 (echoes)",
    "FX 8 (sci-fi)",
    "Sitar",
    "Banjo",
    "Shamisen",
    "Koto",
    "Kalimba",
    "Bag pipe",
    "Fiddle",
    "Shanai",
    "Tinkle Bell",
    "Agogo",
    "Steel Drums",
    "Woodblock",
    "Taiko Drum",
    "Melodic Tom",
    "Synth Drum",
    "Reverse Cymbal",
    "Guitar Fret Noise",
    "Breath Noise",
    "Seashore",
    "Bird Tweet",
    "Telephone Ring",
    "Helicopter",
    "Applause",
    "Gunshot",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load() {
        let _midi = MidiFile::new("../test.mid").unwrap();
    }

    #[test]
    fn load_musicxml() {
        let midi = MidiFile::new("./test-assets/test.musicxml").unwrap();

        // One tempo track, plus one track per staff.
        assert_eq!(midi.tracks.len(), 3);

        let notes: Vec<_> = midi
            .tracks
            .iter()
            .flat_map(|track| track.notes.iter())
            .collect();
        assert!(!notes.is_empty());
    }

    /// The test score is two measures long, the first one is repeated, so it is
    /// played as measure one, measure one, measure two.
    #[test]
    fn musicxml_timings() {
        let midi = MidiFile::new("./test-assets/test.musicxml").unwrap();

        let micros = |d: std::time::Duration| d.as_micros();

        // Three played measures, plus the end of the last one.
        assert_eq!(midi.measures.len(), 4);
        assert_eq!(micros(midi.measures[1]), 2_000_000);
        assert_eq!(micros(midi.measures[2]), 4_000_000);
        assert_eq!(micros(midi.measures[3]), 6_000_000);

        let sorted = |track: &MidiTrack| {
            let mut notes = track.notes.to_vec();
            notes.sort_by_key(|note| (note.start, note.note));
            notes
        };

        let upper = sorted(&midi.tracks[1]);
        let lower = sorted(&midi.tracks[2]);

        let note_at = |notes: &[MidiNote], id: usize| {
            let note = &notes[id];
            (note.note, micros(note.start), micros(note.duration))
        };

        assert_eq!(upper.len(), 8);
        // The two tied quarters became a single half note.
        assert_eq!(note_at(&upper, 0), (72, 0, 1_000_000));
        // Chord, both notes start together.
        assert_eq!(note_at(&upper, 1), (75, 1_000_000, 1_000_000));
        assert_eq!(note_at(&upper, 2), (79, 1_000_000, 1_000_000));
        // Second pass of the repeated measure.
        assert_eq!(note_at(&upper, 3), (72, 2_000_000, 1_000_000));
        // The grace note is squeezed in right before the note it decorates.
        assert_eq!(note_at(&upper, 6), (71, 4_968_750, 31_250));
        assert_eq!(note_at(&upper, 7), (72, 5_000_000, 1_000_000));

        assert_eq!(lower.len(), 3);
        assert_eq!(note_at(&lower, 0), (48, 0, 2_000_000));
        assert_eq!(note_at(&lower, 1), (48, 2_000_000, 2_000_000));
        assert_eq!(note_at(&lower, 2), (43, 4_000_000, 2_000_000));
    }

    /// `D.S. al Coda`: measures one to four, back to the segno, then from the
    /// `to coda` sign straight to the coda.
    #[test]
    fn musicxml_jumps() {
        let midi = MidiFile::new("./test-assets/test-jumps.musicxml").unwrap();

        let mut notes: Vec<_> = midi
            .tracks
            .iter()
            .flat_map(|track| track.notes.iter())
            .collect();
        notes.sort_by_key(|note| note.start);

        let keys: Vec<u8> = notes.iter().map(|note| note.note).collect();
        assert_eq!(keys, [60, 62, 64, 65, 60, 62, 64, 67]);
    }

    #[test]
    fn musicxml_signatures() {
        let midi = MidiFile::new("./test-assets/test.musicxml").unwrap();

        let start = std::time::Duration::ZERO;
        let time = midi.signature_track.time_signature_at(&start);
        assert_eq!((time.numerator, time.denominator), (4, 4));

        let key = midi.signature_track.key_signature_at(&start).unwrap();
        assert_eq!((key.fifths, key.minor), (-3, true));
    }

    /// Measure lines follow the time signature, not a hardcoded four beats.
    #[test]
    fn midi_measures_follow_meter() {
        let midi = MidiFile::new("./test-assets/test-jumps.musicxml").unwrap();

        let time = midi
            .signature_track
            .time_signature_at(&std::time::Duration::ZERO);
        assert_eq!((time.numerator, time.denominator), (1, 4));
    }

    /// Written pitch spelling and clef survive the conversion, even though
    /// the midi events underneath only carry a flattened key number.
    #[test]
    fn musicxml_notation_preserved() {
        let midi = MidiFile::new("./test-assets/test.musicxml").unwrap();

        assert_eq!(midi.tracks[1].clef, Some(Clef::Treble));
        assert_eq!(midi.tracks[2].clef, Some(Clef::Bass));

        let upper = &midi.tracks[1].notes;

        // The tied C5s merge into one note, but keep their written spelling
        // and note value of the segment that started the tie.
        let c5 = upper.iter().find(|n| n.note == 72).unwrap();
        assert_eq!(
            c5.notation,
            Some(NotationInfo {
                step: 'C',
                alter: 0,
                octave: 5,
                value: Some(NoteValue::Quarter),
                dots: 0,
                beam: None,
            })
        );

        // Written as an E-flat, not its enharmonic twin D-sharp.
        let e_flat = upper.iter().find(|n| n.note == 75).unwrap();
        assert_eq!(
            e_flat.notation,
            Some(NotationInfo {
                step: 'E',
                alter: -1,
                octave: 5,
                value: Some(NoteValue::Half),
                dots: 0,
                beam: None,
            })
        );
    }

    /// The exact engraved shape - note value, dots, beam grouping - comes
    /// straight from the score's `<type>`/`<dot>`/`<beam>` elements, not a
    /// guess from how long the note actually plays.
    #[test]
    fn musicxml_rhythm_preserved() {
        let midi = MidiFile::new("./test-assets/test-beams.musicxml").unwrap();
        let notes = &midi.tracks[1].notes;

        let c5 = notes.iter().find(|n| n.note == 72).unwrap();
        assert_eq!(c5.notation.unwrap().value, Some(NoteValue::Eighth));
        assert_eq!(c5.notation.unwrap().beam, Some(BeamState::Begin));

        let d5 = notes.iter().find(|n| n.note == 74).unwrap();
        assert_eq!(d5.notation.unwrap().value, Some(NoteValue::Eighth));
        assert_eq!(d5.notation.unwrap().beam, Some(BeamState::End));

        // Dotted quarter: three eighth-note-divisions long.
        let e5 = notes.iter().find(|n| n.note == 76).unwrap();
        assert_eq!(e5.notation.unwrap().value, Some(NoteValue::Quarter));
        assert_eq!(e5.notation.unwrap().dots, 1);
        assert_eq!(e5.notation.unwrap().beam, None);
    }

    #[test]
    fn load_mxl() {
        let midi = MidiFile::new("./test-assets/test.mxl").unwrap();
        assert!(midi.tracks.iter().any(|track| !track.notes.is_empty()));
    }
}
