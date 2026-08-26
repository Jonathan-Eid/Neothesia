//! Loads `<file>.analysis.json`, if `mxl-analyze` has been run on the song,
//! so the live theory panel can show its per-note chord/scale/role instead
//! of only what's sounding at this exact instant - which is blank for a
//! monophonic run, since there's nothing to detect a chord from.
//!
//! Purely additive: missing, unreadable, or stale files just mean nothing
//! gets enriched, never an error the player sees.

use std::{collections::HashMap, path::Path, time::Duration};

use song_analysis::{AnalysisFile, NoteAnalysis};

#[derive(Debug)]
pub struct PrecomputedAnalysis {
    file: AnalysisFile,
    /// (track_id, start in whole milliseconds, pitch) -> index into `file.notes`.
    /// Milliseconds rather than the raw seconds float, so a note found here by
    /// the live player (going through the same `Duration` the file itself was
    /// analysed from) always matches exactly, with no float-equality risk.
    by_key: HashMap<(usize, u64, u8), usize>,
}

impl PrecomputedAnalysis {
    pub fn load_sibling(source: &Path) -> Option<Self> {
        let stem = source.file_stem()?.to_string_lossy().into_owned();
        let path = source.with_file_name(format!("{stem}.analysis.json"));

        let bytes = std::fs::read(&path).ok()?;
        let file: AnalysisFile = serde_json::from_slice(&bytes)
            .inspect_err(|err| log::warn!("Could not read {path:?}: {err}"))
            .ok()?;

        Some(Self::from_file(file))
    }

    fn from_file(file: AnalysisFile) -> Self {
        let by_key = file
            .notes
            .iter()
            .enumerate()
            .map(|(id, note)| {
                let start_ms = (note.start_seconds * 1000.0).round() as u64;
                ((note.track_id, start_ms, note.pitch), id)
            })
            .collect();

        Self { file, by_key }
    }

    pub fn lookup(&self, track_id: usize, start: Duration, pitch: u8) -> Option<&NoteAnalysis> {
        // Round rather than truncate - `Duration`'s float round-trip can land
        // a hair under a millisecond boundary, and a truncating cast would
        // turn that into an off-by-one miss against the file's own rounding.
        let start_ms = (start.as_secs_f64() * 1000.0).round() as u64;
        self.by_key
            .get(&(track_id, start_ms, pitch))
            .map(|id| &self.file.notes[*id])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(track_id: usize, start_seconds: f64, pitch: u8) -> NoteAnalysis {
        NoteAnalysis {
            id: 0,
            track_id,
            track_name: None,
            start_seconds,
            duration_seconds: 0.5,
            pitch,
            pitch_name: "C4".to_string(),
            measure: 1,
            beat: 0.0,
            beat_strength: song_analysis::BeatStrength::Strong,
            resolution: song_analysis::Resolution::Vertical,
            chord_symbol: Some("C".to_string()),
            chord_description: None,
            roman_numeral: Some("I".to_string()),
            role: song_analysis::NoteRole::ChordTone {
                interval: "1".to_string(),
            },
            local_key: None,
            window_note_ids: vec![],
            confidence: Some(37),
            span_id: None,
        }
    }

    #[test]
    fn looks_up_a_note_by_track_time_and_pitch() {
        let analysis = PrecomputedAnalysis::from_file(AnalysisFile {
            version: 1,
            source_file: "test".to_string(),
            key_hint: None,
            notes: vec![note(0, 1.234, 60)],
            spans: vec![],
            llm_usage: None,
        });

        let found = analysis
            .lookup(0, Duration::from_secs_f64(1.234), 60)
            .unwrap();
        assert_eq!(found.chord_symbol.as_deref(), Some("C"));

        assert!(
            analysis
                .lookup(0, Duration::from_secs_f64(1.235), 60)
                .is_none()
        );
        assert!(
            analysis
                .lookup(1, Duration::from_secs_f64(1.234), 60)
                .is_none()
        );
        assert!(
            analysis
                .lookup(0, Duration::from_secs_f64(1.234), 61)
                .is_none()
        );
    }
}
