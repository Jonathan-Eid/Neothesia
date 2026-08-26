mod backend;
mod cli;
mod key_estimate;
mod model;
mod role;
mod schema;
mod span;
mod window;

use std::{collections::HashMap, time::Duration};

use midi_file::MidiFile;
use music_theory::{Chord, Key, note_name_with_octave};

use backend::{AnalysisBackend, ClaudeCliBackend, SpanNoteText, SpanRequest, Validated};
use model::{NoteId, Score};
use schema::{
    AnalysisFile, KeyHint, LlmUsage, NoteAnalysis, NoteRole, Resolution, SpanAnalysis, SpanOutcome,
};
use window::{ExpandCap, ExpandOutcome};

fn key_hint(key: &Key) -> KeyHint {
    KeyHint {
        tonic: music_theory::note_name(key.tonic, key.prefers_flats()),
        minor: key.minor,
        fifths: key.fifths,
    }
}

struct Resolved {
    chord: Chord,
    window_note_ids: Vec<NoteId>,
    resolution: Resolution,
}

/// An anchor that didn't resolve, plus whatever the expander found anyway -
/// the window and best (too-weak) candidate, which become the LLM prompt's
/// context and the grouping key for merging anchors into spans.
struct Unresolved {
    id: NoteId,
    window: Vec<NoteId>,
    best_candidate: Option<Chord>,
}

fn main() {
    let args = cli::Args::get();

    let file = match MidiFile::new(&args.input) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("Could not load {:?}: {err}", args.input);
            std::process::exit(1);
        }
    };

    let score = Score::from_midi_file(&file);
    if score.is_empty() {
        eprintln!("No notes found in {:?}", args.input);
        std::process::exit(1);
    }

    let cap = ExpandCap::default();

    let mut resolved: Vec<Option<Resolved>> = Vec::with_capacity(score.len());
    let mut unresolved: Vec<Unresolved> = Vec::new();

    for note in score.notes.iter() {
        let id = note.id;

        if let Some((chord, window_note_ids)) = window::resolve_vertical(&score, id) {
            resolved.push(Some(Resolved {
                chord,
                window_note_ids,
                resolution: Resolution::Vertical,
            }));
            continue;
        }

        let result = window::expand_window(&score, id, &cap);
        if result.outcome == ExpandOutcome::Resolved {
            resolved.push(Some(Resolved {
                chord: result.best_chord.expect("resolved implies a chord"),
                window_note_ids: result.window_note_ids,
                resolution: Resolution::Expanded,
            }));
        } else {
            resolved.push(None);
            unresolved.push(Unresolved {
                id,
                window: result.window_note_ids,
                best_candidate: result.best_chord,
            });
        }
    }

    let unresolved_pairs: Vec<(NoteId, Vec<NoteId>)> = unresolved
        .iter()
        .map(|u| (u.id, u.window.clone()))
        .collect();
    let spans = span::merge_spans(&unresolved_pairs);
    let best_candidate_of: HashMap<usize, &Chord> = unresolved
        .iter()
        .filter_map(|u| u.best_candidate.as_ref().map(|c| (u.id.0, c)))
        .collect();

    let mut span_of: HashMap<usize, usize> = HashMap::new();
    for (span_id, s) in spans.iter().enumerate() {
        for id in &s.note_ids {
            span_of.insert(id.0, span_id);
        }
    }
    let window_of: HashMap<usize, Vec<NoteId>> = unresolved
        .iter()
        .map(|u| (u.id.0, u.window.clone()))
        .collect();

    // Nearest resolved chord's symbol at or before/after a note index - the
    // cheapest, strongest disambiguating context to hand the LLM.
    let nearby_chord_symbol = |from: usize, forward: bool| -> Option<String> {
        if forward {
            resolved
                .get(from..)?
                .iter()
                .flatten()
                .next()
                .map(|r| r.chord.symbol(false))
        } else {
            resolved
                .get(..from)?
                .iter()
                .rev()
                .flatten()
                .next()
                .map(|r| r.chord.symbol(false))
        }
    };

    let requests: Vec<SpanRequest> = spans
        .iter()
        .enumerate()
        .map(|(span_id, s)| {
            let notes = s
                .note_ids
                .iter()
                .map(|id| {
                    let note = score.get(*id);
                    let (measure, beat) = score.position_of(note.start);
                    SpanNoteText {
                        measure,
                        beat,
                        pitch_name: note_name_with_octave(note.pitch, false),
                    }
                })
                .collect();

            let first = s.note_ids.first().unwrap().0;
            let last = s.note_ids.last().unwrap().0;

            SpanRequest {
                span_id,
                notes,
                best_candidate: best_candidate_of
                    .get(&first)
                    .map(|c| (c.symbol(false), c.confidence)),
                preceding_chord: nearby_chord_symbol(first, false),
                following_chord: nearby_chord_symbol(last + 1, true),
            }
        })
        .collect();

    let mut span_outcomes: Vec<SpanOutcome> =
        spans.iter().map(|_| SpanOutcome::LeftUnresolved).collect();
    let mut llm_usage: Option<LlmUsage> = None;

    if !args.no_llm && !requests.is_empty() {
        let backend = ClaudeCliBackend::default();
        match backend.resolve_spans(&requests) {
            Ok((responses, usage)) => {
                llm_usage = Some(usage);
                for response in responses {
                    let Some(s) = spans.get(response.span_id) else {
                        continue;
                    };
                    let anchor = s.note_ids[0];

                    match backend::validate(&score, anchor, &response) {
                        Validated::Accepted { chord } => {
                            for id in &s.note_ids {
                                resolved[id.0] = Some(Resolved {
                                    chord: chord.clone(),
                                    window_note_ids: window_of
                                        .get(&id.0)
                                        .cloned()
                                        .unwrap_or_default(),
                                    resolution: Resolution::Escalated,
                                });
                            }
                            span_outcomes[response.span_id] = SpanOutcome::ResolvedByLlm {
                                chord_symbol: chord.symbol(false),
                                confidence: response.confidence,
                                reasoning: response.reasoning,
                            };
                        }
                        Validated::Rejected { reason } => {
                            span_outcomes[response.span_id] =
                                SpanOutcome::RejectedLlmAnswer { reason };
                        }
                    }
                }
            }
            Err(err) => {
                let message = format!("{err:?}");
                for outcome in span_outcomes.iter_mut() {
                    *outcome = SpanOutcome::LlmBackendError {
                        message: message.clone(),
                    };
                }
            }
        }
    }

    let notes: Vec<NoteAnalysis> = score
        .notes
        .iter()
        .map(|note| {
            let id = note.id;
            let (measure, beat) = score.position_of(note.start);
            let local_key = key_estimate::local_key(&score, &score.signature_track, note.start);
            let flats = local_key.prefers_flats();

            let (
                resolution,
                chord_symbol,
                chord_description,
                roman_numeral,
                role,
                confidence,
                window_note_ids,
            ) = match &resolved[id.0] {
                Some(r) => (
                    r.resolution,
                    Some(r.chord.symbol(flats)),
                    Some(r.chord.description(flats)),
                    Some(local_key.roman(&r.chord)),
                    role::classify_role(&score, id, &r.chord),
                    Some(r.chord.confidence),
                    r.window_note_ids.iter().map(|w| w.0).collect(),
                ),
                None => (
                    Resolution::Unresolved,
                    None,
                    None,
                    None,
                    NoteRole::Unclassified,
                    None,
                    window_of
                        .get(&id.0)
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .map(|w| w.0)
                        .collect(),
                ),
            };

            NoteAnalysis {
                id: id.0,
                track_id: note.track_id,
                track_name: note.track_name.clone(),
                start_seconds: note.start.as_secs_f64(),
                duration_seconds: note.end.saturating_sub(note.start).as_secs_f64(),
                pitch: note.pitch,
                pitch_name: note_name_with_octave(note.pitch, flats),
                measure,
                beat,
                beat_strength: role::beat_strength(&score, id),
                resolution,
                chord_symbol,
                chord_description,
                roman_numeral,
                role,
                local_key: Some(key_hint(&local_key)),
                window_note_ids,
                confidence,
                span_id: span_of.get(&id.0).copied(),
            }
        })
        .collect();

    let span_analyses: Vec<SpanAnalysis> = spans
        .iter()
        .enumerate()
        .map(|(id, s)| {
            let measures: Vec<usize> = s
                .note_ids
                .iter()
                .map(|n| score.position_of(score.get(*n).start).0)
                .collect();
            let best = s
                .note_ids
                .first()
                .and_then(|first| best_candidate_of.get(&first.0))
                .map(|c| format!("{} (score {})", c.symbol(false), c.confidence));

            SpanAnalysis {
                id,
                note_ids: s.note_ids.iter().map(|n| n.0).collect(),
                measure_range: (
                    measures.iter().copied().min().unwrap_or(0),
                    measures.iter().copied().max().unwrap_or(0),
                ),
                best_deterministic_candidate: best,
                outcome: std::mem::replace(&mut span_outcomes[id], SpanOutcome::LeftUnresolved),
            }
        })
        .collect();

    let file_key = file
        .signature_track
        .key_signature_at(&Duration::ZERO)
        .map(|k| key_hint(&Key::from_fifths(k.fifths, k.minor)));

    let unresolved_count = notes
        .iter()
        .filter(|n| n.resolution == Resolution::Unresolved)
        .count();
    let notes_len = notes.len();
    let spans_len = spans.len();

    let analysis = AnalysisFile {
        version: 1,
        source_file: args
            .input
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        key_hint: file_key,
        notes,
        spans: span_analyses,
        llm_usage,
    };

    let out_path = args.out.unwrap_or_else(|| {
        let mut p = args.input.clone();
        let stem = p
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        p.set_file_name(format!("{stem}.analysis.json"));
        p
    });

    let json = serde_json::to_string_pretty(&analysis).expect("serialize analysis");
    if let Err(err) = std::fs::write(&out_path, json) {
        eprintln!("Could not write {out_path:?}: {err}");
        std::process::exit(1);
    }

    eprintln!(
        "{}: {notes_len} notes, {spans_len} spans escalated, {unresolved_count} still unresolved -> {out_path:?}",
        analysis.source_file,
    );
    if let Some(usage) = &analysis.llm_usage {
        eprintln!(
            "  llm: {} call(s), {} in / {} out tokens ({} cache read, {} cache write), ${:.4}",
            usage.calls,
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_input_tokens,
            usage.cache_creation_input_tokens,
            usage.total_cost_usd,
        );
    }
}
