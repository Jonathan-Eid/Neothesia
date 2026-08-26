//! Shells out to `claude -p`, batching every span into one call and one
//! session, so the fixed cost of starting a session (its cached system
//! prompt - a few thousand tokens regardless of what's asked) is paid once
//! per analysis run.

use std::process::Command;

use song_analysis::LlmUsage;

use super::{AnalysisBackend, BackendError, SpanRequest, SpanResponse};

pub struct ClaudeCliBackend {
    pub max_budget_usd: f64,
}

impl Default for ClaudeCliBackend {
    fn default() -> Self {
        Self {
            max_budget_usd: 0.50,
        }
    }
}

const SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "resolutions": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "span_id": { "type": "integer" },
          "root_pc": { "type": "integer" },
          "shape_symbol": { "type": "string" },
          "confidence": { "type": "string" },
          "reasoning": { "type": "string" }
        },
        "required": ["span_id", "root_pc", "shape_symbol", "confidence", "reasoning"],
        "additionalProperties": false
      }
    }
  },
  "required": ["resolutions"],
  "additionalProperties": false
}"#;

fn build_prompt(requests: &[SpanRequest]) -> String {
    let mut out = String::new();
    out.push_str(
        "You are doing tonal harmonic analysis of short spans of notes from a \
         score, each one a run our deterministic chord-matcher could not \
         confidently resolve on its own (usually because the notes are \
         played one at a time - an arpeggio or a melodic run - rather than \
         stacked as a chord). For each span, decide the single underlying \
         harmony the notes outline or embellish. root_pc is a pitch class \
         0-11 (0=C, 1=C#, ... 11=B). shape_symbol must be one of the standard \
         chord suffixes (e.g. \"\", \"m\", \"7\", \"maj7\", \"m7\", \"dim\", \
         \"sus4\", \"6\", \"9\") written the way a chord chart would. \
         confidence is a short phrase (e.g. \"high\", \"low, ambiguous\"), \
         reasoning is one sentence.\n\n",
    );

    for req in requests {
        out.push_str(&format!("Span {}:\n", req.span_id));
        if let Some(chord) = &req.preceding_chord {
            out.push_str(&format!("  preceding harmony: {chord}\n"));
        }
        out.push_str("  notes:\n");
        for note in &req.notes {
            out.push_str(&format!(
                "    m{} beat {:.2}: {}\n",
                note.measure, note.beat, note.pitch_name
            ));
        }
        if let Some(chord) = &req.following_chord {
            out.push_str(&format!("  following harmony: {chord}\n"));
        }
        if let Some((symbol, confidence)) = &req.best_candidate {
            out.push_str(&format!(
                "  our best (rejected) deterministic guess: {symbol} (score {confidence})\n"
            ));
        }
        out.push('\n');
    }

    out
}

impl AnalysisBackend for ClaudeCliBackend {
    fn resolve_spans(
        &self,
        requests: &[SpanRequest],
    ) -> Result<(Vec<SpanResponse>, LlmUsage), BackendError> {
        if requests.is_empty() {
            return Ok((Vec::new(), LlmUsage::default()));
        }

        let prompt = build_prompt(requests);

        let output = Command::new("claude")
            .arg("-p")
            .arg(&prompt)
            .arg("--output-format")
            .arg("json")
            .arg("--tools")
            .arg("")
            .arg("--max-budget-usd")
            .arg(self.max_budget_usd.to_string())
            .arg("--json-schema")
            .arg(SCHEMA)
            .output()
            .map_err(|e| BackendError::Process(e.to_string()))?;

        if !output.status.success() {
            return Err(BackendError::Process(format!(
                "claude exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let envelope: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| BackendError::ParseJson(format!("outer envelope: {e}")))?;

        if envelope.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
            return Err(BackendError::Process(format!(
                "claude reported an error: {}",
                envelope
                    .get("result")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<no message>")
            )));
        }

        let usage = LlmUsage {
            calls: 1,
            input_tokens: envelope["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: envelope["usage"]["output_tokens"].as_u64().unwrap_or(0),
            cache_creation_input_tokens: envelope["usage"]["cache_creation_input_tokens"]
                .as_u64()
                .unwrap_or(0),
            cache_read_input_tokens: envelope["usage"]["cache_read_input_tokens"]
                .as_u64()
                .unwrap_or(0),
            total_cost_usd: envelope["total_cost_usd"].as_f64().unwrap_or(0.0),
        };

        let structured = envelope.get("structured_output").ok_or_else(|| {
            BackendError::ParseJson("no structured_output in response".to_string())
        })?;

        let resolutions = structured
            .get("resolutions")
            .ok_or_else(|| BackendError::ParseJson("no resolutions array".to_string()))?;

        let responses: Vec<SpanResponse> = serde_json::from_value(resolutions.clone())
            .map_err(|e| BackendError::ParseJson(format!("resolutions array: {e}")))?;

        Ok((responses, usage))
    }
}
