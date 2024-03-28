#![allow(dead_code)]

extern crate rand;

pub mod grid_activations;
use grid_activations::GridActivations;

pub mod note_assigner;
use note_assigner::Note;
use note_assigner::NoteAssigner;
use note_assigner::NUM_ROWS;

pub mod clock;
pub mod phasor;

pub mod looping_state;

pub type Rows = [looping_state::LoopingSequence<bool>; NUM_ROWS];

pub struct Rho {
    grid_activations: GridActivations,
    note_assigner: NoteAssigner,
    // todo: we have this, but we also have the row_length in the grid_activations ,
    //which is a bit redundant and the states could become out of sync
    row_loopers: Rows,
    playing_notes: Vec<Note>,
}

impl Rho {
    pub fn set_density(&mut self, density: f32) {
        self.grid_activations.set_normalized_density(density);
        self.update_row_looper_from_grid();
    }
    pub fn get_density(&self) -> f32 {
        self.grid_activations.get_density()
    }

    pub fn set_row_length(&mut self, row: usize, length: usize) {
        if row >= NUM_ROWS || length < 1 {
            return;
        }
        self.grid_activations.set_row_length(row, length);
        self.row_loopers[row].data.resize(length, false);
        self.update_row_looper_from_grid();
    }

    pub fn new() -> Self {
        const DEFAULT_STEP_LEN: usize = 4;
        Rho {
            grid_activations: GridActivations::new(NUM_ROWS, DEFAULT_STEP_LEN),
            note_assigner: NoteAssigner::new(),
            row_loopers: Default::default(),
            playing_notes: vec![],
        }
    }
    pub fn note_on(&mut self, note: usize, velocity: usize) {
        self.note_assigner.note_on(note, velocity);
        self.note_assigner.print_row_notes();
        self.update_row_looper_from_grid();
    }

    pub fn note_off(&mut self, note: usize) {
        self.note_assigner.note_off(note);
        self.note_assigner.print_row_notes();
        self.update_row_looper_from_grid();
    }

    pub fn on_clock_high(&mut self) -> Vec<note_assigner::Note> {
        self.update_row_looper_from_grid(); // maybe should happen from UI listeners

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

    fn update_row_looper_from_grid(&mut self) {
        // surely a nicer way to do this
        for row in 0..NUM_ROWS {
            let row_length = self.grid_activations.get_row_length(row);
            self.row_loopers[row].resize(row_length, false);
            self.row_loopers[row].set_data(self.grid_activations.get_row(row));
        }
    }
}
