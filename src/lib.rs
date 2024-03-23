#![allow(dead_code)]

extern crate rand;

pub mod grid_activations;
use grid_activations::GridActivations;

pub mod note_assigner;
use note_assigner::NoteAssigner;
use note_assigner::NUM_ROWS;

pub mod looping_state;

struct Rho {
    grid_activations: GridActivations,
    note_assigner: NoteAssigner,
    row_counter: [looping_state::LoopingSequence<bool>; NUM_ROWS],
}

impl Rho {
    fn note_on(&mut self) {
        self.note_assigner.note_on(0, 0);
    }

    fn on_clock_high(&mut self) {

        //let triggered_rows = self.grid_activations.tick_rows();

        // let notes_to_play = self.note_assigner.get_next_notes(triggered_rows);

        //self.play_midi_notes(notes_to_play);
    }

    fn play_midi_notes(&self, _notes: Vec<note_assigner::Note>) {}
}
