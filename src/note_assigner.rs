#![allow(dead_code)]

use crate::looping_state::LoopingSequence;
use crate::rho_config::NUM_ROWS;
use std::cmp::PartialOrd;
use std::fmt;

pub fn to_string(note: &Note) -> String {
    let note_class = note.note_number % 12;
    let note_name = match note_class {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        11 => "B",
        _ => "Unknown",
    };
    format!("{}{}", note_name, note.note_number / 12)
}

// a midi note
#[derive(Debug, Clone, Copy)]
pub struct Note {
    pub note_number: usize,
    pub velocity: usize,
}

impl PartialEq for Note {
    fn eq(&self, other: &Self) -> bool {
        self.note_number == other.note_number
    }
}

impl PartialOrd for Note {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.note_number.cmp(&other.note_number))
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", to_string(self))
    }
}

// How midi notes are assigned to rows
enum NoteOrdering {
    OldestFirst,
    LowestFirst,
}

// if held notes are pinned to rows, or if changes in held notes reassign rows dynamically
enum RowAssign {
    Dynamic,
    Hold,
}

enum NoteWrapping {
    None,
    Wrap,
    Fold,
    StackHigh,
    StackLow,
}

// data structure for a single row of the sequencer
// this could implement an iterator trait, and next does the right things...
#[derive(Debug)]
struct Row {
    active: bool,                 // is the row on or off
    notes: LoopingSequence<Note>, // the midi notes associated with the row
    rotation_counter: usize,      // which of notes to play next
}

impl Row {
    pub fn add_note(&mut self, note: Note) {
        self.notes.append(note);
    }

    pub fn tick(&mut self) -> Option<Note> {
        self.notes.next()
    }
}

impl Default for Row {
    fn default() -> Self {
        Row {
            active: true,
            notes: LoopingSequence::new(vec![]),
            rotation_counter: 0,
        }
    }
}

pub fn wrap(i: usize, max: usize) -> usize {
    if max == 0 {
        return 0;
    }
    i % (max + 1)
}

// folds over before end so that starts going down again
// note that it repeats the top note before descending:
//     00    00 max = 2
//    0  0  0
//   0    00
// i 0123456789
// because this sounds nice!
// does not work for i > 2 * max
pub fn fold_into_range(i: usize, max: usize) -> usize {
    if max == 0 {
        return 0;
    }
    let rep = (max + 1) * 2 - 1;
    let a = i % rep;

    if a <= max {
        a
    } else {
        rep - a
    }
}

pub fn stack_high(i: usize, max: usize) -> usize {
    if i > max {
        max
    } else {
        i
    }
}

pub fn stack_low(i: usize, max: usize) -> usize {
    if i > max {
        0
    } else {
        i
    }
}

// take the index of the note and return the index of the row it should be assigned to

fn map_note_index_to_row_index(
    note_index: usize,
    active_row_indices: &Vec<usize>,
    note_wrapping_mode: &NoteWrapping,
) -> Option<usize> {
    let active_row_indices = active_row_indices;
    let max_row = active_row_indices.len() - 1;

    let row_index = match note_wrapping_mode {
        NoteWrapping::Fold => Some(fold_into_range(note_index, max_row)),
        NoteWrapping::Wrap => Some(wrap(note_index, max_row)),
        NoteWrapping::StackHigh => Some(stack_high(note_index, max_row)),
        NoteWrapping::StackLow => Some(stack_low(note_index, max_row)),
        NoteWrapping::None => {
            if note_index < max_row {
                Some(note_index)
            } else {
                None
            }
        }
    };

    if let Some(r) = row_index {
        Some(active_row_indices[r])
    } else {
        None
    }
}

// This class keeps track of the active notes, assigns notes to rows, and handles which note comes next for a given row.
// Probably should be renamed to reflect that fact...
pub struct NoteAssigner {
    active_notes: Vec<Option<Note>>, // the none state means that we have an empty row but others are pinned above it
    rows: [Row; NUM_ROWS],
    note_ordering_mode: NoteOrdering,
    note_wrapping_mode: NoteWrapping,

    hold_notes_enabled: bool,
    auto_octave_enabled: bool,
    invert_rows_enabled: bool,
}

impl NoteAssigner {
    pub fn new() -> Self {
        let rows_vec: Vec<Row> = (0..4).map(|_| Row::default()).collect();
        let rows_array: [Row; 4] = rows_vec.try_into().unwrap();
        NoteAssigner {
            active_notes: vec![],
            rows: rows_array,
            note_ordering_mode: NoteOrdering::LowestFirst,
            note_wrapping_mode: NoteWrapping::Fold,
            hold_notes_enabled: false,
            auto_octave_enabled: false,
            invert_rows_enabled: false,
        }
    }

    // given which rows are active and have assigned notes, return the notes
    pub fn get_next_notes(&mut self, triggered_rows: Vec<usize>) -> Vec<Note> {
        // tick all the rows that are active and appear in triggered rows
        let mut notes = vec![];
        for row_index in triggered_rows {
            if self.row_has_note_and_active(row_index) {
                if let Some(note) = self.rows[row_index].tick() {
                    notes.push(note);
                }
            }
        }
        notes
    }

    pub fn note_on(&mut self, note_number: usize, velocity: usize) {
        let new_note = Note {
            note_number,
            velocity,
        };

        if !self.fill_empty_note_if_available(new_note) {
            match self.note_ordering_mode {
                NoteOrdering::LowestFirst => {
                    let pos = self
                        .active_notes
                        .iter()
                        .position(|x| x.map_or(false, |x| x > new_note));

                    if let Some(pos) = pos {
                        self.active_notes.insert(pos, Some(new_note));
                    } else {
                        self.active_notes.push(Some(new_note));
                    }
                }
                NoteOrdering::OldestFirst => {
                    self.active_notes.push(Some(new_note));
                }
            }
        }

        self.update_note_to_row_mapping();
    }

    pub fn note_off(&mut self, note_number: usize) {
        // find the note number and remove it, assume there could be more than one

        if self.hold_notes_enabled {
            self.active_notes.iter_mut().for_each(|note| {
                if note
                    .as_ref()
                    .map_or(false, |n| n.note_number == note_number)
                {
                    *note = None;
                }
            });

            if self.all_active_notes_empty() {
                self.active_notes.clear();
            }
        } else {
            // retain only those where the note number does not match, retain none notes too
            self.active_notes
                .retain(|note| note.map_or(true, |n| n.note_number != note_number));
        }

        self.update_note_to_row_mapping();
    }

    pub fn set_hold_notes_enabled(&mut self, enabled: bool) {
        self.hold_notes_enabled = enabled;
    }

    pub fn all_active_notes_empty(&self) -> bool {
        self.active_notes.iter().all(Option::is_none)
    }

    pub fn row_has_note_and_active(&self, index: usize) -> bool {
        index < NUM_ROWS && self.rows[index].active && self.rows[index].notes.len() > 0
    }

    pub fn set_row_active(&mut self, row_number: usize, active: bool) {
        if row_number < NUM_ROWS {
            self.rows[row_number].active = active;
        }
    }

    pub fn clear_all_note_assignments(&mut self) {
        self.rows.iter_mut().for_each(|row| row.notes.clear());
    }

    // returns a vector of indices for the active notes (will always be in ascending order)
    pub fn active_row_indices(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.active)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn num_active_rows(&self) -> usize {
        self.active_row_indices().len()
    }

    pub fn row_is_active(&self, index: usize) -> bool {
        index < NUM_ROWS && self.rows[index].active
    }

    // "private" stuff

    // reverses the order of indices, but only for active rows
    // e.g. if row 1 & 2 are active then 2 becomes 1 and 1 becomes 2, 3 & 4 are left alone
    fn invert_active_row_index(&self, index: usize) -> usize {
        *self
            .active_row_indices()
            .iter()
            .rev()
            .nth(index)
            .unwrap_or(&index)
    }

    // try to find an unassigned row to assign a note to, if can't return false
    // this active note thing sucks...
    fn fill_empty_note_if_available(&mut self, note: Note) -> bool {
        // todo there could be multiple empty rows, in which case we should respect the NoteOrdering
        // perhaps
        let pos = self.active_notes.iter().position(|n| match n {
            None => true,
            Some(_) => false,
        });

        // if Some(pos) then we found an empty slot

        match pos {
            Some(pos) => {
                self.active_notes[pos] = Some(note);
                return true;
            }
            None => {
                return false;
            }
        }
    }
    pub fn wrap_notes_enabled(&self) -> bool {
        match self.note_wrapping_mode {
            NoteWrapping::None => false,
            _ => true,
        }
    }

    // when anything changes, reassign the notes to the rows
    fn update_note_to_row_mapping(&mut self) {
        self.clear_all_note_assignments();

        // make a copy of active notes, because we can't borrow self.active_notes to change self.rows
        let active_notes = self.active_notes.clone();
        // loop over active notes
        // get the row index that the note will be assigned to
        // copy the note in
        active_notes
            .iter()
            .enumerate()
            .for_each(|(note_index, note)| {
                let row_index = map_note_index_to_row_index(
                    note_index,
                    &self.active_row_indices(),
                    &self.note_wrapping_mode,
                );
                if let Some(r) = row_index {
                    if let Some(n) = note {
                        self.rows[r].add_note(*n);
                    }
                }
            });

        if self.auto_octave_enabled {
            self.fill_remaining_rows_with_octaves();
        }

        //self.wrap_note_rotation_counters();
    }

    pub fn set_fill_octaves_enabled(&mut self, enabled: bool) {
        self.auto_octave_enabled = enabled;
    }

    // this fills any notes that don't have notes with octaves above the ones that do
    fn fill_remaining_rows_with_octaves(&mut self) {
        // TODO
    }

    fn get_octave_shifted_note_for_index(&self, row_index: usize) -> Note {
        // TODO this mod might be wrong or weird if rows are deactivated
        if self.active_notes.is_empty() {
            return Note {
                note_number: 0,
                velocity: 0,
            };
        }
        let note_to_repeat = self.active_notes[row_index % self.active_notes.len()];
        let octave = row_index / self.active_notes.len();

        Note {
            note_number: note_to_repeat.unwrap().note_number + 12 * octave,
            velocity: note_to_repeat.unwrap().velocity,
        }
    }

    // return an array of the notes assigned to each row
    const EMPTY_VEC: Vec<Note> = Vec::new();
    pub fn get_notes_for_rows(&self) -> [Vec<Note>; NUM_ROWS] {
        let mut notes: [Vec<Note>; NUM_ROWS] = Default::default();
        for (i, row) in self.rows.iter().enumerate() {
            notes[i] = row.notes.clone_data();
        }
        notes
    }

    pub fn print_row_notes(&self) {
        print!("[");
        let mut i = 0;
        for row in self.get_notes_for_rows() {
            print!("[{} ", i);
            for note in row {
                print!("{}, ", note);
            }
            i += 1;
            print!("] ");
        }
        print!("]\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_wraps() {
        assert_eq!(wrap(5, 3), 1);
        assert_eq!(wrap(5, 4), 0);
    }

    #[test]
    fn test_note_assigner_note_on_off() {
        let mut ga = NoteAssigner::new();
        assert_eq!(ga.active_notes, vec![]);
        assert!(!ga.row_has_note_and_active(0));

        let note = Note {
            note_number: 69,
            velocity: 100,
        };
        let note2 = Note {
            note_number: 70,
            velocity: 100,
        };

        // one note on
        ga.note_on(69, 100);

        assert_eq!(ga.active_notes.len(), 1);
        assert_eq!(ga.active_notes[0], Some(note));

        // a note on that's higher than previous goes at end
        ga.note_on(70, 100);

        // two active notes
        assert_eq!(ga.active_notes.len(), 2);
        assert_eq!(ga.active_notes[0], Some(note));
        assert_eq!(ga.active_notes[1], Some(note2));

        ga.note_off(69);
        assert_eq!(ga.active_notes.len(), 1);
        assert_eq!(ga.active_notes[0], Some(note2));

        ga.note_off(70);
        assert!(ga.active_notes.is_empty());

        ga.hold_notes_enabled = true;

        ga.note_on(69, 100);

        assert_eq!(ga.active_notes.len(), 1);
        assert_eq!(ga.active_notes[0], Some(note));

        // a note on that's higher than previous goes at end
        ga.note_on(70, 100);

        ga.note_off(69);
        assert_eq!(ga.active_notes.len(), 2);
        assert_eq!(ga.active_notes[0], None);
        assert_eq!(ga.active_notes[1], Some(note2));

        ga.note_off(70);
        assert!(ga.active_notes.is_empty());
    }

    #[test]
    fn test_note_assigner_note_row_mapping() {
        let mut ga = NoteAssigner::new();
        assert_eq!(ga.active_notes, vec![]);
        assert!(!ga.row_has_note_and_active(0));

        let note1 = Note {
            note_number: 69,
            velocity: 100,
        };
        let note2 = Note {
            note_number: 70,
            velocity: 100,
        };
        let note3 = Note {
            note_number: 71,
            velocity: 100,
        };

        // one note on
        ga.note_on(69, 100);
        // a note on that's higher than previous goes at end
        ga.note_on(70, 100);

        // expect that they are mapped to the first two rows
        assert_eq!(ga.rows[0].notes.clone_data()[0].note_number, 69);
        assert_eq!(ga.rows[1].notes.clone_data()[0].note_number, 70);

        // turn the top two rows off
        ga.set_row_active(2, false);
        ga.set_row_active(3, false);

        // adding another note should fold
        ga.note_on(71, 100);

        assert_eq!(ga.rows[0].notes.len(), 1);
        assert_eq!(ga.rows[0].notes.clone_data(), vec![note1]);
        assert_eq!(ga.rows[1].notes.len(), 2);
        assert_eq!(ga.rows[1].notes.clone_data(), vec![note2, note3]);
        // nothing mapped to next two rows
        assert!(ga.rows[2].notes.clone_data().is_empty());

        assert_eq!(ga.invert_active_row_index(1), 0);
        assert_eq!(ga.invert_active_row_index(0), 1);

        // trigger rows 1,2,3
        let notes = ga.get_next_notes(vec![0, 1, 2, 3]);

        // expect the notes from the first two rows
        assert_eq!(notes, vec![note1, note2]);

        ga.print_row_notes();
    }

    #[test]
    fn test_note_assigner_row_active() {
        let mut ga = NoteAssigner::new();

        ga.set_row_active(0, false);
        ga.set_row_active(1, true);
        ga.set_row_active(2, false);
        ga.set_row_active(3, true);

        assert_eq!(ga.num_active_rows(), 2);
        assert_eq!(ga.active_row_indices(), vec![1, 3]);

        ga.set_row_active(3, false);

        assert_eq!(ga.num_active_rows(), 1);
        assert_eq!(ga.active_row_indices(), vec![1]);
    }

    // #[test]
    // fn test_fill_octaves() {
    //     let mut ga = NoteAssigner::new();

    //     ga.note_on(60, 100);
    //     ga.fill_remaining_rows_with_octaves();

    //     assert_eq!(ga.rows[0].notes.clone_data()[0].note_number, 60);
    //     assert_eq!(ga.rows[1].notes.clone_data()[0].note_number, 72);
    //     assert_eq!(ga.rows[2].notes.clone_data()[0].note_number, 84);
    //     assert_eq!(ga.rows[3].notes.clone_data()[0].note_number, 96);
    // }
}
