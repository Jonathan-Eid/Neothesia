use midly::{MetaMessage, TrackEvent, TrackEventKind};
use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::tempo_track::TempoTrack;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeSignature {
    pub numerator: u8,
    /// Note value of one beat, as a number, so `8` for `7/8`.
    pub denominator: u8,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

impl TimeSignature {
    /// Length of one measure, in pulses.
    pub fn measure_pulses(&self, pulses_per_quarter_note: u16) -> u64 {
        let quarters = self.numerator as u64 * 4;
        let pulses = quarters * pulses_per_quarter_note as u64 / self.denominator.max(1) as u64;

        pulses.max(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySignature {
    /// Position on the circle of fifths, negative for flats.
    pub fifths: i8,
    pub minor: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SignatureEvent<T> {
    pub absolute_pulses: u64,
    pub timestamp: Duration,
    pub signature: T,
}

/// Time and key signatures of a file, both of which can change mid song.
#[derive(Debug, Clone, Default)]
pub struct SignatureTrack {
    time: Arc<[SignatureEvent<TimeSignature>]>,
    key: Arc<[SignatureEvent<KeySignature>]>,
}

impl SignatureTrack {
    pub fn build(track_events: &[Vec<TrackEvent>], tempo_track: &TempoTrack) -> Self {
        // Signatures are often duplicated across tracks, the map gets rid of that.
        let mut time: HashMap<u64, TimeSignature> = HashMap::new();
        let mut key: HashMap<u64, KeySignature> = HashMap::new();

        for events in track_events.iter() {
            let mut pulses: u64 = 0;

            for event in events.iter() {
                pulses += event.delta.as_int() as u64;

                match event.kind {
                    TrackEventKind::Meta(MetaMessage::TimeSignature(
                        numerator,
                        denominator,
                        ..,
                    )) => {
                        time.insert(
                            pulses,
                            TimeSignature {
                                numerator: numerator.max(1),
                                // Midi stores the denominator as a power of two.
                                denominator: 1u8.checked_shl(denominator as u32).unwrap_or(4),
                            },
                        );
                    }
                    TrackEventKind::Meta(MetaMessage::KeySignature(fifths, minor)) => {
                        key.insert(pulses, KeySignature { fifths, minor });
                    }
                    _ => {}
                }
            }
        }

        Self {
            time: Self::collect(time, tempo_track),
            key: Self::collect(key, tempo_track),
        }
    }

    fn collect<T: Copy>(
        map: HashMap<u64, T>,
        tempo_track: &TempoTrack,
    ) -> Arc<[SignatureEvent<T>]> {
        let mut events: Vec<SignatureEvent<T>> = map
            .into_iter()
            .map(|(absolute_pulses, signature)| SignatureEvent {
                absolute_pulses,
                timestamp: tempo_track.pulses_to_duration(absolute_pulses),
                signature,
            })
            .collect();

        events.sort_by_key(|event| event.absolute_pulses);
        events.into()
    }

    pub fn time_signatures(&self) -> &[SignatureEvent<TimeSignature>] {
        &self.time
    }

    pub fn key_signatures(&self) -> &[SignatureEvent<KeySignature>] {
        &self.key
    }

    pub fn time_signature_at(&self, timestamp: &Duration) -> TimeSignature {
        Self::at(&self.time, timestamp)
            .map(|event| event.signature)
            .unwrap_or_default()
    }

    pub fn key_signature_at(&self, timestamp: &Duration) -> Option<KeySignature> {
        Self::at(&self.key, timestamp).map(|event| event.signature)
    }

    fn at<'a, T>(
        events: &'a [SignatureEvent<T>],
        timestamp: &Duration,
    ) -> Option<&'a SignatureEvent<T>> {
        let id = match events.binary_search_by_key(timestamp, |event| event.timestamp) {
            Ok(id) => Some(id),
            Err(id) => id.checked_sub(1),
        };

        id.and_then(|id| events.get(id))
    }

    pub fn time_signature_at_pulses(&self, pulses: u64) -> TimeSignature {
        let id = match self
            .time
            .binary_search_by_key(&pulses, |event| event.absolute_pulses)
        {
            Ok(id) => Some(id),
            Err(id) => id.checked_sub(1),
        };

        id.and_then(|id| self.time.get(id))
            .map(|event| event.signature)
            .unwrap_or_default()
    }

    /// Start of every measure, in pulses, up to (and including) `last_pulses`.
    pub fn measures(&self, pulses_per_quarter_note: u16, last_pulses: u64) -> Vec<u64> {
        let mut measures = Vec::new();
        let mut pulses = 0u64;

        loop {
            measures.push(pulses);

            if pulses > last_pulses || measures.len() > 100_000 {
                break;
            }

            // Meter changes land on a measure boundary, so the signature in
            // effect at the start of the measure is the one that sizes it.
            pulses += self
                .time_signature_at_pulses(pulses)
                .measure_pulses(pulses_per_quarter_note);
        }

        measures
    }
}
