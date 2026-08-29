use crate::{InitError, MidiInputPort, MidiOutputPort, SendError};

impl From<midir::InitError> for InitError {
    fn from(_: midir::InitError) -> Self {
        Self
    }
}

pub struct MidiOutputManager {
    output: midir::MidiOutput,
}

impl MidiOutputManager {
    pub fn new() -> Result<Self, InitError> {
        let output = midir::MidiOutput::new("MidiIo-out-manager")?;

        Ok(Self { output })
    }

    pub fn outputs(&self) -> Vec<MidiOutputPort> {
        self.output
            .ports()
            .iter()
            .filter_map(|p| self.output.port_name(p).ok())
            .map(MidiOutputPort)
            .collect()
    }

    pub fn connect_output(port: MidiOutputPort) -> Option<MidiOutputConnection> {
        let output = midir::MidiOutput::new("MidiIo-out").unwrap();

        let port = output.ports().into_iter().find(|info| {
            output
                .port_name(info)
                .ok()
                .map(|name| name == port.0)
                .unwrap_or(false)
        });

        port.and_then(move |port| output.connect(&port, "MidiIo-in-conn").ok())
            .map(MidiOutputConnection)
    }
}

pub struct MidiInputManager {
    input: midir::MidiInput,
}

impl MidiInputManager {
    pub fn new() -> Result<Self, InitError> {
        let input = midir::MidiInput::new("MidiIo-in-manager")?;

        Ok(Self { input })
    }

    pub fn inputs(&self) -> Vec<MidiInputPort> {
        self.input
            .ports()
            .iter()
            .filter_map(|p| self.input.port_name(p).ok())
            .map(MidiInputPort)
            .collect()
    }

    pub fn connect_input<F>(
        port: MidiInputPort,
        mut callback: F,
    ) -> Option<(MidiInputPort, MidiInputConnection)>
    where
        F: FnMut(&[u8]) + Send + 'static,
    {
        let input = midir::MidiInput::new("MidiIo-in").unwrap();

        let midir_port = input.ports().into_iter().find(|info| {
            input
                .port_name(info)
                .ok()
                .map(|name| name == port.0)
                .unwrap_or(false)
        })?;

        Some((
            port.clone(),
            MidiInputConnection(
                input
                    .connect(
                        &midir_port,
                        "MidiIo-in-conn",
                        move |_, data, _| {
                            callback(data);
                            //
                        },
                        (),
                    )
                    .inspect_err(|err| log::error!("MIDI-in connection fail: {err}"))
                    .ok()?,
            ),
        ))
    }
}

#[allow(unused)]
pub struct MidiInputConnection(midir::MidiInputConnection<()>);
pub struct MidiOutputConnection(midir::MidiOutputConnection);

impl MidiOutputConnection {
    /// Send a message to the port that this output connection is connected to.
    /// The message must be a valid MIDI message (see https://www.midi.org/specifications-old/item/table-1-summary-of-midi-message).
    pub fn send(&mut self, message: &[u8]) -> Result<(), SendError> {
        self.0.send(message)?;
        Ok(())
    }
}

impl From<midir::SendError> for SendError {
    fn from(err: midir::SendError) -> Self {
        match err {
            midir::SendError::InvalidData(e) => Self::InvalidData(e),
            midir::SendError::Other(e) => Self::Other(e),
        }
    }
}
