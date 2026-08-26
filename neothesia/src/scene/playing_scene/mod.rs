use midi_file::midly::MidiMessage;
use neothesia_core::render::{
    GlowRenderer, GuidelineRenderer, NoteLabels, QuadRenderer, TextRenderer,
};
use std::time::Duration;
use winit::{
    event::WindowEvent,
    keyboard::{Key, NamedKey},
};

use self::top_bar::TopBar;

use super::{NuonRenderer, Scene};
use crate::{
    NeothesiaEvent, context::Context, render::WaterfallRenderer, scene::MouseToMidiEventState,
    song::Song, utils::window::WinitEvent,
};

mod analysis;
use analysis::{Hand, Snapshot, SongAnalysis};

mod theory_panel;

mod keyboard;
pub use keyboard::Keyboard;

pub(crate) mod midi_player;
use midi_player::MidiPlayer;

mod rewind_controller;
use rewind_controller::RewindController;

mod toast_manager;
use toast_manager::ToastManager;

mod animation;
mod top_bar;

pub struct PlayingScene {
    keyboard: Keyboard,
    waterfall: WaterfallRenderer,
    guidelines: GuidelineRenderer,
    text_renderer: TextRenderer,
    nuon_renderer: NuonRenderer,

    note_labels: Option<NoteLabels>,

    player: MidiPlayer,
    rewind_controller: RewindController,
    quad_renderer_bg: QuadRenderer,
    quad_renderer_fg: QuadRenderer,
    glow: Option<GlowRenderer>,
    toast_manager: ToastManager,

    nuon: nuon::Ui,
    mouse_to_midi_state: MouseToMidiEventState,

    analysis: SongAnalysis,
    snapshot: Option<Snapshot>,
    /// Whether muted tracks are currently kept out of the performance.
    hide_muted: bool,
    /// Stepping through the song by hand, like a debugger.
    stepping: bool,

    top_bar: TopBar,
}

impl PlayingScene {
    pub fn new(ctx: &mut Context, song: Song) -> Self {
        let keyboard = Keyboard::new(ctx, song.config.clone());

        let keyboard_layout = keyboard.layout();

        let guidelines = GuidelineRenderer::new(
            keyboard_layout.clone(),
            *keyboard.pos(),
            ctx.config.vertical_guidelines(),
            ctx.config.horizontal_guidelines(),
            song.file.measures.clone(),
        );

        let hide_muted = ctx.config.hide_muted_tracks();
        let hidden_tracks = song.config.hidden_tracks(hide_muted);

        let mut waterfall = WaterfallRenderer::new(
            &ctx.gpu,
            &song.file.tracks,
            &hidden_tracks,
            &ctx.config,
            &ctx.transform,
            keyboard_layout.clone(),
        );

        let text_renderer = ctx.text_renderer_factory.new_renderer();

        let note_labels = ctx.config.note_labels().then_some(NoteLabels::new(
            *keyboard.pos(),
            waterfall.notes(),
            ctx.text_renderer_factory.new_renderer(),
        ));

        let analysis = SongAnalysis::new(&song, hide_muted);

        let player = MidiPlayer::new(
            ctx.output_manager.connection().clone(),
            song,
            keyboard_layout.range.clone(),
            ctx.config.separate_channels(),
        );
        waterfall.update(player.time_without_lead_in());

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
            note_labels,
            text_renderer,
            nuon_renderer: NuonRenderer::new(ctx),

            waterfall,
            player,
            rewind_controller: RewindController::new(),
            quad_renderer_bg,
            quad_renderer_fg,
            glow,
            toast_manager: ToastManager::default(),

            nuon: nuon::Ui::new(),
            mouse_to_midi_state: MouseToMidiEventState::default(),

            analysis,
            snapshot: None,
            stepping: false,
            hide_muted,

            top_bar: TopBar::new(),
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
            let Some(color) = state.pressed_by_file() else {
                continue;
            };

            glow.push(
                key.id(),
                *color,
                key.x(),
                self.keyboard.pos().y,
                key.width(),
                delta,
            );
        }
    }

    /// Muting a track can take it out of the waterfall and the analysis, which
    /// means rebuilding both when the setting is flipped mid song.
    fn apply_muted_tracks(&mut self, ctx: &mut Context) {
        if self.hide_muted == ctx.config.hide_muted_tracks() {
            return;
        }

        self.hide_muted = ctx.config.hide_muted_tracks();

        let song = self.player.song().clone();
        let hidden_tracks = song.config.hidden_tracks(self.hide_muted);

        self.waterfall = WaterfallRenderer::new(
            &ctx.gpu,
            &song.file.tracks,
            &hidden_tracks,
            &ctx.config,
            &ctx.transform,
            self.keyboard.layout().clone(),
        );
        self.waterfall.update(self.player.time_without_lead_in());

        self.note_labels = ctx.config.note_labels().then_some(NoteLabels::new(
            *self.keyboard.pos(),
            self.waterfall.notes(),
            ctx.text_renderer_factory.new_renderer(),
        ));

        self.analysis = SongAnalysis::new(&song, self.hide_muted);
        self.keyboard.reset_notes();
    }

    /// Time in the song itself, with the lead in taken off.
    fn song_time(&self) -> Duration {
        self.player.time().saturating_sub(*self.player.leed_in())
    }

    fn update_analysis(&mut self, enabled: bool) {
        if !enabled {
            self.snapshot = None;
            return;
        }

        let time = self.song_time();
        self.snapshot = Some(self.analysis.analyse(self.player.song(), time));
    }

    /// Jumps to a point in the song and sounds whatever is held there, so that
    /// a paused player still shows and plays the chord under the playhead.
    fn seek(&mut self, ctx: &Context, song_time: Duration) {
        let player_time = song_time + *self.player.leed_in();
        self.player.set_time(player_time);

        let notes = self.analysis.notes_at(self.player.song(), song_time);

        self.player.play_notes(&notes);
        self.keyboard.reset_notes();
        self.keyboard.press_notes(&ctx.config, &notes);
    }

    /// One step of the song, either hand or both, forwards or backwards.
    fn step(&mut self, ctx: &Context, hand: Option<Hand>, forward: bool) {
        self.player.pause();
        self.stepping = true;

        let now = self.song_time();
        let target = if forward {
            self.analysis.next_onset(hand, now)
        } else {
            self.analysis.previous_onset(hand, now)
        };

        let Some(target) = target else {
            return;
        };

        self.seek(ctx, target);
    }

    fn handle_step_input(&mut self, ctx: &mut Context, event: &WindowEvent) {
        // Letters and brackets are taken by the play along keyboard, so
        // stepping lives on the keys next to them.
        let step = match event.character_released() {
            Some(".") => Some((None, true)),
            Some(",") => Some((None, false)),
            Some("k") => Some((Some(Hand::Left), true)),
            Some("K") => Some((Some(Hand::Left), false)),
            Some("l") => Some((Some(Hand::Right), true)),
            Some("L") => Some((Some(Hand::Right), false)),
            Some("a") => {
                ctx.config.set_theory_panel(!ctx.config.theory_panel());
                None
            }
            _ => None,
        };

        if let Some((hand, forward)) = step {
            self.step(ctx, hand, forward);
        }
    }

    #[profiling::function]
    fn update_midi_player(&mut self, ctx: &Context, delta: Duration) -> f32 {
        if self.top_bar.is_looper_active() && self.player.time() > self.top_bar.loop_end_timestamp()
        {
            self.player.set_time(self.top_bar.loop_start_timestamp());
            self.keyboard.reset_notes();
        }

        if self.player.play_along().are_required_keys_pressed() {
            let delta = (delta / 10) * (ctx.config.speed_multiplier() * 10.0) as u32;
            let midi_events = self.player.update(delta);
            self.keyboard.file_midi_events(&ctx.config, &midi_events);
        }

        self.player.time_without_lead_in() + ctx.config.animation_offset()
    }

    #[profiling::function]
    fn resize(&mut self, ctx: &mut Context) {
        self.keyboard.resize(ctx);

        self.guidelines.set_layout(self.keyboard.layout().clone());
        self.guidelines.set_pos(*self.keyboard.pos());
        if let Some(note_labels) = self.note_labels.as_mut() {
            note_labels.set_pos(*self.keyboard.pos());
        }

        self.waterfall
            .resize(&ctx.config, self.keyboard.layout().clone());
    }
}

impl Scene for PlayingScene {
    #[profiling::function]
    fn update(&mut self, ctx: &mut Context, delta: Duration) {
        self.quad_renderer_bg.clear();
        self.quad_renderer_fg.clear();

        self.apply_muted_tracks(ctx);
        self.rewind_controller.update(&mut self.player, ctx, delta);
        self.toast_manager.update(&mut self.text_renderer);

        let time = self.update_midi_player(ctx, delta);
        self.waterfall.update(time);
        self.guidelines.update(
            &mut self.quad_renderer_bg,
            ctx.config.animation_speed(),
            ctx.window_state.scale_factor as f32,
            time,
            ctx.window_state.logical_size,
        );
        self.keyboard
            .update(&mut self.quad_renderer_fg, &mut self.text_renderer);
        self.update_analysis(ctx.config.theory_panel() || ctx.config.chord_identifier());
        if let Some(note_labels) = self.note_labels.as_mut() {
            note_labels.update(
                ctx.window_state.physical_size,
                ctx.window_state.scale_factor as f32,
                self.keyboard.renderer(),
                ctx.config.animation_speed(),
                time,
            );
        }

        self.update_glow(delta);

        TopBar::update(self, ctx);

        // Outstanding notes only count as "the thing blocking us" while the
        // song is actually trying to move forward on its own.
        let waiting_for = if self.stepping || self.player.is_paused() {
            Vec::new()
        } else {
            self.player.play_along().required_notes()
        };

        if let Some(snapshot) = self.snapshot.as_ref() {
            if ctx.config.theory_panel() {
                theory_panel::build(
                    &mut self.nuon,
                    ctx,
                    snapshot,
                    self.stepping,
                    &waiting_for,
                    snapshot.key.prefers_flats(),
                    ctx.window_state.logical_size.width,
                    self.keyboard.pos().y - theory_panel::HEIGHT - 12.0,
                );
            } else if ctx.config.chord_identifier() {
                nuon::label()
                    .text(snapshot.symbol())
                    .font_size(25.0)
                    .y(self.keyboard.pos().y - 35.0)
                    .height(25.0)
                    .width(ctx.window_state.logical_size.width)
                    .build(&mut self.nuon);
            }
        }

        super::render_nuon(&mut self.nuon, &mut self.nuon_renderer, ctx);

        self.quad_renderer_bg.prepare();
        self.quad_renderer_fg.prepare();

        if let Some(glow) = &mut self.glow {
            glow.prepare();
        }

        #[cfg(debug_assertions)]
        self.text_renderer.queue_fps(
            ctx.fps_ticker.avg(),
            self.top_bar
                .topbar_expand_animation
                .animate_bool(5.0, 80.0, ctx.frame_timestamp),
        );
        self.text_renderer.update(
            ctx.window_state.physical_size,
            ctx.window_state.scale_factor as f32,
        );

        if self.player.is_finished() && !self.player.is_paused() {
            ctx.proxy
                .send_event(NeothesiaEvent::MainMenu(Some(self.player.song().clone())))
                .ok();
        }
    }

    #[profiling::function]
    fn render<'pass>(&'pass mut self, rpass: &mut wgpu_jumpstart::RenderPass<'pass>) {
        self.quad_renderer_bg.render(rpass);
        self.waterfall.render(rpass);
        if let Some(note_labels) = self.note_labels.as_mut() {
            note_labels.render(rpass);
        }
        self.quad_renderer_fg.render(rpass);
        if let Some(glow) = &self.glow {
            glow.render(rpass);
        }
        self.text_renderer.render(rpass);

        self.nuon_renderer.render(rpass);
    }

    fn window_event(&mut self, ctx: &mut Context, event: &WindowEvent) {
        self.rewind_controller
            .handle_window_event(ctx, event, &mut self.player);

        if self.rewind_controller.is_rewinding() {
            self.keyboard.reset_notes();
        }

        if event.back_mouse_pressed() || event.key_released(Key::Named(NamedKey::Escape)) {
            ctx.proxy
                .send_event(NeothesiaEvent::MainMenu(Some(self.player.song().clone())))
                .ok();
        }

        if event.key_released(Key::Named(NamedKey::Space)) {
            self.stepping = false;
            self.player.pause_resume();
        }

        self.handle_step_input(ctx, event);

        handle_settings_input(ctx, &mut self.toast_manager, &mut self.waterfall, event);
        super::handle_pc_keyboard_to_midi_event(ctx, event);
        super::handle_mouse_to_midi_event(
            &mut self.keyboard,
            &mut self.mouse_to_midi_state,
            ctx,
            event,
        );

        if event.window_resized() || event.scale_factor_changed() {
            self.resize(ctx)
        }

        super::handle_nuon_window_event(&mut self.nuon, event, ctx);
    }

    fn midi_event(&mut self, _ctx: &mut Context, channel: u8, message: &MidiMessage) {
        self.player.user_midi_event(channel, message);
        self.keyboard.user_midi_event(message);
    }
}

fn handle_settings_input(
    ctx: &mut Context,
    toast_manager: &mut ToastManager,
    waterfall: &mut WaterfallRenderer,
    event: &WindowEvent,
) {
    if event.key_released(Key::Named(NamedKey::ArrowUp))
        || event.key_released(Key::Named(NamedKey::ArrowDown))
    {
        let amount = if ctx.window_state.modifiers_state.shift_key() {
            0.5
        } else {
            0.1
        };

        if event.key_released(Key::Named(NamedKey::ArrowUp)) {
            ctx.config
                .set_speed_multiplier(ctx.config.speed_multiplier() + amount);
        } else {
            ctx.config
                .set_speed_multiplier(ctx.config.speed_multiplier() - amount);
        }

        toast_manager.speed_toast(ctx.config.speed_multiplier());
        return;
    }

    if event.key_released(Key::Named(NamedKey::PageUp))
        || event.key_released(Key::Named(NamedKey::PageDown))
    {
        let amount = if ctx.window_state.modifiers_state.shift_key() {
            500.0
        } else {
            100.0
        };

        if event.key_released(Key::Named(NamedKey::PageUp)) {
            ctx.config
                .set_animation_speed(ctx.config.animation_speed() + amount);
        } else {
            ctx.config
                .set_animation_speed(ctx.config.animation_speed() - amount);
        }

        waterfall
            .pipeline()
            .set_speed(&ctx.gpu.queue, ctx.config.animation_speed());
        toast_manager.animation_speed_toast(ctx.config.animation_speed());
        return;
    }

    if let Some(ch @ ("_" | "-" | "+" | "=")) = event.character_released() {
        let amount = if ctx.window_state.modifiers_state.shift_key() {
            0.1
        } else {
            0.01
        };

        if matches!(ch, "-" | "_") {
            ctx.config
                .set_animation_offset(ctx.config.animation_offset() - amount);
        } else {
            ctx.config
                .set_animation_offset(ctx.config.animation_offset() + amount);
        }

        toast_manager.offset_toast(ctx.config.animation_offset());
    }
}
