//! What resolves the residual, genuinely ambiguous spans stage 1 couldn't.

mod claude_cli;
pub use claude_cli::ClaudeCliBackend;

#[cfg(test)]
mod test_double;
#[cfg(test)]
pub use test_double::FixedBackend;

use music_theory::chord::SHAPES;

use crate::{
    model::{NoteId, Score},
    role::plausibly_belongs_to,
    schema::LlmUsage,
};

pub struct SpanNoteText {
    pub measure: usize,
    pub beat: f32,
    pub pitch_name: String,
}

pub struct SpanRequest {
    pub span_id: usize,
    pub notes: Vec<SpanNoteText>,
    pub best_candidate: Option<(String, i32)>,
    pub preceding_chord: Option<String>,
    pub following_chord: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SpanResponse {
    pub span_id: usize,
    pub root_pc: u8,
    pub shape_symbol: String,
    pub confidence: String,
    pub reasoning: String,
}

#[derive(Debug)]
#[allow(dead_code)] // fields read via Debug when reporting errors
pub enum BackendError {
    Process(String),
    ParseJson(String),
}

pub trait AnalysisBackend {
    /// Resolves every requested span in one call - not one call per span, so
    /// the fixed per-invocation overhead (session setup, cached system
    /// prompt) is paid once per analysis run rather than once per ambiguity.
    fn resolve_spans(
        &self,
        requests: &[SpanRequest],
    ) -> Result<(Vec<SpanResponse>, LlmUsage), BackendError>;
}

pub enum Validated {
    Accepted { chord: music_theory::Chord },
    Rejected { reason: String },
}

/// Backend-agnostic, so it's testable against canned responses without a
/// real process. Rejects an unknown shape outright; for a known shape,
/// accepts only if the anchor is a chord tone of it *or* plausibly a
/// non-chord tone approaching/leaving it - a bare pitch-class-membership
/// check would reject exactly the answers this system exists to get right,
/// since an escalated anchor is disproportionately the embellishing note.
pub fn validate(score: &Score, anchor: NoteId, response: &SpanResponse) -> Validated {
    let Some(shape) = SHAPES.iter().find(|s| s.symbol == response.shape_symbol) else {
        return Validated::Rejected {
            reason: format!("unknown shape symbol {:?}", response.shape_symbol),
        };
    };

    let notes: Vec<u8> = shape
        .intervals
        .iter()
        .map(|interval| response.root_pc + interval)
        .collect();

    let Some(chord) = music_theory::detect(&notes) else {
        return Validated::Rejected {
            reason: "proposed shape did not re-detect as a chord".to_string(),
        };
    };

    if plausibly_belongs_to(score, anchor, &chord) {
        Validated::Accepted { chord }
    } else {
        Validated::Rejected {
            reason: "anchor note is neither a tone of the proposed chord nor a plausible embellishment of it".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AnalyzedNote;
    use std::{sync::Arc, time::Duration};

    fn note(id: usize, start_ms: u64, pitch: u8) -> AnalyzedNote {
        AnalyzedNote {
            id: NoteId(id),
            start: Duration::from_millis(start_ms),
            end: Duration::from_millis(start_ms + 200),
            pitch,
            track_id: 0,
            track_name: None,
        }
    }

    fn score(notes: Vec<AnalyzedNote>) -> Score {
        Score {
            notes,
            measures: Arc::from([Duration::ZERO, Duration::from_millis(1600)]),
            signature_track: midi_file::SignatureTrack::default(),
        }
    }

    fn response(shape: &str, root_pc: u8) -> SpanResponse {
        SpanResponse {
            span_id: 0,
            root_pc,
            shape_symbol: shape.to_string(),
            confidence: "high".to_string(),
            reasoning: "test".to_string(),
        }
    }

    #[test]
    fn fixed_backend_returns_its_canned_response_and_counts_the_call() {
        let backend = FixedBackend::new(vec![response("", 0)]);
        let (responses, _) = backend.resolve_spans(&[]).unwrap();
        assert_eq!(responses.len(), 1);
        assert_eq!(*backend.calls.borrow(), 1);
    }

    #[test]
    fn unknown_shape_symbol_is_rejected() {
        let s = score(vec![note(0, 0, 60)]);
        let bad = response("not-a-real-shape", 0);
        assert!(matches!(
            validate(&s, NoteId(0), &bad),
            Validated::Rejected { .. }
        ));
    }

    #[test]
    fn a_chord_tone_is_accepted() {
        // Anchor is E4 (pc 4), proposed chord is C major (root 0, "").
        let s = score(vec![note(0, 0, 64)]);
        let good = response("", 0);
        assert!(matches!(
            validate(&s, NoteId(0), &good),
            Validated::Accepted { .. }
        ));
    }

    #[test]
    fn a_passing_tone_is_accepted_not_rejected_for_missing_the_chord() {
        // D4 (pc 2) surrounded by C4 and E4 on a weak beat: a textbook
        // passing tone through a C major chord it is not itself a member of.
        // This is exactly the case naive pitch-class-membership would wrongly
        // reject.
        let s = score(vec![note(0, 0, 60), note(1, 100, 62), note(2, 200, 64)]);
        let good = response("", 0);
        assert!(matches!(
            validate(&s, NoteId(1), &good),
            Validated::Accepted { .. }
        ));
    }

    #[test]
    fn an_unrelated_note_is_rejected() {
        // F#4 (pc 6) against a C major chord, approached and left by leap:
        // not a chord tone and not a plausible embellishment either.
        let s = score(vec![note(0, 0, 48), note(1, 100, 66), note(2, 200, 79)]);
        let bad = response("", 0);
        assert!(matches!(
            validate(&s, NoteId(1), &bad),
            Validated::Rejected { .. }
        ));
    }
}
