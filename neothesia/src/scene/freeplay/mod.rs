use std::time::Duration;

use midi_file::midly::MidiMessage;
use neothesia_core::render::{GlowRenderer, GuidelineRenderer, QuadRenderer, TextRenderer};
use winit::{
    event::WindowEvent,
    keyboard::{Key, NamedKey},
};

use crate::{
    NeothesiaEvent,
    context::Context,
    scene::{
        MouseToMidiEventState, NuonRenderer, Scene,
        freeplay::recorder::{FreeplayRecorder, Preview, RecorderStatus},
        playing_scene::Keyboard,
    },
    song::Song,
    utils::{BoxFuture, noop_waker_ref, window::WinitEvent},
};

mod recorder;

type MsgFn = Box<dyn FnOnce(&mut FreeplayScene, &mut Context)>;

fn on_async<T, Fut, FN>(future: Fut, f: FN) -> BoxFuture<MsgFn>
where
    T: 'static,
    Fut: Future<Output = T> + Send + 'static,
    FN: FnOnce(T, &mut FreeplayScene, &mut Context) + Send + 'static,
{
    Box::pin(async {
        let res = future.await;
        let f: MsgFn = Box::new(move |state, ctx| f(res, state, ctx));
        f
    })
}

pub struct FreeplayScene {
    keyboard: Keyboard,
    guidelines: GuidelineRenderer,

    text_renderer: TextRenderer,
    quad_renderer_bg: QuadRenderer,
    quad_renderer_fg: QuadRenderer,
    glow: Option<GlowRenderer>,

    // TODO: This does not make sens, but get's us going without refactoring
    song: Option<Song>,

    nuon_renderer: NuonRenderer,
    nuon: nuon::Ui,
    mouse_to_midi_state: MouseToMidiEventState,
    deduced_chord_name: String,

    recorder: FreeplayRecorder,
    recorder_status: RecorderStatus,
    preview: Option<Preview>,

    context: std::task::Context<'static>,
    futures: Vec<BoxFuture<MsgFn>>,
}

impl FreeplayScene {
    pub fn new(ctx: &mut Context, song: Option<Song>) -> Self {
        let mut keyboard = Keyboard::new(ctx, Default::default());
        keyboard.set_pressed_by_user_colors(ctx.config.color_schema()[0].clone());

        let keyboard_layout = keyboard.layout();

        let guidelines = GuidelineRenderer::new(
            keyboard_layout.clone(),
            *keyboard.pos(),
            ctx.config.vertical_guidelines(),
            false,
            Default::default(),
        );

        let text_renderer = ctx.text_renderer_factory.new_renderer();

        let quad_renderer_bg = ctx.quad_renderer_factory.new_renderer();
        let quad_renderer_fg = ctx.quad_renderer_factory.new_renderer();

        let glow = ctx.config.glow().then_some(GlowRenderer::new(
            &ctx.gpu,
            &ctx.transform,
            keyboard.layout(),
        ));

        Self {
            keyboard,
            guidelines,
            text_renderer,
            quad_renderer_bg,
            quad_renderer_fg,
            glow,
            song,
            nuon_renderer: NuonRenderer::new(ctx),
            nuon: nuon::Ui::new(),
            mouse_to_midi_state: MouseToMidiEventState::default(),
            deduced_chord_name: String::new(),
            recorder: FreeplayRecorder::default(),
            recorder_status: RecorderStatus::default(),
            preview: None,

            context: std::task::Context::from_waker(noop_waker_ref()),
            futures: Vec::new(),
        }
    }

    fn update_glow(&mut self, delta: Duration) {
        let Some(glow) = &mut self.glow else {
            return;
        };

        glow.clear();

        let keys = &self.keyboard.layout().keys;
        let states = self.keyboard.key_states();

        for (key, state) in keys.iter().zip(states) {
            let Some(mut color) = state.pressed_by_user().copied() else {
                continue;
            };

            color.r *= 0.5;
            color.g *= 0.5;
            color.b *= 0.5;

            glow.push(
                key.id(),
                color,
                key.x(),
                self.keyboard.pos().y,
                key.width(),
                delta,
            );
        }
    }

    fn update_ui(&mut self, ctx: &mut Context) {
        recorder::update_preview_ui(self, ctx);

        nuon::label()
            .text(&self.deduced_chord_name)
            .font_size(25.0)
            .y(self.keyboard.pos().y - 25.0 - 10.0)
            .height(25.0)
            .width(ctx.window_state.logical_size.width)
            .build(&mut self.nuon);
    }

    fn resize(&mut self, ctx: &mut Context) {
        self.keyboard.resize(ctx);
        self.guidelines.set_layout(self.keyboard.layout().clone());
        self.guidelines.set_pos(*self.keyboard.pos());
        if let Some(preview) = self.preview.as_mut() {
            preview.resize(&self.keyboard, ctx);
        }
    }

    fn dispatch_futures(&mut self, ctx: &mut Context) {
        let mut cbs = Vec::new();
        self.futures
            .retain_mut(|f| match f.as_mut().poll(&mut self.context) {
                std::task::Poll::Ready(msg) => {
                    cbs.push(msg);
                    false
                }
                std::task::Poll::Pending => true,
            });

        for cb in cbs {
            cb(self, ctx);
        }
    }
}

impl Scene for FreeplayScene {
    fn update(&mut self, ctx: &mut Context, delta: Duration) {
        self.quad_renderer_bg.clear();
        self.quad_renderer_fg.clear();

        self.dispatch_futures(ctx);

        if let Some(preview) = self.preview.as_mut() {
            preview.update(&mut self.keyboard, ctx, delta);
        }

        let time = 0.0;

        self.guidelines.update(
            &mut self.quad_renderer_bg,
            ctx.config.animation_speed(),
            ctx.window_state.scale_factor as f32,
            time,
            ctx.window_state.logical_size,
        );
        self.keyboard
            .update(&mut self.quad_renderer_fg, &mut self.text_renderer);

        self.update_glow(delta);

        self.quad_renderer_bg.prepare();
        self.quad_renderer_fg.prepare();

        if let Some(glow) = &mut self.glow {
            glow.prepare();
        }

        self.text_renderer.update(
            ctx.window_state.physical_size,
            ctx.window_state.scale_factor as f32,
        );

        self.update_ui(ctx);

        super::render_nuon(&mut self.nuon, &mut self.nuon_renderer, ctx);
    }

    fn render<'pass>(&'pass mut self, rpass: &mut wgpu_jumpstart::RenderPass<'pass>) {
        self.quad_renderer_bg.render(rpass);
        if let Some(preview) = self.preview.as_mut() {
            preview.render(rpass);
        }
        self.quad_renderer_fg.render(rpass);
        if let Some(glow) = &self.glow {
            glow.render(rpass);
        }
        self.text_renderer.render(rpass);
        self.nuon_renderer.render(rpass);
    }

    fn window_event(&mut self, ctx: &mut Context, event: &WindowEvent) {
        if event.window_resized() || event.scale_factor_changed() {
            self.resize(ctx)
        }

        if event.back_mouse_pressed() || event.key_released(Key::Named(NamedKey::Escape)) {
            ctx.proxy
                .send_event(NeothesiaEvent::MainMenu(self.song.clone()))
                .ok();
        }

        if event.key_released(Key::Named(NamedKey::Space)) && self.preview.is_some() {
            recorder::toggle_preview_playback(self);
        }

        super::handle_nuon_window_event(&mut self.nuon, event, ctx);
        super::handle_pc_keyboard_to_midi_event(ctx, event);
        super::handle_mouse_to_midi_event(
            &mut self.keyboard,
            &mut self.mouse_to_midi_state,
            ctx,
            event,
        );
    }

    fn midi_event(&mut self, ctx: &mut Context, channel: u8, message: &MidiMessage) {
        self.recorder.push_event(channel, *message);
        self.keyboard.user_midi_event(message);
        ctx.output_manager
            .connection()
            .midi_event(0.into(), *message);

        if let MidiMessage::NoteOn { .. } = message {
            let start = self.keyboard.layout().range.start();

            let notes: Vec<u8> = self
                .keyboard
                .key_states()
                .iter()
                .enumerate()
                .filter(|(_, state)| state.pressed_by_user().is_some())
                .map(|(id, _)| id as u8 + start)
                .collect();

            self.deduced_chord_name = chords::deduce_name(&notes).unwrap_or_default();
        }
    }
}

pub(crate) mod chords {
    /// Get chord name based on notes, eg. Cmaj7
    ///
    /// Backed by the same analysis the player uses, so a chord keeps its name
    /// even when it is inverted, voiced across octaves, or missing its fifth.
    pub fn deduce_name(midi_notes: &[u8]) -> Option<String> {
        match midi_notes {
            [] => None,
            [note] => Some(music_theory::note_name(*note, false).to_string()),
            notes => music_theory::detect(notes).map(|chord| chord.symbol(false)),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_major_chords() {
            assert_eq!(deduce_name(&[60, 64, 67]).unwrap(), "C");
            assert_eq!(deduce_name(&[62, 66, 69]).unwrap(), "D");
        }

        #[test]
        fn test_minor_chords() {
            assert_eq!(deduce_name(&[60, 63, 67]).unwrap(), "Cm");
            assert_eq!(deduce_name(&[57, 60, 64]).unwrap(), "Am");
        }

        #[test]
        fn test_seventh_chords() {
            assert_eq!(deduce_name(&[60, 64, 67, 71]).unwrap(), "Cmaj7");
            assert_eq!(deduce_name(&[60, 64, 67, 70]).unwrap(), "C7");
            assert_eq!(deduce_name(&[60, 63, 67, 70]).unwrap(), "Cm7");
        }

        #[test]
        fn test_other_chords() {
            assert_eq!(deduce_name(&[60, 63, 66]).unwrap(), "Cdim");
            assert_eq!(deduce_name(&[60, 64, 68]).unwrap(), "Caug");
            assert_eq!(deduce_name(&[60, 65, 67]).unwrap(), "Csus4");
            assert_eq!(deduce_name(&[60, 67]).unwrap(), "C5");
        }

        #[test]
        fn test_edge_cases() {
            assert_eq!(deduce_name(&[]), None);
            assert_eq!(deduce_name(&[60]).unwrap(), "C");
            // Multiple octaves should normalize
            assert_eq!(deduce_name(&[48, 64, 67, 72]).unwrap(), "C");
        }

        #[test]
        fn inversions_keep_their_name() {
            // First inversion of C major used to come out as nothing at all.
            assert_eq!(deduce_name(&[64, 67, 72]).unwrap(), "C/E");
        }
    }
}
