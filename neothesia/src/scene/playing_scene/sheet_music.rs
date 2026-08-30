//! A one-bar-at-a-time staff notation view, built from the notation data
//! MusicXML files keep and plain midi files don't - see
//! [`midi_file::NotationInfo`].
//!
//! Unlike the waterfall, this doesn't scroll: the whole current measure is
//! laid out at once, a beat column lights up as the song reaches it, and the
//! view jumps to the next measure only once the current one is done - meant
//! to be readable at a glance, not tracked continuously.

use midi_file::{
    BeamState, Clef, MidiFile, MidiNote, MidiTrack, NotationInfo, NoteValue,
    tempo_track::TempoTrack,
};
use nuon::Ui;
use std::time::Duration;

use crate::{context::Context, icons::sheet, song::Song};

const STAFF_LINE_GAP: f32 = 9.0;
/// Vertical distance between adjacent diatonic steps - half a line gap,
/// since a line and the space above it are one step apart.
const STEP: f32 = STAFF_LINE_GAP / 2.0;
const STAFF_GAP: f32 = 48.0;
const PAD: f32 = 18.0;
const CLEF_W: f32 = 34.0;
const HEADER_H: f32 = 22.0;

const LINE_COLOR: [u8; 4] = [150, 148, 160, 200];
const BEAT_LINE_COLOR: [u8; 4] = [90, 88, 100, 120];
const DIM_NOTE_COLOR: [u8; 4] = [150, 148, 160, 180];
const BEAT_HIGHLIGHT: [u8; 4] = [255, 196, 84, 40];
const HEADER_COLOR: [u8; 3] = [180, 178, 190];

/// Whether `song` has any track worth drawing a staff for.
pub fn is_available(song: &Song) -> bool {
    song.file.tracks.iter().any(|track| track.clef.is_some())
}

pub fn height(song: &Song) -> f32 {
    let staves = song
        .file
        .tracks
        .iter()
        .filter(|t| t.clef.is_some())
        .count()
        .max(1) as f32;

    HEADER_H + PAD * 2.0 + staves * (STAFF_LINE_GAP * 4.0) + (staves - 1.0).max(0.0) * STAFF_GAP
}

/// Index of the measure `at` falls in, and its [start, end) bounds.
fn current_measure(file: &MidiFile, at: Duration) -> (usize, Duration, Duration) {
    let measures = &file.measures;
    let idx = match measures.binary_search(&at) {
        Ok(idx) => idx,
        Err(idx) => idx.saturating_sub(1),
    };

    let start = measures.get(idx).copied().unwrap_or(Duration::ZERO);
    let end = measures
        .get(idx + 1)
        .copied()
        .unwrap_or(start + Duration::from_secs(4));

    (idx, start, end)
}

pub fn build(ui: &mut Ui, ctx: &Context, song: &Song, song_time: Duration, width: f32, y: f32) {
    let staves: Vec<&MidiTrack> = song
        .file
        .tracks
        .iter()
        .filter(|t| t.clef.is_some())
        .collect();

    if staves.is_empty() {
        return;
    }

    let panel_h = height(song);
    let (measure_idx, measure_start, measure_end) = current_measure(&song.file, song_time);
    let bar_len = (measure_end - measure_start).as_secs_f32().max(0.001);
    let progress = (song_time.saturating_sub(measure_start)).as_secs_f32() / bar_len;

    let beats = song
        .file
        .signature_track
        .time_signature_at(&measure_start)
        .numerator
        .max(1);
    let current_beat = ((progress * beats as f32) as u32).min(beats as u32 - 1);

    let content_x0 = PAD + CLEF_W + 14.0;
    let content_x1 = width - PAD;

    nuon::translate().x(0.0).y(y).build(ui, |ui| {
        nuon::quad()
            .size(width, panel_h)
            .color(nuon::Color::new_u8(24, 23, 28, 0.88))
            .border_radius([12.0; 4])
            .build(ui);

        nuon::label()
            .text(format!("Measure {}", measure_idx + 1))
            .font_size(13.0)
            .bold(true)
            .color(HEADER_COLOR)
            .x(PAD)
            .y(4.0)
            .width(200.0)
            .height(HEADER_H)
            .build(ui);

        // The beat currently playing, as one lit column behind every staff.
        let beat_w = (content_x1 - content_x0) / beats as f32;
        nuon::quad()
            .x(content_x0 + current_beat as f32 * beat_w)
            .y(HEADER_H)
            .width(beat_w)
            .height(panel_h - HEADER_H)
            .color(BEAT_HIGHLIGHT)
            .build(ui);

        let key = song.file.signature_track.key_signature_at(&measure_start);
        let note_color = ctx
            .config
            .color_schema()
            .first()
            .map(|c| [c.base.0, c.base.1, c.base.2])
            .unwrap_or([235, 235, 240]);

        let mut staff_y = HEADER_H + PAD;
        for track in staves {
            let clef = track.clef.unwrap();
            staff(
                ui,
                track,
                clef,
                note_color,
                key.map(|k| k.fifths).unwrap_or(0),
                &song.file.tempo_track,
                song_time,
                measure_start,
                measure_end,
                content_x0,
                content_x1,
                beats,
                width,
                staff_y,
            );
            staff_y += STAFF_LINE_GAP * 4.0 + STAFF_GAP;
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn staff(
    ui: &mut Ui,
    track: &MidiTrack,
    clef: Clef,
    note_color: [u8; 3],
    fifths: i8,
    tempo_track: &TempoTrack,
    song_time: Duration,
    measure_start: Duration,
    measure_end: Duration,
    content_x0: f32,
    content_x1: f32,
    beats: u8,
    width: f32,
    top_y: f32,
) {
    let bar_len = (measure_end - measure_start).as_secs_f32().max(0.001);
    let ppq = tempo_track.pulses_per_quarter_note() as f32;

    nuon::translate().y(top_y).build(ui, |ui| {
        // The five lines, top to bottom.
        for line in 0..5 {
            nuon::quad()
                .y(line as f32 * STAFF_LINE_GAP)
                .width(width - PAD)
                .x(PAD)
                .height(1.0)
                .color(LINE_COLOR)
                .build(ui);
        }

        // Faint dividers between beats, so the highlighted one reads as a
        // position within the bar rather than a floating box.
        for beat in 1..beats {
            let x = content_x0 + (content_x1 - content_x0) * (beat as f32 / beats as f32);
            nuon::quad()
                .x(x)
                .y(-4.0)
                .width(1.0)
                .height(STAFF_LINE_GAP * 4.0 + 8.0)
                .color(BEAT_LINE_COLOR)
                .build(ui);
        }

        let clef_glyph = match clef {
            Clef::Treble => sheet::g_clef(),
            Clef::Bass => sheet::f_clef(),
            Clef::Alto => sheet::c_clef(),
            Clef::Percussion => sheet::g_clef(),
        };

        // Leland's glyphs are anchored so that this y sits the clef right on
        // the staff; nudged per clef since the G and F clefs don't share an
        // anchor point.
        let clef_y = match clef {
            Clef::Treble => -STAFF_LINE_GAP * 3.6,
            Clef::Bass => -STAFF_LINE_GAP * 0.6,
            Clef::Alto | Clef::Percussion => -STAFF_LINE_GAP * 2.0,
        };

        nuon::label()
            .text(clef_glyph)
            .font_family("Leland")
            .font_size(STAFF_LINE_GAP * 7.0)
            .x(PAD)
            .y(clef_y)
            .width(CLEF_W)
            .height(STAFF_LINE_GAP * 8.0)
            .build(ui);

        key_signature(ui, clef, fifths, PAD + CLEF_W);

        // Barlines closing the bar on both ends, same as any real staff.
        for x in [content_x0 - 6.0, content_x1] {
            nuon::quad()
                .x(x)
                .y(0.0)
                .width(1.4)
                .height(STAFF_LINE_GAP * 4.0)
                .color(LINE_COLOR)
                .build(ui);
        }

        let mut stems: Vec<StemInfo> = Vec::new();

        for note in track.notes.iter() {
            let Some(info) = note.notation else {
                continue;
            };

            if note.start < measure_start || note.start >= measure_end {
                continue;
            }

            let fraction = (note.start - measure_start).as_secs_f32() / bar_len;
            let x = content_x0 + fraction * (content_x1 - content_x0);

            let is_sounding = song_time >= note.start && song_time < note.start + note.duration;
            let color = if is_sounding {
                note_color
            } else {
                [DIM_NOTE_COLOR[0], DIM_NOTE_COLOR[1], DIM_NOTE_COLOR[2]]
            };

            let shape = shape_for(&info, tempo_track, ppq, note);
            let bottom = clef_bottom_line(clef);
            let steps_above_bottom = diatonic_number(info.step, info.octave) - bottom;
            let y = STAFF_LINE_GAP * 4.0 - steps_above_bottom as f32 * STEP;

            note_head(ui, info, shape, steps_above_bottom, x, y, color);

            if shape.stem {
                stems.push(StemInfo {
                    x,
                    y,
                    stem_down: steps_above_bottom >= 4,
                    flags: shape.flags,
                    beam: info.beam,
                    color,
                });
            }
        }

        draw_stems_and_beams(ui, &stems);
    });
}

/// A note whose stem (and possibly beam) is drawn in a second pass, once
/// every note in the bar is known - a beam has to connect several noteheads,
/// so it can't be drawn note-by-note the way the notehead itself can.
struct StemInfo {
    x: f32,
    y: f32,
    stem_down: bool,
    flags: u8,
    beam: Option<BeamState>,
    color: [u8; 3],
}

/// Walks a bar's notes in order, grouping consecutive `Begin..End` runs into
/// a shared beam and falling back to individual flags for anything left
/// over (a lone note, or a beam left dangling by a malformed score).
fn draw_stems_and_beams(ui: &mut Ui, stems: &[StemInfo]) {
    let mut group: Vec<&StemInfo> = Vec::new();

    for item in stems {
        match item.beam {
            Some(BeamState::Begin) => {
                flush_group(ui, &mut group);
                group.push(item);
            }
            Some(BeamState::Continue) => group.push(item),
            Some(BeamState::End) => {
                group.push(item);
                flush_group(ui, &mut group);
            }
            None => {
                flush_group(ui, &mut group);
                draw_stem(ui, item, None);
            }
        }
    }
    flush_group(ui, &mut group);
}

fn flush_group(ui: &mut Ui, group: &mut Vec<&StemInfo>) {
    if group.len() >= 2 {
        draw_beamed_group(ui, group);
    } else {
        for item in group.iter() {
            draw_stem(ui, item, None);
        }
    }
    group.clear();
}

const STEM_LEN: f32 = STAFF_LINE_GAP * 3.5;

fn stem_x(item: &StemInfo) -> f32 {
    if item.stem_down {
        item.x - 6.0
    } else {
        item.x + 5.5
    }
}

/// A lone note's stem, with its own flags if it has any. `beam_y`, when
/// given, is where a shared beam this note belongs to sits - the stem
/// reaches that instead of using its own fixed length.
fn draw_stem(ui: &mut Ui, item: &StemInfo, beam_y: Option<f32>) {
    let (top, height) = match beam_y {
        Some(beam_y) if item.stem_down => (item.y, beam_y - item.y),
        Some(beam_y) => (beam_y, item.y - beam_y),
        None if item.stem_down => (item.y, STEM_LEN),
        None => (item.y - STEM_LEN, STEM_LEN),
    };
    let x = stem_x(item);

    nuon::quad()
        .x(x)
        .y(top)
        .width(1.4)
        .height(height.max(1.0))
        .color(item.color)
        .build(ui);

    if beam_y.is_none() && item.flags > 0 {
        let flag_glyph = match (item.flags, item.stem_down) {
            (1, false) => sheet::flag_8th_up(),
            (1, true) => sheet::flag_8th_down(),
            (2, false) => sheet::flag_16th_up(),
            (2, true) => sheet::flag_16th_down(),
            (_, false) => sheet::flag_32nd_up(),
            (_, true) => sheet::flag_32nd_down(),
        };

        let flag_y = if item.stem_down { top + height } else { top };
        nuon::label()
            .text(flag_glyph)
            .font_family("Leland")
            .font_size(STAFF_LINE_GAP * 3.2)
            .x(x - 1.0)
            .y(flag_y - STAFF_LINE_GAP * (if item.stem_down { 0.2 } else { 1.6 }))
            .width(16.0)
            .height(STAFF_LINE_GAP * 3.2)
            .color(item.color)
            .build(ui);
    }
}

/// A run of beamed notes: one stem per note reaching a shared, flat beam
/// line, plus extra beam segments between adjacent pairs for anything
/// shorter than an eighth note - `min(a.flags, b.flags)` naturally produces
/// the shorter "partial beam" look a mixed-value run (like an eighth
/// followed by two sixteenths) is supposed to have.
fn draw_beamed_group(ui: &mut Ui, group: &[&StemInfo]) {
    let stem_down = group.iter().filter(|s| s.stem_down).count() * 2 >= group.len();

    let beam_y = if stem_down {
        group
            .iter()
            .map(|s| s.y + STEM_LEN)
            .fold(f32::MIN, f32::max)
    } else {
        group
            .iter()
            .map(|s| s.y - STEM_LEN)
            .fold(f32::MAX, f32::min)
    };

    for item in group {
        draw_stem(ui, item, Some(beam_y));
    }

    for pair in group.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let levels = a.flags.min(b.flags).max(1);
        let (ax, bx) = (stem_x(a).min(stem_x(b)), stem_x(a).max(stem_x(b)));

        for level in 0..levels {
            let offset = level as f32 * 4.0;
            let y = if stem_down {
                beam_y - 3.0 - offset
            } else {
                beam_y + offset
            };

            nuon::quad()
                .x(ax)
                .y(y)
                .width((bx - ax).max(1.0))
                .height(3.0)
                .color(a.color)
                .build(ui);
        }
    }
}

/// How a note is drawn: notehead shape, whether it gets a stem, how many
/// flags a lone copy of it would carry (a beamed note draws beam segments
/// instead - see [`draw_beamed_group`]), and how many augmentation dots.
#[derive(Clone, Copy)]
struct DurationShape {
    notehead: &'static str,
    stem: bool,
    flags: u8,
    dots: u8,
}

/// The shape a note is drawn with - straight from the score's `<type>`/
/// `<dot>` when it has one, since that's the actual engraved answer, not a
/// guess. Falls back to inferring one from the note's real sounding length
/// only for notation lacking an explicit `<type>` (rare, but legal
/// MusicXML).
fn shape_for(
    info: &NotationInfo,
    tempo_track: &TempoTrack,
    ppq: f32,
    note: &MidiNote,
) -> DurationShape {
    let Some(value) = info.value else {
        let start_pulses = tempo_track.duration_to_pulses(note.start);
        let end_pulses = tempo_track.duration_to_pulses(note.start + note.duration);
        let quarters = (end_pulses.saturating_sub(start_pulses)) as f32 / ppq.max(1.0);
        return classify_duration(quarters);
    };

    let notehead = match value {
        NoteValue::Whole => sheet::notehead_whole(),
        NoteValue::Half => sheet::notehead_half(),
        _ => sheet::notehead_black(),
    };

    DurationShape {
        notehead,
        stem: value != NoteValue::Whole,
        flags: value.flags(),
        dots: info.dots,
    }
}

/// Classifies a note's length, in quarter notes, into the shape it would be
/// engraved with. Thresholds sit halfway between each standard duration (and
/// its dotted variant) so real-world timing/quantization noise still lands
/// on the right symbol.
fn classify_duration(quarters: f32) -> DurationShape {
    let (notehead, stem, flags, dots) = if quarters >= 3.5 {
        (sheet::notehead_whole(), false, 0, 0)
    } else if quarters >= 2.5 {
        (sheet::notehead_half(), true, 0, 1)
    } else if quarters >= 1.75 {
        (sheet::notehead_half(), true, 0, 0)
    } else if quarters >= 1.25 {
        (sheet::notehead_black(), true, 0, 1)
    } else if quarters >= 0.875 {
        (sheet::notehead_black(), true, 0, 0)
    } else if quarters >= 0.625 {
        (sheet::notehead_black(), true, 1, 1)
    } else if quarters >= 0.4375 {
        (sheet::notehead_black(), true, 1, 0)
    } else if quarters >= 0.3125 {
        (sheet::notehead_black(), true, 2, 1)
    } else if quarters >= 0.1875 {
        (sheet::notehead_black(), true, 2, 0)
    } else {
        (sheet::notehead_black(), true, 3, 0)
    };

    DurationShape {
        notehead,
        stem,
        flags,
        dots,
    }
}

/// Position, in diatonic steps, of `step`/`octave` from C0 - a single
/// monotonic ladder a staff position is just an offset into.
fn diatonic_number(step: char, octave: i32) -> i32 {
    let index = match step {
        'C' => 0,
        'D' => 1,
        'E' => 2,
        'F' => 3,
        'G' => 4,
        'A' => 5,
        'B' => 6,
        _ => 0,
    };
    octave * 7 + index
}

/// The bottom line of the staff, in the same ladder as [`diatonic_number`].
fn clef_bottom_line(clef: Clef) -> i32 {
    match clef {
        Clef::Treble => diatonic_number('E', 4),
        Clef::Bass => diatonic_number('G', 2),
        Clef::Alto => diatonic_number('F', 3),
        Clef::Percussion => diatonic_number('E', 4),
    }
}

/// Sharps/flats, in the fixed order they're always written in, as staff
/// positions relative to the bottom line - the standard layout every
/// engraving program uses, it just isn't derivable from anything simpler.
fn key_signature_offsets(clef: Clef, fifths: i8) -> Vec<i32> {
    if fifths == 0 {
        return Vec::new();
    }

    // Offsets from the bottom line, in diatonic steps, treble clef.
    const TREBLE_SHARPS: [i32; 7] = [8, 5, 9, 6, 3, 7, 4]; // F5 C5 G5 D5 A4 E5 B4
    const TREBLE_FLATS: [i32; 7] = [4, 7, 3, 6, 2, 5, 1]; // B4 E5 A4 D5 G4 C5 F4
    const BASS_SHARPS: [i32; 7] = [6, 3, 7, 4, 1, 5, 2];
    const BASS_FLATS: [i32; 7] = [2, 5, 1, 4, 0, 3, -1];

    let (sharps, flats) = match clef {
        Clef::Bass => (&BASS_SHARPS, &BASS_FLATS),
        // Alto/percussion don't get a proper table; treble's is a reasonable
        // fallback rather than drawing nothing.
        _ => (&TREBLE_SHARPS, &TREBLE_FLATS),
    };

    let table = if fifths > 0 { sharps } else { flats };
    table[..fifths.unsigned_abs() as usize % 8].to_vec()
}

fn key_signature(ui: &mut Ui, clef: Clef, fifths: i8, x: f32) {
    let offsets = key_signature_offsets(clef, fifths);
    if offsets.is_empty() {
        return;
    }

    let bottom = clef_bottom_line(clef);
    let glyph = if fifths > 0 {
        sheet::accidental_sharp()
    } else {
        sheet::accidental_flat()
    };

    for (i, offset) in offsets.iter().enumerate() {
        let y = STAFF_LINE_GAP * 4.0 - (bottom + offset - bottom) as f32 * STEP;
        nuon::label()
            .text(glyph)
            .font_family("Leland")
            .font_size(STAFF_LINE_GAP * 3.2)
            .x(x + i as f32 * 8.0)
            .y(y - STAFF_LINE_GAP * 1.6)
            .width(10.0)
            .height(STAFF_LINE_GAP * 3.2)
            .build(ui);
    }
}

/// Notehead, accidental, ledger lines and augmentation dots - everything
/// about a note that doesn't depend on its neighbors. The stem (and, for a
/// beamed note, the beam itself) is drawn separately once every note in the
/// bar is known - see [`draw_stems_and_beams`].
fn note_head(
    ui: &mut Ui,
    pitch: NotationInfo,
    shape: DurationShape,
    steps_above_bottom: i32,
    x: f32,
    y: f32,
    color: [u8; 3],
) {
    // Ledger lines for anything poking out above or below the staff.
    if steps_above_bottom < 0 {
        let mut step = -2;
        while step >= steps_above_bottom {
            if step % 2 == 0 {
                let ly = STAFF_LINE_GAP * 4.0 - step as f32 * STEP;
                nuon::quad()
                    .x(x - 6.0)
                    .y(ly)
                    .width(20.0)
                    .height(1.0)
                    .color(LINE_COLOR)
                    .build(ui);
            }
            step -= 1;
        }
    } else if steps_above_bottom > 8 {
        let mut step = 10;
        while step <= steps_above_bottom {
            if step % 2 == 0 {
                let ly = STAFF_LINE_GAP * 4.0 - step as f32 * STEP;
                nuon::quad()
                    .x(x - 6.0)
                    .y(ly)
                    .width(20.0)
                    .height(1.0)
                    .color(LINE_COLOR)
                    .build(ui);
            }
            step += 1;
        }
    }

    if pitch.alter != 0 {
        let glyph = match pitch.alter {
            ..=-1 => sheet::accidental_flat(),
            1.. => sheet::accidental_sharp(),
            0 => sheet::accidental_natural(),
        };

        nuon::label()
            .text(glyph)
            .font_family("Leland")
            .font_size(STAFF_LINE_GAP * 3.0)
            .x(x - 16.0)
            .y(y - STAFF_LINE_GAP * 1.5)
            .width(14.0)
            .height(STAFF_LINE_GAP * 3.0)
            .color(color)
            .build(ui);
    }

    for dot in 0..shape.dots {
        nuon::quad()
            .x(x + 9.0 + dot as f32 * 5.0)
            .y(y - STAFF_LINE_GAP * 0.15)
            .width(3.0)
            .height(3.0)
            .border_radius([1.5; 4])
            .color(color)
            .build(ui);
    }

    nuon::label()
        .text(shape.notehead)
        .font_family("Leland")
        .font_size(STAFF_LINE_GAP * 2.6)
        .x(x - 6.0)
        .y(y - STAFF_LINE_GAP * 1.3)
        .width(14.0)
        .height(STAFF_LINE_GAP * 2.6)
        .color(color)
        .build(ui);
}
