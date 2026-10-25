use crate::note_assigner::Note;
use crate::note_assigner::NoteAssigner;
use crate::rho_config::{MAX_STEPS, NUM_ROWS};
use arrayvec::ArrayVec;

pub type RowSteps = ArrayVec<bool, MAX_STEPS>;

// The sequencer. Rows of different lengths loop against each other, the step to play is worked
// out from a global step index so it always lines up with the host's song position.
// Nothing in here allocates, so it is safe to use on the audio thread.
pub struct Rho {
    note_assigner: NoteAssigner,
    rows: [RowSteps; NUM_ROWS],
    current_step: Option<i64>,
}

impl Rho {
    pub fn new() -> Self {
        Rho {
            note_assigner: NoteAssigner::new(),
            rows: Default::default(),
            current_step: None,
        }
    }

    pub fn set_fill_octaves_enabled(&mut self, enabled: bool) {
        self.note_assigner.set_fill_octaves_enabled(enabled);
    }

    // set which steps are on for a row, anything past MAX_STEPS is dropped
    pub fn set_row(&mut self, row: usize, steps: &[bool]) {
        if row < NUM_ROWS {
            self.rows[row].clear();
            let n = steps.len().min(MAX_STEPS);
            // can't fail, we just checked the length
            let _ = self.rows[row].try_extend_from_slice(&steps[..n]);
        }
    }

    pub fn set_hold_notes_enabled(&mut self, enabled: bool) {
        self.note_assigner.set_hold_notes_enabled(enabled);
    }

    pub fn note_on(&mut self, note: usize, velocity: usize) {
        self.note_assigner.note_on(note, velocity);
    }

    pub fn note_off(&mut self, note: usize) {
        self.note_assigner.note_off(note);
    }

    // allocates, not for the audio thread
    pub fn get_notes_for_rows(&self) -> [Vec<Note>; NUM_ROWS] {
        self.note_assigner.get_notes_for_rows()
    }

    // play the step at this index, returns the notes to start
    pub fn on_step(&mut self, step_index: i64) -> ArrayVec<Note, NUM_ROWS> {
        self.current_step = Some(step_index);
        let triggered_rows = self.triggered_rows(step_index);
        self.note_assigner.get_next_notes(&triggered_rows)
    }

    // the step each row is on, for display
    pub fn get_playing_steps(&self) -> [Option<usize>; NUM_ROWS] {
        let mut steps = [None; NUM_ROWS];
        if let Some(step_index) = self.current_step {
            for (i, row) in self.rows.iter().enumerate() {
                if self.note_assigner.row_is_active(i) && !row.is_empty() {
                    steps[i] = Some(Self::row_step(step_index, row.len()));
                }
            }
        }
        steps
    }

    // forget the playing position, e.g. when the transport stops
    pub fn stop(&mut self) {
        self.current_step = None;
    }

    fn row_step(step_index: i64, row_length: usize) -> usize {
        step_index.rem_euclid(row_length as i64) as usize
    }

    fn triggered_rows(&self, step_index: i64) -> ArrayVec<usize, NUM_ROWS> {
        let mut triggered_rows = ArrayVec::new();
        for (i, row) in self.rows.iter().enumerate() {
            if !row.is_empty() && row[Self::row_step(step_index, row.len())] {
                triggered_rows.push(i);
            }
        }
        triggered_rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_on_step() {
        let mut rho = Rho::new();

        // set 4 note ons
        rho.note_on(0, 100);
        rho.note_on(1, 100);
        rho.note_on(2, 100);
        rho.note_on(3, 100);

        for row in 0..NUM_ROWS {
            rho.set_row(row, &[true, true]);
        }

        assert_eq!(rho.triggered_rows(0).as_slice(), &[0, 1, 2, 3]);

        let notes = rho.on_step(1);
        assert_eq!(notes.len(), 4);

        let playing_steps = rho.get_playing_steps();
        assert_eq!(playing_steps, [Some(1), Some(1), Some(1), Some(1)]);
    }

    #[test]
    fn rows_of_different_lengths_loop_independently() {
        let mut rho = Rho::new();
        rho.set_row(0, &[true, false]);
        rho.set_row(1, &[true, false, false]);

        let rows_at = |rho: &Rho, step| rho.triggered_rows(step).to_vec();
        assert_eq!(rows_at(&rho, 0), vec![0, 1]);
        assert_eq!(rows_at(&rho, 2), vec![0]);
        assert_eq!(rows_at(&rho, 3), vec![1]);
        assert_eq!(rows_at(&rho, 6), vec![0, 1]);
        // before the start of the song wraps the right way
        assert_eq!(rows_at(&rho, -3), vec![1]);
    }

    #[test]
    fn nothing_playing_before_first_step() {
        let mut rho = Rho::new();
        rho.set_row(0, &[true, false]);
        assert_eq!(rho.get_playing_steps(), [None; NUM_ROWS]);

        rho.on_step(3);
        assert_eq!(rho.get_playing_steps()[0], Some(1));

        rho.stop();
        assert_eq!(rho.get_playing_steps(), [None; NUM_ROWS]);
    }
}
