# `mxl-analyze`: per-note contextual chord/scale analysis for MusicXML

## Context

The live theory panel (`neothesia/src/scene/playing_scene/analysis.rs`) only knows
harmony that's *literally sounding at this instant* — `music_theory::detect()`
needs ≥2 simultaneously-ringing notes. A monophonic run (an arpeggio, a scale
passage — e.g. the unaccompanied piano intro to Firth of Fifth) never produces
that, so every note in it shows blank: no chord, no roman numeral, no scale.
That's correct behavior for a real-time player (it has no business guessing),
but it means the tool can't answer "what's this note doing" for exactly the
passages a self-taught player most wants explained.

This adds a separate, offline analysis step: point `mxl-analyze` at a `.mxl`
file, it produces `<file>.analysis.json` next to it, with every note tagged
with the chord/scale/role it belongs to — resolved deterministically wherever
possible by widening a time window until enough context appears, and escalated
to an LLM (`claude -p`, agentic, tool-using) only for the residual handful of
genuinely ambiguous spans. Scope is `.mxl`/`.musicxml` input only for v1 (per
direction) — no need to harden for headless `.mid` files, even though the
underlying `midi_file::MidiFile::new` happens to parse both.

The live player is untouched by this — this is a new, separate offline tool.
Wiring the player to *consume* `<file>.analysis.json` if present is a later,
separate piece of work, not part of this plan.

## Two small changes to existing crates

**`music-theory/src/chord.rs`** — `Chord` currently discards its match score.
Add one field, populated at the one existing construction site inside
`detect()` (the `score` local is already in scope there):

```rust
pub struct Chord {
    pub root: u8,
    pub bass: u8,
    shape: usize,
    pub tensions: Vec<u8>,
    pub notes: Vec<u8>,
    pub confidence: i32, // raw match score; compare only against other Chord::confidence values
}
```
Confirmed safe: every existing test constructs chords only via `detect(...).unwrap()`
and asserts on `.symbol()`/`.description()`/`.inversion()` — never a struct
literal or whole-struct equality (`Chord` derives `Debug, Clone`, not `PartialEq`).

**`midi-file/src/musicxml/mod.rs`** — the LLM stage needs the raw extracted
`score.xml` on disk so `claude -p` can read/grep it directly. `from_musicxml`
already extracts and parses this internally but doesn't expose the bytes back.
Add a small additive accessor reusing the existing zip-reading path:
```rust
pub fn extract_musicxml(data: &[u8]) -> Result<Vec<u8>, String>
```
(Avoids `mxl-analyze` taking its own `zip` dependency just to re-extract what
`midi-file` already knows how to extract.)

## New crate: `mxl-analyze`

Binary crate, added to root `Cargo.toml` `[workspace] members` (not
`default-members` — invoked explicitly, same as `neothesia-cli`). No new
workspace dependencies needed: `serde`/`serde_json`/`clap`/`midi-file`/
`music-theory` are all already declared workspace-wide. `serde` needs
`features = ["derive"]` enabled per-crate (workspace table doesn't enable it;
`neothesia-core/Cargo.toml` does this the same way). The `claude -p` backend
uses `std::process::Command` — no HTTP client dependency for v1.

```
mxl-analyze/
  Cargo.toml
  src/
    main.rs            — CLI entry: parse args, MidiFile::new, run pipeline, write json
    cli.rs              — Args struct + clap builder Command (mirrors neothesia-cli/src/cli.rs's style — no derive feature is enabled workspace-wide, so use the builder API, not #[derive(Parser)])
    model.rs            — Score/AnalyzedNote: flattens all non-drum notes (filtered per-note on channel != 9, not per-track — a track can be mixed per has_drums/has_other_than_drums) across all tracks into one Vec sorted by (start, pitch); NoteId = index
    schema.rs           — serde structs for song.analysis.json (below)
    window.rs           — the core algorithm: resolve_vertical, expand_window (pure functions over &Score + NoteId)
    role.rs             — chord-tone / non-chord-tone classification (passing/neighbor/appoggiatura/suspension), using each note's same-track neighbors + beat strength
    key_estimate.rs      — local_key(): SignatureTrack::key_signature_at if present, else Key::estimate over a duration-weighted histogram of a 2-measure window around the anchor
    span.rs              — merge_spans(): groups adjacent unresolved anchors whose expansion windows overlap into one Span (lean signature: takes (NoteId, Vec<NoteId>) pairs, not full Score, so it's independently unit-testable)
    backend/
      mod.rs             — AnalysisBackend trait, SpanRequest/SpanResponse, and validate() (backend-agnostic, unit-testable without a real backend)
      claude_cli.rs       — shells out: `claude -p <prompt> --output-format json`, --max-turns bounded; two JSON parses (outer envelope, then the inner strict-schema payload)
      test_double.rs      — #[cfg(test)] FixedBackend, so `cargo test` never invokes a real `claude` process
  test-assets/
    monophonic-arpeggio.musicxml      — single voice, broken C major arpeggio, nothing else sounding: exercises expand_window resolving from melodic accumulation alone
    ambiguous-run.musicxml            — a run that never clears threshold even at the cap: exercises HitCap → Span → backend called exactly once (checked via a call-counting fake, not a real LLM call)
    two-voice-suspension.musicxml     — two simultaneous voices, one sustains into dissonance then resolves down by step: the one deterministic case role.rs can't get from a monophonic fixture
```

### Algorithm (`window.rs`), concretely

- **Vertical check**: notes overlapping the anchor's `[start, end)` across all
  tracks. `music_theory::detect()` on their pitch classes. Confidence ≥
  threshold (start at `26` — a complete triad scores `37`, a bare unconfirmed
  dyad tops out at `24`; tune against the fixtures) → resolved, done.
- **Expand**: grow a window outward from the anchor, nearest-neighbor-first
  (whichever of the two next candidate notes — one per side — is temporally
  closer to the anchor gets added next), re-running `detect()` on the
  accumulated pitch-class union after each addition. A **total** window budget
  (not per-side) of e.g. 16 notes / 2 measures, whichever comes first. Stop on:
  confidence clears threshold (**Resolved**); a genuine simultaneous silence in
  *every* active track in the direction being advanced (**StoppedAtSilence** for
  that side — freeze it, keep advancing the other side); budget exhausted
  (**HitCap**). Always keep the best-scoring candidate seen, even if the
  window that produced it wasn't the final one — this backfills the span's
  `best_deterministic_candidate` for the LLM prompt regardless of outcome.
- **Role classification**: chord tone (pitch class is one of the resolved
  chord's tones → tag with its interval) or non-chord tone, sub-classified
  from the anchor's same-track neighbor intervals (step vs. leap in/out) and
  beat strength (measure boundary from `MidiFile.measures`, subdivision from
  `SignatureTrack::time_signature_at`): passing / neighbor / appoggiatura /
  suspension / unclassified.
- **Local key**: `SignatureTrack::key_signature_at` if the score has an
  explicit signature there; else `Key::estimate` over a duration-weighted
  pitch-class histogram of the ~2 measures around the anchor (not the whole
  file — catches modal color a global estimate would average away).
- **Spans**: adjacent unresolved anchors whose expansion windows share at
  least one note id merge into one span — one LLM call per genuine ambiguous
  passage, not one per note.

### `schema.rs` — `song.analysis.json`

Flat, versioned (`version: 1`), not round-tripped so no need for the config
crate's `V1(...)` enum ceremony — one version tag is enough for an emit-only
artifact:

```rust
pub struct AnalysisFile { version: u32, source_file: String, key_hint: Option<KeyHint>, notes: Vec<NoteAnalysis>, spans: Vec<SpanAnalysis> }

pub struct NoteAnalysis {
    id: usize, track_id: usize, track_name: Option<String>,
    start_seconds: f64, duration_seconds: f64,
    pitch: u8, pitch_name: String, measure: usize, beat: f32, beat_strength: BeatStrength,
    resolution: Resolution,           // vertical | expanded | escalated | unresolved
    chord_symbol: Option<String>, chord_description: Option<String>, roman_numeral: Option<String>,
    role: NoteRole,                   // ChordTone{interval} | PassingTone | NeighborTone | Appoggiatura | Suspension | Unclassified
    local_key: Option<KeyHint>,
    window_note_ids: Vec<usize>, confidence: Option<i32>, span_id: Option<usize>,
}

pub struct SpanAnalysis {
    id: usize, note_ids: Vec<usize>, measure_range: (usize, usize),
    best_deterministic_candidate: Option<String>,
    outcome: SpanOutcome, // ResolvedByLlm{chord_symbol,confidence,reasoning} | RejectedLlmAnswer{reason} | LlmBackendError{message} | LeftUnresolved
}
```

### Backend validation — the one correctness fix from design review

Naively rejecting any LLM answer whose chord doesn't contain the anchor's
pitch class is **wrong**: an escalated span's anchor is very often itself the
non-chord-tone the run is built around, so a *correct* answer about the
surrounding harmony legitimately won't contain it. Validation instead:
accept if the anchor's pitch class is a chord tone of the returned chord, **or**
it forms a plausible non-chord-tone relationship against that chord (reuse
`role.rs`'s same step/leap-based classifier here, don't hand-roll a second
rule). Still hard-reject if `shape_symbol` isn't one of `music_theory::chord::SHAPES`'s
known symbols. Validation lives in `backend/mod.rs` as a free function, not
inside the `claude_cli` impl, so it's unit-testable against `FixedBackend`
canned responses without a real process call.

### `claude_cli.rs` invocation

`std::process::Command::new("claude").arg("-p").arg(prompt).arg("--output-format").arg("json")`,
with `--max-turns` bounded (~6–10) since the model may go read/grep the
extracted `score.xml` on its own. Ask for strict JSON in the prompt; parse the
outer `--output-format json` envelope, then parse *that* payload's text as the
`SpanResponse` schema — two JSON parses. Extract the first-`{`-to-last-`}`
span defensively in case the model wraps the JSON in prose despite instructions.

Prompt content per span: the span's notes as plain text (measure/beat/pitch/
duration — already-parsed data, not re-derived from XML), the deterministic
best-candidate and why it fell short, the resolved chord symbols immediately
before/after the span, and the path to the extracted `score.xml` so the
model's own Read/Grep can pull more context or check for recurrence elsewhere
in the piece entirely at its own discretion.

## Verification

**`cargo test -p mxl-analyze` (fully offline, no LLM calls):**
- `window.rs`: vertical resolution on a clean simultaneous triad; forced
  expansion on the monophonic arpeggio fixture (assert exact `window_note_ids`
  and stop reason); `StoppedAtSilence` on a fixture with a genuine rest;
  `HitCap` on the ambiguous-run fixture, with `best_chord` still populated.
- `role.rs`: synthetic neighbor-note triples covering each role, plus the
  two-voice-suspension fixture for the one case a monophonic fixture can't produce.
- `span.rs`: `merge_spans` on synthetic `(NoteId, window)` pairs — no `Score` needed.
- `backend`: `FixedBackend` canned responses covering happy path, unknown
  shape symbol (rejected), and a chord-tone-vs-non-chord-tone edge case that
  exercises the review fix above (not just a bare pitch-class check).
- `music-theory`: existing suite passes unchanged; add one test asserting
  `detect(...).unwrap().confidence` is higher for a complete triad than a bare dyad.
- Integration: parse `midi-file/test-assets/test.musicxml` through the full
  pipeline, assert on coarse stable properties (which chord/triad measure 1
  resolves to), not exact JSON byte-matching.

**Manual smoke test**, against a real file the user supplies locally (no such
file exists in this repo to commit):
1. `mxl-analyze that-file.mxl` → confirm `that-file.analysis.json` appears
   next to it.
2. The unaccompanied arpeggiated intro should resolve via `"expanded"`, not
   `"vertical"` (nothing else is sounding) — spot-check the named chords by ear.
3. `spans` should be small relative to total note count.
4. For any `ResolvedByLlm` span, sanity-check the symbol by ear; for any
   `RejectedLlmAnswer`, confirm the reason is legitimate, not a false rejection
   of a correct non-chord-tone answer.
5. Time the run once against a dense/chromatic piece — per-span subprocess
   latency is the real cost driver, worth knowing before using this on a big library.
