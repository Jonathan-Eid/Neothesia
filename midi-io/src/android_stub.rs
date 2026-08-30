use crate::{InitError, MidiInputPort, MidiOutputPort, SendError};

pub struct MidiOutputManager;

impl MidiOutputManager {
    pub fn new() -> Result<Self, InitError> {
        Ok(Self)
    }

    pub fn outputs(&self) -> Vec<MidiOutputPort> {
        Vec::new()
    }

    pub fn connect_output(_port: MidiOutputPort) -> Option<MidiOutputConnection> {
        None
    }
}

pub struct MidiInputManager;

impl MidiInputManager {
    pub fn new() -> Result<Self, InitError> {
        Ok(Self)
    }

    pub fn inputs(&self) -> Vec<MidiInputPort> {
        Vec::new()
    }

    pub fn connect_input<F>(
        _port: MidiInputPort,
        _callback: F,
    ) -> Option<(MidiInputPort, MidiInputConnection)>
    where
        F: FnMut(&[u8]) + Send + 'static,
    {
        None
    }
}

pub struct MidiInputConnection;
pub struct MidiOutputConnection;

impl MidiOutputConnection {
    pub fn send(&mut self, _message: &[u8]) -> Result<(), SendError> {
        Ok(())
    }
}
