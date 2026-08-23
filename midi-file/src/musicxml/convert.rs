//! Conversion of a MusicXML document into an in memory midi file.
//!
//! MusicXML is a notation format, so a fair amount of interpretation is needed
//! to get something playable out of it:
//!
//! - `<backup>`/`<forward>`/`<chord>` are used to place every note on a shared
//!   per measure timeline.
//! - Repeats (`<barline><repeat>`) and voltas (`<ending>`) are unrolled, the same
//!   way a player would perform the score.
//! - Ties are merged into a single long note.
//! - Every staff of a part becomes its own track, so that the left and the right
//!   hand of a piano score can be assigned separately in the ui.

use std::collections::{HashMap, HashSet};

use midly::{
    Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
    num::{u4, u7, u15, u24, u28},
};

use crate::signature_track::{KeySignature, TimeSignature};

use super::ConvertedScore;

pub const PULSES_PER_QUARTER_NOTE: u16 = 960;

const PPQ: i64 = PULSES_PER_QUARTER_NOTE as i64;
const DEFAULT_VELOCITY: u8 = 80;
const DEFAULT_TEMPO: u32 = 500_000;
/// Grace notes carry no duration of their own, so we steal a bit of room right
/// before the note they decorate.
const GRACE_LEN: u64 = PPQ as u64 / 8;
const ACCIACCATURA_LEN: u64 = PPQ as u64 / 16;
/// Safety net for malformed repeat structures.
const MAX_UNROLLED_MEASURES: usize = 100_000;

type Node<'a, 'i> = roxmltree::Node<'a, 'i>;
type TieKey = (u32, String, u8);

pub fn convert(doc: &roxmltree::Document) -> Result<ConvertedScore, String> {
    let root = doc.root_element();

    if !root.has_tag_name("score-partwise") && !root.has_tag_name("score-timewise") {
        return Err(String::from("Not A MusicXML Score"));
    }

    let instruments = collect_instruments(root);
    let part_names = collect_part_names(root);
    let mut track_names: Vec<Option<String>> = Vec::new();

    // Every staff gets a track of its own, the tempo map is prepended later on.
    let mut track_count = 0;
    let mut parts = Vec::new();
    for (id, measures) in collect_parts(root) {
        let (instruments, default_instrument) = instruments
            .get(&id)
            .cloned()
            .unwrap_or_else(|| (HashMap::new(), Instrument::default()));

        let staves = count_staves(&measures);
        let states = scan_states(&measures);

        // Tablature staves hold the very same notes as the staff they belong
        // to, playing both would double every note.
        let mut tabs = tab_staves(&measures);
        if (1..=staves).all(|staff| tabs.contains(&staff)) {
            tabs.clear();
        }

        // A part with several staves shows up as several tracks, so the name
        // has to say which staff it is, otherwise they are impossible to tell
        // apart in the track picker.
        let clefs = staff_clefs(&measures);
        let part_name = part_names.get(&id).cloned();

        let mut staff_tracks = HashMap::new();
        for staff in (1..=staves).filter(|staff| !tabs.contains(staff)) {
            staff_tracks.insert(staff, track_count);
            track_count += 1;

            let name = match (&part_name, staves > 1) {
                (Some(part), true) => {
                    let clef = clefs.get(&staff).copied().unwrap_or(if staff == 1 {
                        "treble"
                    } else {
                        "bass"
                    });
                    Some(format!("{part} · {clef}"))
                }
                (Some(part), false) => Some(part.clone()),
                (None, _) => None,
            };

            track_names.push(name);
        }

        parts.push(Part {
            measures,
            states,
            tabs,
            staff_tracks,
            instruments,
            default_instrument,
        });
    }

    if parts.is_empty() {
        return Err(String::from("MusicXML File Has No Parts"));
    }

    let measure_count = parts.iter().map(|p| p.measures.len()).max().unwrap_or(0);
    if measure_count == 0 {
        return Err(String::from("MusicXML File Has No Measures"));
    }

    // Measures are the same length in every part, but a part can be missing
    // some of them, so we take the longest one we find.
    let mut lengths = vec![0u64; measure_count];
    for part in parts.iter() {
        for (id, state) in part.states.iter().enumerate() {
            lengths[id] = lengths[id].max(state.len);
        }
    }

    // Repeats are usually written in every part, but directions like a `dal
    // segno` tend to live in the first one only, so we merge them all.
    let mut repeats = vec![RepeatMeta::default(); measure_count];
    for part in parts.iter() {
        for (id, measure) in part.measures.iter().enumerate() {
            repeats[id].merge(repeat_meta(*measure));
        }
    }

    let order = unroll_repeats(&repeats, measure_count);

    let mut starts = Vec::with_capacity(order.len() + 1);
    let mut pulses = 0u64;
    for id in order.iter() {
        starts.push(pulses);
        pulses += lengths[*id];
    }
    starts.push(pulses);

    let mut renderer = Renderer {
        tracks: (0..track_count).map(|_| TrackBuf::default()).collect(),
        tempo: HashMap::new(),
        time: HashMap::new(),
        key: HashMap::new(),
    };

    for part in parts.iter() {
        renderer.render_part(part, &order, &starts);
    }

    Ok(ConvertedScore {
        smf: renderer.build_smf(),
        measures: starts,
        track_names,
    })
}

#[derive(Debug, Default, Clone, Copy)]
struct Instrument {
    channel: u8,
    program: u8,
    unpitched: Option<u8>,
}

struct Part<'a, 'i> {
    measures: Vec<Node<'a, 'i>>,
    states: Vec<MeasureState>,
    /// Staves that are tablature, and therefore silent.
    tabs: HashSet<u32>,
    /// Staff number -> track index.
    staff_tracks: HashMap<u32, usize>,
    instruments: HashMap<String, Instrument>,
    default_instrument: Instrument,
}

impl Part<'_, '_> {
    fn track_of(&self, staff: u32) -> Option<usize> {
        if self.tabs.contains(&staff) {
            return None;
        }

        self.staff_tracks
            .get(&staff)
            .or_else(|| self.staff_tracks.values().min())
            .copied()
    }
}

/// State of a part at the very start of a measure.
#[derive(Debug, Clone, Copy)]
struct MeasureState {
    divisions: u32,
    transpose: i32,
    /// Length of the measure in pulses.
    len: u64,
    time: Option<TimeSignature>,
    key: Option<KeySignature>,
}

struct NoteRec {
    start: u64,
    len: u64,
    key: u8,
    channel: u8,
    velocity: u8,
}

#[derive(Default)]
struct TrackBuf {
    notes: Vec<NoteRec>,
    /// (channel, program)
    programs: Vec<(u8, u8)>,
}

struct Renderer {
    tracks: Vec<TrackBuf>,
    /// Pulses -> microseconds per quarter note.
    tempo: HashMap<u64, u32>,
    /// Meter and key changes, keyed by the pulse they happen on.
    time: HashMap<u64, TimeSignature>,
    key: HashMap<u64, KeySignature>,
}

impl Renderer {
    fn render_part(&mut self, part: &Part, order: &[usize], starts: &[u64]) {
        let mut programs: Vec<(u8, u8)> = part
            .instruments
            .values()
            .map(|i| (i.channel, i.program))
            .collect();
        if programs.is_empty() {
            let i = part.default_instrument;
            programs.push((i.channel, i.program));
        }
        programs.sort_unstable();
        programs.dedup();
        for track in part.staff_tracks.values() {
            self.tracks[*track].programs = programs.clone();
        }

        let mut velocity = DEFAULT_VELOCITY;
        let mut ties: HashMap<TieKey, (usize, usize)> = HashMap::new();

        for (seq, id) in order.iter().enumerate() {
            let Some(measure) = part.measures.get(*id) else {
                continue;
            };
            let state = part.states[*id];

            if let Some(time) = state.time {
                self.time.entry(starts[seq]).or_insert(time);
            }
            if let Some(key) = state.key {
                self.key.entry(starts[seq]).or_insert(key);
            }

            self.render_measure(part, *measure, state, starts[seq], &mut velocity, &mut ties);
        }
    }

    fn render_measure(
        &mut self,
        part: &Part,
        measure: Node,
        state: MeasureState,
        base: u64,
        velocity: &mut u8,
        ties: &mut HashMap<TieKey, (usize, usize)>,
    ) {
        let mut divisions = state.divisions;
        let mut transpose = state.transpose;
        let mut cursor: i64 = 0;
        let mut last_start: i64 = 0;

        for node in measure.children() {
            match node.tag_name().name() {
                "attributes" => {
                    if let Some(d) = num::<u32>(node, "divisions").filter(|d| *d > 0) {
                        divisions = d;
                    }
                    if let Some(t) = child(node, "transpose") {
                        transpose = transpose_of(t);
                    }
                }
                "backup" => {
                    cursor = (cursor - duration_of(node, divisions)).max(0);
                }
                "forward" => {
                    cursor += duration_of(node, divisions);
                }
                "sound" => {
                    self.on_sound(node, (base as i64 + cursor).max(0) as u64, velocity);
                }
                "direction" => {
                    let offset = child(node, "offset")
                        .and_then(|o| o.text())
                        .and_then(|o| o.trim().parse::<f64>().ok())
                        .map(|o| pulses(o, divisions))
                        .unwrap_or(0);
                    let at = (base as i64 + cursor + offset).max(0) as u64;

                    for kind in node.children().filter(|n| n.has_tag_name("direction-type")) {
                        for node in kind.children().filter(|n| n.is_element()) {
                            match node.tag_name().name() {
                                "dynamics" => {
                                    if let Some(vel) = dynamics_velocity(node) {
                                        *velocity = vel;
                                    }
                                }
                                "metronome" => {
                                    if let Some(tempo) = metronome_tempo(node) {
                                        self.tempo.insert(at, tempo);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    if let Some(sound) = child(node, "sound") {
                        self.on_sound(sound, at, velocity);
                    }
                }
                "note" => {
                    let grace = child(node, "grace");
                    let is_grace = grace.is_some();
                    let is_chord = child(node, "chord").is_some();
                    let is_rest = child(node, "rest").is_some();
                    // Cue notes are visual only, but they do take up time.
                    let is_cue = child(node, "cue").is_some();

                    let len = duration_of(node, divisions);
                    let start = if is_chord { last_start } else { cursor };
                    let staff = num::<u32>(node, "staff").unwrap_or(1);

                    if !is_rest
                        && !is_cue
                        && let Some((key, instrument)) = note_pitch(node, part, transpose)
                        && let Some(track) = part.track_of(staff)
                    {
                        let voice = text(node, "voice").unwrap_or("1");

                        let mut len = len.max(0) as u64;
                        let mut start = (base as i64 + start).max(0) as u64;

                        if is_grace {
                            len = if grace.and_then(|g| g.attribute("slash")) == Some("yes") {
                                ACCIACCATURA_LEN
                            } else {
                                GRACE_LEN
                            };
                            start = start.saturating_sub(len);
                        }

                        let (tie_start, tie_stop) = tie_flags(node);
                        let tie_key = (staff, voice.to_string(), key);

                        // A note tied to a previous one just makes it longer.
                        let held = if tie_stop {
                            ties.remove(&tie_key)
                        } else {
                            None
                        };
                        let slot = match held {
                            Some((track, id)) => {
                                self.tracks[track].notes[id].len += len;
                                (track, id)
                            }
                            None => {
                                let velocity = node
                                    .attribute("dynamics")
                                    .and_then(|d| d.parse::<f64>().ok())
                                    .map(percent_to_velocity)
                                    .unwrap_or(*velocity);

                                self.tracks[track].notes.push(NoteRec {
                                    start,
                                    len: len.max(1),
                                    key,
                                    channel: instrument.channel,
                                    velocity,
                                });

                                (track, self.tracks[track].notes.len() - 1)
                            }
                        };

                        if tie_start {
                            ties.insert(tie_key, slot);
                        }
                    }

                    if !is_grace {
                        if is_chord {
                            cursor = cursor.max(last_start + len);
                        } else {
                            last_start = cursor;
                            cursor += len;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn on_sound(&mut self, node: Node, at: u64, velocity: &mut u8) {
        if let Some(tempo) = node.attribute("tempo").and_then(|t| t.parse::<f64>().ok())
            && tempo > 0.0
        {
            self.tempo.insert(at, (60_000_000.0 / tempo).round() as u32);
        }

        if let Some(dynamics) = node
            .attribute("dynamics")
            .and_then(|d| d.parse::<f64>().ok())
        {
            *velocity = percent_to_velocity(dynamics);
        }
    }

    fn build_smf(self) -> Smf<'static> {
        let mut smf = Smf::new(Header {
            format: Format::Parallel,
            timing: Timing::Metrical(u15::new(PULSES_PER_QUARTER_NOTE)),
        });

        let mut tempo: Vec<(u64, u32)> = self.tempo.into_iter().collect();
        tempo.sort_unstable();
        if tempo.first().map(|(at, _)| *at) != Some(0) {
            tempo.insert(0, (0, DEFAULT_TEMPO));
        }

        let mut tempo_track: Vec<(u64, u8, TrackEventKind<'static>)> = tempo
            .into_iter()
            .map(|(at, tempo)| {
                (
                    at,
                    0,
                    TrackEventKind::Meta(MetaMessage::Tempo(u24::new(tempo))),
                )
            })
            .collect();

        for (at, time) in self.time.iter() {
            // Midi stores the denominator as a power of two.
            let denominator = time.denominator.max(1).ilog2() as u8;

            tempo_track.push((
                *at,
                0,
                TrackEventKind::Meta(MetaMessage::TimeSignature(
                    time.numerator,
                    denominator,
                    24,
                    8,
                )),
            ));
        }

        for (at, key) in self.key.iter() {
            tempo_track.push((
                *at,
                0,
                TrackEventKind::Meta(MetaMessage::KeySignature(key.fifths, key.minor)),
            ));
        }

        smf.tracks.push(to_track(&mut tempo_track));

        for track in self.tracks.into_iter() {
            let mut events: Vec<(u64, u8, TrackEventKind<'static>)> = Vec::new();

            for (channel, program) in track.programs.iter() {
                events.push((
                    0,
                    0,
                    TrackEventKind::Midi {
                        channel: u4::new(*channel),
                        message: MidiMessage::ProgramChange {
                            program: u7::new(*program),
                        },
                    },
                ));
            }

            for note in track.notes.iter() {
                let channel = u4::new(note.channel);
                let key = u7::new(note.key);

                events.push((
                    note.start,
                    2,
                    TrackEventKind::Midi {
                        channel,
                        message: MidiMessage::NoteOn {
                            key,
                            vel: u7::new(note.velocity),
                        },
                    },
                ));
                events.push((
                    note.start + note.len,
                    1,
                    TrackEventKind::Midi {
                        channel,
                        message: MidiMessage::NoteOff {
                            key,
                            vel: u7::new(0),
                        },
                    },
                ));
            }

            smf.tracks.push(to_track(&mut events));
        }

        smf
    }
}

/// Sorts absolute events and turns them into a delta timed midi track.
fn to_track(events: &mut Vec<(u64, u8, TrackEventKind<'static>)>) -> Vec<TrackEvent<'static>> {
    // Note offs have to land before note ons sharing a timestamp, otherwise a
    // repeated note would be cut short by its own predecessor.
    events.sort_by_key(|(at, priority, _)| (*at, *priority));

    let mut last = 0u64;
    let mut track: Vec<TrackEvent> = events
        .iter()
        .map(|(at, _, kind)| {
            let delta = at.saturating_sub(last);
            last = *at;
            TrackEvent {
                delta: u28::new(delta as u32),
                kind: *kind,
            }
        })
        .collect();

    track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });

    track
}

fn collect_parts<'a, 'i>(root: Node<'a, 'i>) -> Vec<(String, Vec<Node<'a, 'i>>)> {
    let mut parts: Vec<(String, Vec<Node<'a, 'i>>)> = Vec::new();
    let mut ids: HashMap<String, usize> = HashMap::new();

    let mut push = |id: &str, measure: Node<'a, 'i>| {
        let id = ids.entry(id.to_string()).or_insert_with(|| {
            parts.push((id.to_string(), Vec::new()));
            parts.len() - 1
        });
        parts[*id].1.push(measure);
    };

    if root.has_tag_name("score-timewise") {
        for measure in root.children().filter(|n| n.has_tag_name("measure")) {
            for part in measure.children().filter(|n| n.has_tag_name("part")) {
                push(part.attribute("id").unwrap_or_default(), part);
            }
        }
    } else {
        for part in root.children().filter(|n| n.has_tag_name("part")) {
            let id = part.attribute("id").unwrap_or_default();
            for measure in part.children().filter(|n| n.has_tag_name("measure")) {
                push(id, measure);
            }
        }
    }

    parts
}

/// Name of every part, as declared in `<part-list>`.
fn collect_part_names(root: Node) -> HashMap<String, String> {
    let Some(list) = child(root, "part-list") else {
        return HashMap::new();
    };

    list.children()
        .filter(|n| n.has_tag_name("score-part"))
        .filter_map(|part| {
            let id = part.attribute("id")?;
            let name = text(part, "part-name")
                .filter(|name| !name.is_empty())
                .or_else(|| text(part, "part-abbreviation"))?;

            Some((id.to_string(), name.to_string()))
        })
        .collect()
}

/// What clef each staff of a part is written in, which is how a reader tells
/// the right hand from the left.
fn staff_clefs(measures: &[Node]) -> HashMap<u32, &'static str> {
    let mut clefs = HashMap::new();

    for clef in measures
        .iter()
        .flat_map(|m| m.children().filter(|n| n.has_tag_name("attributes")))
        .flat_map(|a| a.children().filter(|n| n.has_tag_name("clef")))
    {
        let staff = clef
            .attribute("number")
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(1);

        let label = match text(clef, "sign").unwrap_or("G") {
            "G" => "treble",
            "F" => "bass",
            "C" => "alto",
            "percussion" => "percussion",
            "TAB" => "tab",
            _ => continue,
        };

        clefs.entry(staff).or_insert(label);
    }

    clefs
}

/// Instruments of every part, as declared in `<part-list>`.
#[allow(clippy::type_complexity)]
fn collect_instruments(root: Node) -> HashMap<String, (HashMap<String, Instrument>, Instrument)> {
    let mut out = HashMap::new();

    let Some(list) = child(root, "part-list") else {
        return out;
    };

    for part in list.children().filter(|n| n.has_tag_name("score-part")) {
        let Some(part_id) = part.attribute("id") else {
            continue;
        };

        let mut instruments = HashMap::new();
        let mut default_instrument = None;

        for node in part
            .children()
            .filter(|n| n.has_tag_name("midi-instrument"))
        {
            // MusicXML counts channels and programs from one.
            let instrument = Instrument {
                channel: num::<u8>(node, "midi-channel")
                    .map(|c| c.saturating_sub(1))
                    .unwrap_or(0)
                    .min(15),
                program: num::<u8>(node, "midi-program")
                    .map(|p| p.saturating_sub(1))
                    .unwrap_or(0)
                    .min(127),
                unpitched: num::<u8>(node, "midi-unpitched").map(|u| u.saturating_sub(1).min(127)),
            };

            default_instrument.get_or_insert(instrument);

            if let Some(id) = node.attribute("id") {
                instruments.insert(id.to_string(), instrument);
            }
        }

        out.insert(
            part_id.to_string(),
            (instruments, default_instrument.unwrap_or_default()),
        );
    }

    out
}

/// Staves that only carry tablature, going by their clef.
fn tab_staves(measures: &[Node]) -> HashSet<u32> {
    measures
        .iter()
        .flat_map(|m| m.children().filter(|n| n.has_tag_name("attributes")))
        .flat_map(|a| a.children().filter(|n| n.has_tag_name("clef")))
        .filter(|clef| text(*clef, "sign").map(str::trim) == Some("TAB"))
        .map(|clef| {
            clef.attribute("number")
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(1)
        })
        .collect()
}

fn count_staves(measures: &[Node]) -> u32 {
    let declared = measures
        .iter()
        .flat_map(|m| m.children().filter(|n| n.has_tag_name("attributes")))
        .filter_map(|a| num::<u32>(a, "staves"))
        .max()
        .unwrap_or(1);

    let used = measures
        .iter()
        .flat_map(|m| m.children().filter(|n| n.has_tag_name("note")))
        .filter_map(|n| num::<u32>(n, "staff"))
        .max()
        .unwrap_or(1);

    declared.max(used).clamp(1, 16)
}

/// Walks every measure once to resolve the state it starts with, and how long it is.
fn scan_states(measures: &[Node]) -> Vec<MeasureState> {
    let mut divisions = 1;
    let mut transpose = 0;
    let mut out = Vec::with_capacity(measures.len());

    for measure in measures.iter() {
        let state_divisions = divisions;
        let state_transpose = transpose;
        let attributes = || measure.children().filter(|n| n.has_tag_name("attributes"));

        let time = attributes()
            .filter_map(|a| child(a, "time"))
            .next()
            .and_then(|time| {
                Some(TimeSignature {
                    numerator: num::<u8>(time, "beats")?.max(1),
                    denominator: num::<u8>(time, "beat-type")?.max(1),
                })
            });

        let key = attributes()
            .filter_map(|a| child(a, "key"))
            .next()
            .map(|key| KeySignature {
                fifths: num::<i8>(key, "fifths").unwrap_or(0).clamp(-7, 7),
                minor: matches!(text(key, "mode"), Some("minor")),
            });

        let mut cursor: i64 = 0;
        let mut len: i64 = 0;

        for node in measure.children() {
            match node.tag_name().name() {
                "attributes" => {
                    if let Some(d) = num::<u32>(node, "divisions").filter(|d| *d > 0) {
                        divisions = d;
                    }
                    if let Some(t) = child(node, "transpose") {
                        transpose = transpose_of(t);
                    }
                }
                "note" => {
                    // Chord notes share the position of the note before them,
                    // grace notes take no time at all.
                    if child(node, "chord").is_none() && child(node, "grace").is_none() {
                        cursor += duration_of(node, divisions);
                    }
                }
                "backup" => {
                    cursor = (cursor - duration_of(node, divisions)).max(0);
                }
                "forward" => {
                    cursor += duration_of(node, divisions);
                }
                _ => {}
            }

            len = len.max(cursor);
        }

        out.push(MeasureState {
            divisions: state_divisions,
            transpose: state_transpose,
            len: len.max(0) as u64,
            time,
            key,
        });
    }

    out
}

#[derive(Debug, Default, Clone)]
struct RepeatMeta {
    /// Start of a repeated section.
    forward: bool,
    /// End of a repeated section, with the total amount of times it is played.
    backward: Option<u32>,
    /// Volta, with the passes it is played on.
    ending_start: Option<Vec<u32>>,
    ending_stop: bool,
    /// Target of a `dal segno`.
    segno: bool,
    /// Target of a `to coda`.
    coda: bool,
    /// End of the piece, once we came back around.
    fine: bool,
    /// Jump back to the start, or to the segno.
    dacapo: bool,
    dalsegno: bool,
    tocoda: bool,
}

impl RepeatMeta {
    fn merge(&mut self, other: Self) {
        self.forward |= other.forward;
        self.backward = self.backward.max(other.backward);
        self.ending_start = self.ending_start.take().or(other.ending_start);
        self.ending_stop |= other.ending_stop;
        self.segno |= other.segno;
        self.coda |= other.coda;
        self.fine |= other.fine;
        self.dacapo |= other.dacapo;
        self.dalsegno |= other.dalsegno;
        self.tocoda |= other.tocoda;
    }

    fn is_empty(&self) -> bool {
        !self.forward
            && self.backward.is_none()
            && self.ending_start.is_none()
            && !self.ending_stop
            && !self.segno
            && !self.coda
            && !self.fine
            && !self.dacapo
            && !self.dalsegno
            && !self.tocoda
    }
}

fn repeat_meta(measure: Node) -> RepeatMeta {
    let mut meta = RepeatMeta::default();

    // Jump directives live in `<sound>`, either on the measure itself or inside
    // a direction.
    for sound in measure.descendants().filter(|n| n.has_tag_name("sound")) {
        meta.segno |= sound.attribute("segno").is_some();
        meta.coda |= sound.attribute("coda").is_some();
        meta.fine |= sound.attribute("fine").is_some();
        meta.dacapo |= sound.attribute("dacapo").is_some();
        meta.dalsegno |= sound.attribute("dalsegno").is_some();
        meta.tocoda |= sound.attribute("tocoda").is_some();
    }

    for barline in measure.children().filter(|n| n.has_tag_name("barline")) {
        if let Some(repeat) = child(barline, "repeat") {
            match repeat.attribute("direction") {
                Some("forward") => meta.forward = true,
                Some("backward") => {
                    let times = repeat
                        .attribute("times")
                        .and_then(|t| t.parse::<u32>().ok())
                        .unwrap_or(2);
                    meta.backward = Some(times.max(2));
                }
                _ => {}
            }
        }

        if let Some(ending) = child(barline, "ending") {
            match ending.attribute("type") {
                Some("start") => {
                    let numbers: Vec<u32> = ending
                        .attribute("number")
                        .unwrap_or_default()
                        .split(',')
                        .filter_map(|n| {
                            n.trim()
                                .trim_matches(|c: char| !c.is_ascii_digit())
                                .parse()
                                .ok()
                        })
                        .collect();

                    if !numbers.is_empty() {
                        meta.ending_start = Some(numbers);
                    }
                }
                Some("stop") | Some("discontinue") => meta.ending_stop = true,
                _ => {}
            }
        }
    }

    meta
}

/// Turns the written score into the order a player would actually perform it in.
fn unroll_repeats(repeats: &[RepeatMeta], measure_count: usize) -> Vec<usize> {
    if repeats.iter().all(RepeatMeta::is_empty) {
        return (0..measure_count).collect();
    }

    let count = repeats.len();
    let limit = (count * 8 + 1000).min(MAX_UNROLLED_MEASURES);

    let segno = repeats.iter().position(|m| m.segno);

    let mut out = Vec::new();
    let mut taken: HashMap<usize, u32> = HashMap::new();
    let mut passes: HashMap<usize, u32> = HashMap::new();
    let mut jumped: HashSet<usize> = HashSet::new();
    // Written repeats are not played again on the way back from a jump.
    let mut coming_back = false;
    let mut section_start = 0usize;
    let mut pos = 0usize;

    while pos < count && out.len() < limit {
        let meta = &repeats[pos];

        if meta.forward {
            section_start = pos;
            passes.entry(pos).or_insert(1);
        }

        // Skip the voltas that don't belong to the pass we are on.
        if let Some(numbers) = &meta.ending_start {
            let pass = passes.get(&section_start).copied().unwrap_or(1);

            if !numbers.contains(&pass) {
                let mut skip = pos;
                while skip < count && !repeats[skip].ending_stop {
                    skip += 1;
                }
                pos = skip + 1;
                continue;
            }
        }

        out.push(pos);

        if coming_back {
            if meta.fine {
                break;
            }

            // The coda sign sits after the `to coda` we are jumping from,
            // unless a writer put both of them on the same measure.
            let coda = repeats
                .iter()
                .skip(pos + 1)
                .position(|m| m.coda)
                .map(|id| id + pos + 1)
                .or_else(|| repeats.iter().rposition(|m| m.coda));

            if meta.tocoda
                && let Some(coda) = coda.filter(|coda| *coda != pos)
            {
                pos = coda;
                continue;
            }
        }

        if (meta.dacapo || meta.dalsegno) && jumped.insert(pos) {
            let target = if meta.dalsegno { segno.unwrap_or(0) } else { 0 };

            if target != pos {
                coming_back = true;
                pos = target;
                continue;
            }
        }

        if let Some(times) = meta.backward.filter(|_| !coming_back) {
            let taken = taken.entry(pos).or_insert(0);

            if *taken < times - 1 {
                *taken += 1;
                *passes.entry(section_start).or_insert(1) += 1;
                pos = section_start;
                continue;
            }
        }

        pos += 1;
    }

    if out.is_empty() {
        return (0..measure_count).collect();
    }

    // Parts can be longer than the one we read the repeats from.
    for id in count..measure_count {
        out.push(id);
    }

    out
}

fn note_pitch(node: Node, part: &Part, transpose: i32) -> Option<(u8, Instrument)> {
    let instrument = child(node, "instrument")
        .and_then(|i| i.attribute("id"))
        .and_then(|id| part.instruments.get(id).copied())
        .unwrap_or(part.default_instrument);

    if let Some(pitch) = child(node, "pitch") {
        let step = step_semitones(text(pitch, "step")?)?;
        let octave = num::<i32>(pitch, "octave")?;
        let alter = num::<f32>(pitch, "alter").unwrap_or(0.0).round() as i32;

        let key = (octave + 1) * 12 + step + alter + transpose;

        Some((key.clamp(0, 127) as u8, instrument))
    } else if let Some(unpitched) = child(node, "unpitched") {
        // Percussion, the written position is only a display hint, the sound
        // comes from the instrument definition.
        let key = instrument
            .unpitched
            .map(i32::from)
            .or_else(|| {
                let step = step_semitones(text(unpitched, "display-step")?)?;
                let octave = num::<i32>(unpitched, "display-octave")?;
                Some((octave + 1) * 12 + step)
            })
            .unwrap_or(38);

        Some((key.clamp(0, 127) as u8, instrument))
    } else {
        None
    }
}

fn tie_flags(node: Node) -> (bool, bool) {
    let mut start = false;
    let mut stop = false;

    let ties = node
        .children()
        .filter(|n| n.has_tag_name("tie"))
        .chain(
            node.children()
                .filter(|n| n.has_tag_name("notations"))
                .flat_map(|n| n.children().filter(|n| n.has_tag_name("tied"))),
        )
        .filter_map(|n| n.attribute("type"));

    for tie in ties {
        match tie {
            "start" => start = true,
            "stop" => stop = true,
            _ => {}
        }
    }

    (start, stop)
}

fn dynamics_velocity(node: Node) -> Option<u8> {
    let mark = node.children().find(|n| n.is_element())?;

    let velocity = match mark.tag_name().name() {
        "pppp" | "ppppp" | "pppppp" => 10,
        "ppp" => 20,
        "pp" => 31,
        "p" => 49,
        "mp" => 64,
        "mf" => 80,
        "f" => 96,
        "ff" => 112,
        "fff" => 126,
        "ffff" | "fffff" | "ffffff" => 127,
        "sf" | "sfz" | "sffz" | "rf" | "rfz" => 112,
        "fp" | "sfp" => 96,
        "fz" => 112,
        _ => return None,
    };

    Some(velocity)
}

fn metronome_tempo(node: Node) -> Option<u32> {
    let per_minute = text(node, "per-minute")?.parse::<f64>().ok()?;
    if per_minute <= 0.0 {
        return None;
    }

    let unit = match text(node, "beat-unit").unwrap_or("quarter") {
        "maxima" => 32.0,
        "long" => 16.0,
        "breve" => 8.0,
        "whole" => 4.0,
        "half" => 2.0,
        "quarter" => 1.0,
        "eighth" => 0.5,
        "16th" => 0.25,
        "32nd" => 0.125,
        "64th" => 0.0625,
        "128th" => 0.03125,
        _ => 1.0,
    };

    // Every `beat-unit-dot` adds half of the value again.
    let dots = node
        .children()
        .filter(|n| n.has_tag_name("beat-unit-dot"))
        .count();
    let unit = unit * (2.0 - 0.5f64.powi(dots as i32));

    let quarters_per_minute = per_minute * unit;
    if quarters_per_minute <= 0.0 {
        return None;
    }

    Some((60_000_000.0 / quarters_per_minute).round() as u32)
}

/// MusicXML expresses loudness as a percentage of velocity 90.
fn percent_to_velocity(percent: f64) -> u8 {
    ((percent * 0.9).round() as i64).clamp(1, 127) as u8
}

fn transpose_of(node: Node) -> i32 {
    let chromatic = num::<i32>(node, "chromatic").unwrap_or(0);
    let octaves = num::<i32>(node, "octave-change").unwrap_or(0);

    chromatic + octaves * 12
}

fn step_semitones(step: &str) -> Option<i32> {
    Some(match step.trim() {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        "B" => 11,
        _ => return None,
    })
}

fn duration_of(node: Node, divisions: u32) -> i64 {
    pulses(num::<f64>(node, "duration").unwrap_or(0.0), divisions)
}

fn pulses(duration: f64, divisions: u32) -> i64 {
    if divisions == 0 {
        return 0;
    }

    (duration * PPQ as f64 / divisions as f64).round() as i64
}

fn child<'a, 'i>(node: Node<'a, 'i>, tag: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|n| n.has_tag_name(tag))
}

fn text<'a>(node: Node<'a, '_>, tag: &str) -> Option<&'a str> {
    child(node, tag)?.text().map(str::trim)
}

fn num<T: std::str::FromStr>(node: Node, tag: &str) -> Option<T> {
    text(node, tag)?.parse().ok()
}
