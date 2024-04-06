// inter thread messages

use crate::note_assigner::Note;
use crate::rho_config::NUM_ROWS;

pub const NOTE_ON_MSG: u8 = 0x90;
pub const NOTE_OFF_MSG: u8 = 0x80;

// when notes are recieved, we send them to the rho sequencer via a channel
pub enum MidiInMessage {
    NoteOn(u8, u8),
    NoteOff(u8),
}

// messages from the clock to the gui, to display the state of the sequencer
pub enum MessageToGui {
    NotesForRows {
        notes: [Vec<Note>; NUM_ROWS],
    },
    Tick {
        playing_steps: [Option<usize>; NUM_ROWS],
    },
}

// messages from the gui to the rho sequencer (clock thread). send when the row activations change
pub enum MessageGuiToRho {
    RowActivations {
        row_activations: [Vec<bool>; NUM_ROWS],
    },
    HoldNotesEnabled {
        enabled: bool,
    },
    SetMidiInPort {
        port: usize,
    },
    SetMidiOutPort {
        port: usize,
    },
    SetMidiChannelIn {
        channel: u8,
    },
    SetMidiChannelOut {
        channel: u8,
    },
    SetPlaying {
        playing: bool,
    },
    SetTempo {
        tempo: f32,
    },
}
