//! The shape of `<file>.analysis.json`: per-note chord/scale/role analysis
//! produced by `mxl-analyze` and read back by anything that wants to enrich a
//! song with it (namely Neothesia's theory panel). Shared so the writer and
//! the reader can never drift apart into two slightly different formats.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisFile {
    pub version: u32,
    pub source_file: String,
    pub key_hint: Option<KeyHint>,
    pub notes: Vec<NoteAnalysis>,
    pub spans: Vec<SpanAnalysis>,
    pub llm_usage: Option<LlmUsage>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct KeyHint {
    pub tonic: u8,
    pub minor: bool,
    pub fifths: i8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    Vertical,
    Expanded,
    Escalated,
    /// Correctly, not a failure: the widest window still contains only one
    /// pitch class - a pedal point or a droning ostinato, not a chord at all.
    /// Asking an LLM to name a triad for it would just manufacture one.
    PedalTone,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeatStrength {
    Strong,
    Medium,
    Weak,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum NoteRole {
    ChordTone { interval: String },
    PassingTone,
    NeighborTone,
    Appoggiatura,
    Suspension,
    Unclassified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteAnalysis {
    pub id: usize,
    pub track_id: usize,
    pub track_name: Option<String>,
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub pitch: u8,
    pub pitch_name: String,
    pub measure: usize,
    pub beat: f32,
    pub beat_strength: BeatStrength,

    pub resolution: Resolution,
    pub chord_symbol: Option<String>,
    pub chord_description: Option<String>,
    pub roman_numeral: Option<String>,
    pub role: NoteRole,
    pub local_key: Option<KeyHint>,

    pub window_note_ids: Vec<usize>,
    pub confidence: Option<i32>,
    pub span_id: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanAnalysis {
    pub id: usize,
    pub note_ids: Vec<usize>,
    pub measure_range: (usize, usize),
    pub best_deterministic_candidate: Option<String>,
    pub outcome: SpanOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum SpanOutcome {
    ResolvedByLlm {
        chord_symbol: String,
        confidence: String,
        reasoning: String,
    },
    RejectedLlmAnswer {
        reason: String,
    },
    LlmBackendError {
        message: String,
    },
    LeftUnresolved,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmUsage {
    pub calls: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub total_cost_usd: f64,
}
