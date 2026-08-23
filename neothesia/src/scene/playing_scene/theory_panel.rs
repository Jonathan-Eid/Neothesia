//! The panel that explains what is being played right now.
//!
//! Left column is the left hand, right column is the right hand, and the middle
//! holds the harmony both of them make together, plus where that sits in the
//! key and in the bar.

use nuon::{TextJustify, Ui};

use super::analysis::{HandSnapshot, Snapshot};
use crate::context::Context;

pub const HEIGHT: f32 = 116.0;

const PAD: f32 = 18.0;
const DIM: [u8; 3] = [150, 148, 160];
const FAINT: [u8; 3] = [110, 108, 120];

pub struct PanelColors {
    pub left: [u8; 3],
    pub right: [u8; 3],
}

impl PanelColors {
    pub fn from_config(ctx: &Context) -> Self {
        let schema = ctx.config.color_schema();

        let color = |id: usize| {
            let color = &schema[id % schema.len().max(1)].base;
            [color.0, color.1, color.2]
        };

        Self {
            left: color(1),
            right: color(0),
        }
    }
}

pub fn build(ui: &mut Ui, ctx: &Context, snapshot: &Snapshot, stepping: bool, width: f32, y: f32) {
    let colors = PanelColors::from_config(ctx);
    let column = (width - PAD * 2.0) / 3.0;

    nuon::translate().x(0.0).y(y).build(ui, |ui| {
        nuon::quad()
            .size(width, HEIGHT)
            .color(nuon::Color::new_u8(24, 23, 28, 0.88))
            .border_radius([12.0; 4])
            .build(ui);

        hand(ui, &snapshot.left, "LEFT", colors.left, PAD, column, false);
        hand(
            ui,
            &snapshot.right,
            "RIGHT",
            colors.right,
            width - PAD - column,
            column,
            true,
        );

        center(ui, snapshot, stepping, PAD + column, column);
    });
}

/// One hand: what it is playing, and what that is called.
fn hand(
    ui: &mut Ui,
    hand: &HandSnapshot,
    title: &str,
    color: [u8; 3],
    x: f32,
    width: f32,
    right_side: bool,
) {
    let justify = if right_side {
        TextJustify::Right
    } else {
        TextJustify::Left
    };

    let text = |ui: &mut Ui, text: &str, y: f32, size: f32, color: [u8; 3], bold: bool| {
        nuon::label()
            .text(text)
            .x(x)
            .y(y)
            .width(width)
            .height(size * 1.4)
            .font_size(size)
            .color(color)
            .bold(bold)
            .text_justify(justify)
            .build(ui);
    };

    text(ui, title, 10.0, 11.0, FAINT, false);

    if hand.is_empty() {
        text(ui, "—", 26.0, 24.0, FAINT, false);
        return;
    }

    text(ui, &hand.symbol, 26.0, 24.0, color, true);

    let degree = if hand.roman.is_empty() {
        String::new()
    } else {
        format!("{}   {}", hand.roman, hand.intervals)
    };
    text(ui, &degree, 58.0, 14.0, DIM, false);
    text(ui, &hand.note_names, 80.0, 13.0, FAINT, false);
}

/// The harmony of both hands, and the context it sits in.
fn center(ui: &mut Ui, snapshot: &Snapshot, stepping: bool, x: f32, width: f32) {
    let text = |ui: &mut Ui, text: &str, y: f32, size: f32, color: [u8; 3], bold: bool| {
        nuon::label()
            .text(text)
            .x(x)
            .y(y)
            .width(width)
            .height(size * 1.4)
            .font_size(size)
            .color(color)
            .bold(bold)
            .build(ui);
    };

    let header = if stepping {
        "STEP  , .  both   k  left   l  right"
    } else {
        "HARMONY"
    };
    text(ui, header, 10.0, 11.0, FAINT, false);

    // A single note is not a chord, but naming it still beats a dash.
    let symbol = match snapshot.symbol() {
        symbol if !symbol.is_empty() => symbol,
        _ => snapshot.sounding_names(),
    };

    let headline = if symbol.is_empty() {
        String::from("—")
    } else if snapshot.roman.is_empty() {
        symbol
    } else {
        format!("{}    {}", symbol, snapshot.roman)
    };
    text(ui, &headline, 24.0, 27.0, [255, 255, 255], true);

    // What the chord is called, and the scale it implies.
    let mut detail = snapshot.description();
    if !snapshot.mode.is_empty() && !detail.is_empty() {
        detail.push_str(" · ");
        detail.push_str(snapshot.mode);
    }
    text(ui, &detail, 60.0, 13.0, DIM, false);

    let meter = snapshot.meter.label();
    let grouping = snapshot.meter.grouping_label();
    let meter = if grouping.is_empty() {
        meter
    } else {
        format!("{meter} ({grouping})")
    };

    let mut context = format!(
        "{}  ·  {}  ·  bar {}  beat {}",
        snapshot.key.name(),
        meter,
        snapshot.measure,
        snapshot.beat.floor() as i32
    );
    if !snapshot.rhythm.is_empty() {
        context.push_str("  ·  ");
        context.push_str(snapshot.rhythm);
    }
    text(ui, &context, 82.0, 12.0, FAINT, false);
}
