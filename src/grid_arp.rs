use std::cmp::PartialOrd;

// a midi note
#[derive(Debug, Clone, Copy)]
struct Note {
    note_number: usize,
    velocity: usize,
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
    active: bool,            // is the row on or off
    notes: Vec<Note>,        // the midi notes associated with the row
    rotation_counter: usize, // which of notes to play next
}

impl Default for Row {
    fn default() -> Self {
        Row {
            active: true,
            notes: vec![],
            rotation_counter: 0,
        }
    }
}

pub fn increment_and_wrap(i: usize, wrap_before: usize) -> usize {
    if i + 1 >= wrap_before {
        0
    } else {
        i + 1
    }
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

const NUM_ROWS: usize = 4;

// This class keeps track of the active notes, assigns notes to rows, and handles which note comes next for a given row.
// Probably should be renamed to reflect that fact...
struct GridArp {
    active_notes: Vec<Option<Note>>, // the none state means that we have an empty row but others are pinned above it
    rows: [Row; NUM_ROWS],
    note_ordering_mode: NoteOrdering,
    note_wrapping_mode: NoteWrapping,

    hold_notes_enabled: bool,
    auto_octave_enabled: bool,
    invert_rows_enabled: bool,
}

impl GridArp {
    pub fn new() -> Self {
        let rows_vec: Vec<Row> = (0..4).map(|_| Row::default()).collect();
        let rows_array: [Row; 4] = rows_vec.try_into().unwrap();
        GridArp {
            active_notes: vec![],
            rows: rows_array,
            note_ordering_mode: NoteOrdering::LowestFirst,
            note_wrapping_mode: NoteWrapping::Fold,
            hold_notes_enabled: false,
            auto_octave_enabled: false,
            invert_rows_enabled: false,
        }
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

    // "private" stuff
    fn invert_active_row_index(index: usize) {}

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

    // when anything changes, reassign the notes to the rows
    fn update_note_to_row_mapping(&mut self) {
        self.clear_all_note_assignments();

        // loop through the active rows assigning notes this depends on the various modes
        let mut note_index = 0;
        let mut row_index = 0;
        let num_notes = self.active_notes.len();

        if num_notes == 0 || self.num_active_rows() == 0 {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_arp_note_on_off() {
        let mut ga = GridArp::new();
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
    fn test_grid_arp_note_row_mapping() {
        let mut ga = GridArp::new();
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
        // a note on that's higher than previous goes at end
        ga.note_on(70, 100);

        // expect that they are mapped to the first two rows
    }

    fn test_grid_arp_row_active() {
        let mut ga = GridArp::new();
        ga.set_row_active(1, true);
        assert_eq!(ga.num_active_rows(), 1);
        assert_eq!(ga.active_row_indices(), vec![1]);
        assert_eq!(ga.num_active_rows(), 2);
        assert_eq!(ga.active_row_indices(), vec![1, 3]);
    }
}
