#![allow(dead_code)]

extern crate rand;

pub mod grid_activations;
pub mod note_assigner;
pub mod rho_config;
pub mod step_switch;

use note_assigner::Note;
use note_assigner::NoteAssigner;
use rho_config::NUM_ROWS;

pub mod clock;
pub mod phasor;

pub mod looping_state;

pub type Rows = [looping_state::LoopingSequence<bool>; NUM_ROWS];

pub struct Rho {
    note_assigner: NoteAssigner,
    row_loopers: Rows,
    playing_notes: Vec<Note>,
}

impl Rho {
    pub fn new() -> Self {
        const DEFAULT_STEP_LEN: usize = 4;
        Rho {
            note_assigner: NoteAssigner::new(),
            row_loopers: Default::default(),
            playing_notes: vec![],
        }
    }
    pub fn note_on(&mut self, note: usize, velocity: usize) {
        self.note_assigner.note_on(note, velocity);
        self.note_assigner.print_row_notes();
    }

    pub fn note_off(&mut self, note: usize) {
        self.note_assigner.note_off(note);
        self.note_assigner.print_row_notes();
    }

    pub fn get_notes_for_rows(&self) -> [Vec<Note>; NUM_ROWS] {
        self.note_assigner.get_notes_for_rows()
    }

    pub fn on_clock_high(&mut self) -> Vec<note_assigner::Note> {
        // get the rows that are triggered by ticking the row loopers
        let triggered_rows = self.tick_rows();

        let notes_to_play = self.note_assigner.get_next_notes(triggered_rows);

        self.track_midi_notes(notes_to_play.clone());

        // keep track of the midi notes
        notes_to_play
    }

    pub fn on_clock_low(&mut self) -> Vec<note_assigner::Note> {
        // send note offs for all the notes
        let notes_to_stop = self.playing_notes.clone();
        self.playing_notes.clear();
        notes_to_stop
    }

    fn track_midi_notes(&mut self, notes: Vec<note_assigner::Note>) {
        for note in notes {
            self.playing_notes.push(note);
        }
    }

    fn tick_rows(&mut self) -> Vec<usize> {
        let mut triggered_rows = vec![];
        for i in 0..NUM_ROWS {
            if let Some(t) = self.row_loopers[i].next() {
                if t {
                    triggered_rows.push(i);
                }
            }
        }
        triggered_rows
    }
}
