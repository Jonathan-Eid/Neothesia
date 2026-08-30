use std::{error::Error, fmt};

/// An error that can occur during initialization (i.e., while
/// creating a `MidiInput` or `MidiOutput` object).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitError;

impl Error for InitError {}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        "MIDI support could not be initialized".fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MidiOutputPort(String);

impl std::fmt::Display for MidiOutputPort {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MidiInputPort(String);

impl std::fmt::Display for MidiInputPort {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An error that can occur when sending MIDI messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    InvalidData(&'static str),
    Other(&'static str),
}

impl Error for SendError {}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SendError::InvalidData(msg) | SendError::Other(msg) => msg.fmt(f),
        }
    }
}

#[cfg(not(target_os = "android"))]
mod midir_backend;
#[cfg(not(target_os = "android"))]
pub use midir_backend::{
    MidiInputConnection, MidiInputManager, MidiOutputConnection, MidiOutputManager,
};

// `midir` has no Android backend (no ALSA/CoreMIDI/WinRT equivalent available
// there), so hardware MIDI I/O is unavailable on Android. This stub keeps the
// public API identical so the rest of the app doesn't need to special-case it.
#[cfg(target_os = "android")]
mod android_stub;
#[cfg(target_os = "android")]
pub use android_stub::{
    MidiInputConnection, MidiInputManager, MidiOutputConnection, MidiOutputManager,
};
