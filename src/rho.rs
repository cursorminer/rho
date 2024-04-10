extern crate rand;

use crate::looping_state;
use crate::note_assigner;
use crate::note_assigner::Note;
use crate::note_assigner::NoteAssigner;
use crate::rho_config::NUM_ROWS;

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

    pub fn set_fill_octaves_enabled(&mut self, enabled: bool) {
        self.note_assigner.set_fill_octaves_enabled(enabled);
    }

    pub fn set_row_activations(&mut self, row_activations: [Vec<bool>; NUM_ROWS]) {
        for i in 0..NUM_ROWS {
            // if the length changes, we need to resize the row looper
            if self.row_loopers[i].len() != row_activations[i].len() {
                self.row_loopers[i].resize(row_activations[i].len(), false);
            }
            // set each step
            for (j, active) in row_activations[i].iter().enumerate() {
                self.row_loopers[i].set_step(j, *active);
            }
        }
    }

    pub fn set_hold_notes_enabled(&mut self, enabled: bool) {
        self.note_assigner.set_hold_notes_enabled(enabled);
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

    pub fn get_playing_steps(&self) -> [Option<usize>; NUM_ROWS] {
        let mut steps = [None; NUM_ROWS];
        let mut i = 0;
        for row in self.row_loopers.iter() {
            if self.note_assigner.row_is_active(i) {
                steps[i] = Some(row.get_current_step());
            }
            i += 1;
        }
        steps
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

// todo test this
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_rows() {
        let mut rho = Rho::new();

        // set 4 note ons
        rho.note_on(0, 100);
        rho.note_on(1, 100);
        rho.note_on(2, 100);
        rho.note_on(3, 100);

        let two_true = looping_state::LoopingSequence::new(vec![true, true]);
        let rows = [
            two_true.clone(),
            two_true.clone(),
            two_true.clone(),
            two_true.clone(),
        ];

        rho.row_loopers = rows;

        let triggered_rows = rho.tick_rows();
        assert_eq!(triggered_rows, vec![0, 1, 2, 3]);

        let notes = rho.on_clock_high();
        assert_eq!(notes.len(), 4);

        let playing_steps = rho.get_playing_steps();
        assert_eq!(playing_steps, [Some(1), Some(1), Some(1), Some(1)]);
    }
}
