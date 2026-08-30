//! A scrolling staff notation view, built from the pitch spelling MusicXML
//! files keep and plain midi files don't - see [`midi_file::NotationPitch`].
//!
//! Notes travel right to left as the song plays, crossing a fixed playhead
//! near the left edge, one staff per track that has a clef.

use midi_file::{Clef, MidiTrack, NotationPitch};
use nuon::Ui;
use std::time::Duration;

use crate::{context::Context, icons::sheet, song::Song};

const STAFF_LINE_GAP: f32 = 8.0;
/// Vertical distance between adjacent diatonic steps - half a line gap,
/// since a line and the space above it are one step apart.
const STEP: f32 = STAFF_LINE_GAP / 2.0;
const STAFF_GAP: f32 = 46.0;
const PAD: f32 = 18.0;
const CLEF_W: f32 = 34.0;
/// How far right of the playhead the view looks, in seconds.
const LOOKAHEAD: f32 = 4.0;
/// How long a note lingers left of the playhead after it starts.
const LOOKBACK: f32 = 0.5;

const LINE_COLOR: [u8; 4] = [150, 148, 160, 200];
const NOTE_COLOR: [u8; 3] = [235, 235, 240];
const PLAYHEAD_COLOR: [u8; 4] = [255, 196, 84, 220];

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

    PAD * 2.0 + staves * (STAFF_LINE_GAP * 4.0) + (staves - 1.0).max(0.0) * STAFF_GAP
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
    let now = song_time.as_secs_f32();
    let playhead_x = PAD + CLEF_W + 10.0;
    let px_per_sec = (width - playhead_x - PAD) / LOOKAHEAD;

    nuon::translate().x(0.0).y(y).build(ui, |ui| {
        nuon::quad()
            .size(width, panel_h)
            .color(nuon::Color::new_u8(24, 23, 28, 0.88))
            .border_radius([12.0; 4])
            .build(ui);

        nuon::quad()
            .x(playhead_x)
            .y(PAD - 4.0)
            .width(2.0)
            .height(panel_h - PAD * 2.0 + 8.0)
            .color(PLAYHEAD_COLOR)
            .build(ui);

        let key = song.file.signature_track.key_signature_at(&song_time);
        let note_color = ctx
            .config
            .color_schema()
            .first()
            .map(|c| [c.base.0, c.base.1, c.base.2])
            .unwrap_or(NOTE_COLOR);

        let mut staff_y = PAD;
        for track in staves {
            let clef = track.clef.unwrap();
            staff(
                ui,
                track,
                clef,
                note_color,
                key.map(|k| k.fifths).unwrap_or(0),
                now,
                playhead_x,
                px_per_sec,
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
    now: f32,
    playhead_x: f32,
    px_per_sec: f32,
    width: f32,
    top_y: f32,
) {
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

        for note in track.notes.iter() {
            let Some(pitch) = note.notation else {
                continue;
            };

            let start = note.start.as_secs_f32();
            let dt = start - now;
            if !(-LOOKBACK..=LOOKAHEAD).contains(&dt) {
                continue;
            }

            let x = playhead_x + dt * px_per_sec;
            notehead(ui, clef, pitch, x, note_color);
        }
    });
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

fn notehead(ui: &mut Ui, clef: Clef, pitch: NotationPitch, x: f32, color: [u8; 3]) {
    let bottom = clef_bottom_line(clef);
    let steps_above_bottom = diatonic_number(pitch.step, pitch.octave) - bottom;
    let y = STAFF_LINE_GAP * 4.0 - steps_above_bottom as f32 * STEP;

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

    nuon::label()
        .text(sheet::notehead_black())
        .font_family("Leland")
        .font_size(STAFF_LINE_GAP * 2.6)
        .x(x - 6.0)
        .y(y - STAFF_LINE_GAP * 1.3)
        .width(14.0)
        .height(STAFF_LINE_GAP * 2.6)
        .color(color)
        .build(ui);
}
