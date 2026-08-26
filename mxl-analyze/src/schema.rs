//! `song.analysis.json`'s shape. Emit-only - freshly regenerated each run, so
//! a single version tag is enough, no need for the config crate's
//! backward-compatible `V1(...)` enum ceremony.

use serde::Serialize;

#[derive(Serialize)]
pub struct AnalysisFile {
    pub version: u32,
    pub source_file: String,
    pub key_hint: Option<KeyHint>,
    pub notes: Vec<NoteAnalysis>,
    pub spans: Vec<SpanAnalysis>,
    pub llm_usage: Option<LlmUsage>,
}

#[derive(Serialize)]
pub struct KeyHint {
    pub tonic: &'static str,
    pub minor: bool,
    pub fifths: i8,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    Vertical,
    Expanded,
    Escalated,
    Unresolved,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BeatStrength {
    Strong,
    Medium,
    Weak,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum NoteRole {
    ChordTone { interval: &'static str },
    PassingTone,
    NeighborTone,
    Appoggiatura,
    Suspension,
    Unclassified,
}

#[derive(Serialize)]
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

#[derive(Serialize)]
pub struct SpanAnalysis {
    pub id: usize,
    pub note_ids: Vec<usize>,
    pub measure_range: (usize, usize),
    pub best_deterministic_candidate: Option<String>,
    pub outcome: SpanOutcome,
}

#[derive(Serialize)]
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

#[derive(Serialize, Default)]
pub struct LlmUsage {
    pub calls: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub total_cost_usd: f64,
}
