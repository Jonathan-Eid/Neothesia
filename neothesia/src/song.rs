use midi_file::MidiTrack;

use crate::context::Context;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PlayerConfig {
    Mute,
    Auto,
    Human,
}

#[derive(Debug, Clone)]
pub struct TrackConfig {
    pub track_id: usize,
    pub player: PlayerConfig,
    pub visible: bool,
}

#[derive(Default, Debug, Clone)]
pub struct SongConfig {
    pub tracks: Box<[TrackConfig]>,
}

impl SongConfig {
    /// Whether a track takes part in the performance at all: it has to be
    /// visible, and, when `hide_muted` is set, it has to be sounding too.
    pub fn is_active(&self, track_id: usize, hide_muted: bool) -> bool {
        self.tracks.get(track_id).is_none_or(|track| {
            track.visible && !(hide_muted && track.player == PlayerConfig::Mute)
        })
    }

    /// Tracks to keep out of the waterfall.
    pub fn hidden_tracks(&self, hide_muted: bool) -> Vec<usize> {
        self.tracks
            .iter()
            .filter(|track| !self.is_active(track.track_id, hide_muted))
            .map(|track| track.track_id)
            .collect()
    }
}

impl SongConfig {
    fn new(tracks: &[MidiTrack]) -> Self {
        let tracks: Vec<_> = tracks
            .iter()
            .map(|t| {
                let is_drums = t.has_drums && !t.has_other_than_drums;
                TrackConfig {
                    track_id: t.track_id,
                    player: PlayerConfig::Auto,
                    visible: !is_drums,
                }
            })
            .collect();
        Self {
            tracks: tracks.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Song {
    pub file: midi_file::MidiFile,
    pub config: SongConfig,
}

impl Song {
    pub fn new(file: midi_file::MidiFile) -> Self {
        let config = SongConfig::new(&file.tracks);
        Self { file, config }
    }

    pub fn from_env(ctx: &Context) -> Option<Self> {
        let args: Vec<String> = std::env::args().collect();
        let midi_file = if args.len() > 1 {
            midi_file::MidiFile::new(&args[1]).ok()
        } else if let Some(last) = ctx.config.last_opened_song() {
            midi_file::MidiFile::new(last).ok()
        } else {
            None
        };

        Some(Self::new(midi_file?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SongConfig {
        SongConfig {
            tracks: vec![
                TrackConfig {
                    track_id: 0,
                    player: PlayerConfig::Auto,
                    visible: true,
                },
                TrackConfig {
                    track_id: 1,
                    player: PlayerConfig::Mute,
                    visible: true,
                },
                TrackConfig {
                    track_id: 2,
                    player: PlayerConfig::Auto,
                    visible: false,
                },
                TrackConfig {
                    track_id: 3,
                    player: PlayerConfig::Human,
                    visible: true,
                },
            ]
            .into(),
        }
    }

    #[test]
    fn muted_tracks_drop_out_when_asked() {
        let config = config();

        assert!(config.is_active(0, true));
        assert!(!config.is_active(1, true));
        assert!(!config.is_active(2, true));
        // A track you play yourself is still part of the song.
        assert!(config.is_active(3, true));

        assert_eq!(config.hidden_tracks(true), [1, 2]);
    }

    #[test]
    fn muted_tracks_stay_when_not_asked() {
        let config = config();

        assert!(config.is_active(1, false));
        assert_eq!(config.hidden_tracks(false), [2]);
    }
}
