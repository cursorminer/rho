use crate::grid_activations::GridActivations;
use crate::note_assigner::Note;
use crate::rho::RowSteps;
use crate::rho_config::{MAX_NOTES, MAX_STEPS, NUM_ROWS};
use arrayvec::ArrayVec;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;

pub type RowNotes = ArrayVec<Note, MAX_NOTES>;

const NO_STEP: u8 = u8::MAX;

// State passed between the editor and the audio thread. The audio thread only touches atomics
// and try_lock, so it never waits on the editor.
pub struct SharedState {
    // the rows as edited in the gui, packed with pack_row
    rows: [AtomicU32; NUM_ROWS],
    // the step each row is on, for display
    playing_steps: [AtomicU8; NUM_ROWS],
    // the notes assigned to each row, for display
    notes_for_rows: Mutex<[RowNotes; NUM_ROWS]>,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            rows: Default::default(),
            playing_steps: std::array::from_fn(|_| AtomicU8::new(NO_STEP)),
            notes_for_rows: Default::default(),
        }
    }
}

impl SharedState {
    // gui side: send the grid's rows to the audio thread
    pub fn publish_rows(&self, grid: &GridActivations) {
        for (row, packed) in self.rows.iter().enumerate() {
            packed.store(pack_row(&grid.get_row(row)), Ordering::Relaxed);
        }
    }

    // audio side: the packed row, compare with the last one seen to spot changes
    pub fn packed_row(&self, row: usize) -> u32 {
        self.rows[row].load(Ordering::Relaxed)
    }

    pub fn set_playing_steps(&self, steps: [Option<usize>; NUM_ROWS]) {
        for (step, playing) in steps.iter().zip(&self.playing_steps) {
            playing.store(step.map_or(NO_STEP, |s| s as u8), Ordering::Relaxed);
        }
    }

    pub fn playing_steps(&self) -> [Option<usize>; NUM_ROWS] {
        std::array::from_fn(
            |row| match self.playing_steps[row].load(Ordering::Relaxed) {
                NO_STEP => None,
                step => Some(step as usize),
            },
        )
    }

    // audio side: skipped if the gui is reading, it'll be updated next block
    pub fn try_set_notes_for_rows<'a>(&self, notes: impl Fn(usize) -> &'a [Note]) {
        if let Ok(mut notes_for_rows) = self.notes_for_rows.try_lock() {
            for (row, row_notes) in notes_for_rows.iter_mut().enumerate() {
                row_notes.clear();
                let _ = row_notes.try_extend_from_slice(notes(row));
            }
        }
    }

    pub fn notes_for_rows(&self) -> [RowNotes; NUM_ROWS] {
        self.notes_for_rows.lock().unwrap().clone()
    }
}

// a row fits in one atomic: the length in the top 16 bits, a bit per step in the bottom 16
pub fn pack_row(steps: &[bool]) -> u32 {
    let len = steps.len().min(MAX_STEPS);
    let bits = steps[..len]
        .iter()
        .enumerate()
        .fold(0, |bits, (i, &on)| bits | ((on as u32) << i));
    ((len as u32) << 16) | bits
}

pub fn unpack_row(packed: u32) -> RowSteps {
    let len = ((packed >> 16) as usize).min(MAX_STEPS);
    (0..len).map(|i| packed & (1 << i) != 0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_survive_packing() {
        let row = [true, false, false, true, true];
        assert_eq!(unpack_row(pack_row(&row)).as_slice(), &row);
        assert!(unpack_row(pack_row(&[])).is_empty());

        let long = [true; MAX_STEPS];
        assert_eq!(unpack_row(pack_row(&long)).as_slice(), &long);
    }

    #[test]
    fn rows_of_different_lengths_pack_differently() {
        assert_ne!(pack_row(&[true, false]), pack_row(&[true, false, false]));
    }

    #[test]
    fn playing_steps_round_trip() {
        let shared = SharedState::default();
        assert_eq!(shared.playing_steps(), [None; NUM_ROWS]);

        let steps = [Some(0), None, Some(7), Some(3)];
        shared.set_playing_steps(steps);
        assert_eq!(shared.playing_steps(), steps);
    }

    #[test]
    fn publishing_a_grid_packs_each_row() {
        let mut grid = GridActivations::new(NUM_ROWS, 2);
        grid.set_row_length(2, 5);
        grid.set(2, 4, true);
        grid.set(0, 1, true);

        let shared = SharedState::default();
        shared.publish_rows(&grid);
        for row in 0..NUM_ROWS {
            assert_eq!(
                unpack_row(shared.packed_row(row)).to_vec(),
                grid.get_row(row)
            );
        }
    }

    fn note(note_number: usize) -> Note {
        Note {
            note_number,
            velocity: 100,
        }
    }

    #[test]
    fn notes_for_rows_round_trip() {
        let shared = SharedState::default();
        let rows = [vec![note(60), note(72)], vec![note(64)], vec![], vec![]];
        shared.try_set_notes_for_rows(|row| &rows[row]);

        let notes = shared.notes_for_rows();
        for row in 0..NUM_ROWS {
            assert_eq!(notes[row].to_vec(), rows[row]);
        }
    }

    #[test]
    fn notes_update_is_skipped_while_the_gui_reads() {
        let shared = SharedState::default();
        let rows = [vec![note(60)], vec![], vec![], vec![]];

        let gui_lock = shared.notes_for_rows.lock().unwrap();
        // must return rather than wait
        shared.try_set_notes_for_rows(|row| &rows[row]);
        drop(gui_lock);
        assert!(shared.notes_for_rows()[0].is_empty());

        shared.try_set_notes_for_rows(|row| &rows[row]);
        assert_eq!(shared.notes_for_rows()[0].to_vec(), rows[0]);
    }
}
