#![allow(dead_code)]

extern crate rand;

use crate::rand::prelude::SliceRandom;
use rand::thread_rng;

use crate::rho_config::NUM_ROWS;

//--------------------------------------------------------------------------------
// TODOs
//
// put free functions in a different module
// understand how lib, modules, crates and tests should be organized
// add the RowAssigner type and impl

// todo this belongs somewhere else

// flatten a bunch of row sequences into one single sequence
pub fn flatten(v: Vec<Vec<usize>>) -> Vec<usize> {
    let mut result: Vec<usize> = Vec::new();
    for x in v {
        result.extend(x);
    }
    result
}

// todo this could be generic?
pub fn unflatten(flat: &Vec<usize>, row_lengths: &Vec<usize>) -> Vec<Vec<usize>> {
    let mut grid: Vec<Vec<usize>> = Vec::new();
    debug_assert!(row_lengths.iter().sum::<usize>() == flat.len());

    let mut i_f = 0;
    for len in row_lengths {
        grid.push(flat[i_f..i_f + len].to_vec());
        i_f += len;
    }

    grid
}

// create a new bunch of thresholds
pub fn create_new_distribution(n: usize) -> Vec<usize> {
    // vec has ascending integers 0-N
    // then shuffle it randomly
    let mut v = Vec::from_iter(0..n);
    let mut rng = thread_rng();
    v.shuffle(&mut rng);
    v
}

pub fn flat_index_to_grid_index(flat_index: usize, row_lengths: &Vec<usize>) -> (usize, usize) {
    let mut row_index = 0;
    let mut step_index = 0;
    let mut row_start = 0;
    let mut row_end = 0;

    for len in row_lengths {
        row_end += len;
        if flat_index >= row_start && flat_index < row_end {
            step_index = flat_index - row_start;
            break;
        }
        row_index += 1;
        row_start += len;
    }
    debug_assert!(row_index < row_lengths.len());
    debug_assert!(step_index < row_lengths[row_index]);

    (row_index, step_index)
}

pub fn grid_index_to_flat_index(grid_index: (usize, usize), row_lengths: &Vec<usize>) -> usize {
    debug_assert!(grid_index.0 <= row_lengths.len());
    // sum all the row lenghts up to our row
    let steps_up_to_this_row = row_lengths[0..grid_index.0].iter().sum::<usize>();

    let flat = steps_up_to_this_row + grid_index.1;

    // allow an index just off end
    debug_assert!(flat <= row_lengths.iter().sum());
    return flat;
}

pub struct GridActivations {
    active: Vec<bool>,
    thresh: Vec<usize>,
    row_lengths: Vec<usize>,
    // these suck because they both interdepend on the steps
    normalized_density: f32,
}

impl GridActivations {
    pub fn new(rows: usize, steps: usize) -> Self {
        let total_steps = steps * rows;
        GridActivations {
            active: vec![false; total_steps],
            thresh: create_new_distribution(total_steps),
            row_lengths: vec![steps; rows],
            normalized_density: 0.0,
        }
    }

    pub fn get_total_num_steps(&self) -> usize {
        self.row_lengths.iter().sum()
    }

    pub fn set_normalized_density(&mut self, density: f32) {
        print!("set_normalized_density {}\n", density);
        self.normalized_density = density;
        let wanted_num_active_steps = (density * self.get_total_num_steps() as f32) as usize;

        if self.num_active_steps() != wanted_num_active_steps {
            self.set_activations_for_new_density(wanted_num_active_steps);
        }
    }

    pub fn get_normalized_density(&self) -> f32 {
        self.normalized_density
    }

    pub fn set_row_length(&mut self, row_index: usize, new_length: usize) {
        if new_length > self.row_lengths[row_index] {
            self.append_steps(row_index, new_length);
        } else if new_length < self.row_lengths[row_index] {
            self.remove_steps(row_index, new_length);
        }
    }

    // When the density is changed, the active steps change according to their threshold
    pub fn set_activations_for_new_density(&mut self, density: usize) {
        for i in 0..self.active.len() {
            self.active[i] = self.thresh[i] < density;
        }
    }

    pub fn get_row(&self, index: usize) -> Vec<bool> {
        let start = self.row_lengths[0..index].iter().sum();
        let end = start + self.row_lengths[index];
        self.active[start..end].to_vec()
    }

    pub fn get_row_activations(&self) -> [Vec<bool>; NUM_ROWS] {
        let mut result: [Vec<bool>; NUM_ROWS] = Default::default();
        for i in 0..self.row_lengths.len() {
            result[i] = self.get_row(i);
        }
        result
    }

    fn num_active_steps(&self) -> usize {
        self.active
            .iter()
            .fold(0, |acc, x| if *x { acc + 1 } else { acc })
    }

    pub fn set(&mut self, row: usize, step: usize, on: bool) {
        let flat_index = grid_index_to_flat_index((row, step), &self.row_lengths);
        self.change_step_update_thresholds(flat_index, on);
    }

    // switch a step on or off
    //  adjust distribution  whilst respecting the changed step (step at index)
    // if something changed, returns true
    fn change_step_update_thresholds(&mut self, step_index: usize, on: bool) -> bool {
        if self.active[step_index] == on {
            return false;
        }

        self.active[step_index] = on;

        //  find the index of the step that would have changed as a result of the new density
        // and swap thresholds of the step we want to change with that
        let density = if on {
            self.num_active_steps() - 1
        } else {
            self.num_active_steps()
        };

        let i = self.thresh.iter().position(|&x| x == density).unwrap();

        self.thresh.swap(step_index, i);
        self.update_density();
        true
    }

    // a new random distribution, generate thresholds where only the provided steps exceed the threshold.
    // The threshold is returned as a density
    // the use case is, you have a nice sequence but you want a new way to randomise it.
    // it is conveneient to reuse change step but something about this function really sucks
    pub fn create_new_distribution_given_active_steps(&mut self) {
        let original_active = self.active.clone();
        let n = self.active.len();

        // set all to false
        self.active = vec![false; n];

        // randomise thresholds
        self.thresh = create_new_distribution(n);

        // now make sure lowest thresholds correspond to active steps activating steps one by
        // one
        // need to randomise order to avoid consecutive thresholds
        let random_order = create_new_distribution(n);

        for i in random_order {
            self.change_step_update_thresholds(i, original_active[i]);
        }
        debug_assert!(original_active == self.active);
    }

    // appending a new step to the end of a row will change the steps arrays, the thresh arrays etc.
    // the new step is always inactive
    pub fn append_steps(&mut self, row_to_append: usize, new_length: usize) {
        let num_to_insert = new_length - self.row_lengths[row_to_append];

        // we need to insert the thresholds that do not exist yet, they're always the biggest
        // (should they be? yes, because we want to preseve the patterns in the other bit)

        let old_flat_length = self.row_lengths.iter().sum::<usize>();

        let mut thresh_to_insert: Vec<_> =
            (old_flat_length..old_flat_length + num_to_insert).collect();

        let mut rng = thread_rng();
        thresh_to_insert.shuffle(&mut rng);

        let active_to_insert = vec![false; num_to_insert];

        let insert_position = grid_index_to_flat_index((row_to_append + 1, 0), &self.row_lengths);

        self.active
            .splice(insert_position..insert_position, active_to_insert);
        self.thresh
            .splice(insert_position..insert_position, thresh_to_insert);

        // @todo is there some nice way to assert this always happens for any mutation
        debug_assert!(self.active.len() == self.thresh.len());

        self.row_lengths[row_to_append] = new_length;
        self.update_density();
    }

    pub fn remove_steps(&mut self, row_to_remove_from: usize, new_length: usize) {
        debug_assert!(new_length < self.row_lengths[row_to_remove_from]);
        let num_to_remove = self.row_lengths[row_to_remove_from] - new_length;

        // remove last one from each row, scaling the bigger thresholds as we go
        for _i in 0..num_to_remove {
            let last_in_row = self.row_lengths[row_to_remove_from] - 1;
            let remove_position =
                grid_index_to_flat_index((row_to_remove_from, last_in_row), &self.row_lengths);
            let removed_threshold = self.thresh[remove_position];

            // erase the active step and the thresh at that point
            self.thresh.remove(remove_position);
            self.active.remove(remove_position);

            // all the thresholds higher than the removed one need to be reduced by one
            self.thresh.iter_mut().for_each(|x| {
                if *x > removed_threshold {
                    *x -= 1;
                }
            });

            self.row_lengths[row_to_remove_from] -= 1;
        }
        self.update_density();
    }

    pub fn get_row_length(&self, row: usize) -> usize {
        self.row_lengths[row]
    }

    pub fn get(&self, row: usize, step: usize) -> bool {
        debug_assert!(row < self.row_lengths.len());
        debug_assert!(step < self.row_lengths[row]);
        self.active[grid_index_to_flat_index((row, step), &self.row_lengths)]
    }

    pub fn row_length(&self, row: usize) -> usize {
        self.row_lengths[row]
    }

    // the steps have changed so change the density
    pub fn update_density(&mut self) {
        if self.get_total_num_steps() == 0 {
            self.normalized_density = 0.0;
        }
        self.normalized_density =
            self.num_active_steps() as f32 / self.get_total_num_steps() as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_create_new_distribution() {
        assert_eq!(create_new_distribution(5).len(), 5);
    }

    #[test]
    fn test_set_activations_for_new_density() {
        let mut seq = GridActivations {
            active: vec![false, false, false, false, false],
            thresh: vec![0, 1, 2, 4, 3],
            row_lengths: vec![1, 2, 3],
            normalized_density: 0.0,
        };

        seq.set_activations_for_new_density(0);
        assert_eq!(seq.active, vec![false, false, false, false, false]);

        seq.set_activations_for_new_density(4);
        assert_eq!(seq.active, vec![true, true, true, false, true]);

        seq.set_activations_for_new_density(5);
        assert_eq!(seq.active, vec![true, true, true, true, true]);
    }
    #[test]
    fn test_num_active_steps() {
        let mut seq = GridActivations {
            active: vec![false, true, false, false, true],
            thresh: vec![0, 1, 2, 4, 3],
            row_lengths: vec![1, 2, 3],
            normalized_density: 0.0,
        };

        assert_eq!(seq.num_active_steps(), 2);
        seq.active = vec![false, true, false, true, true];
        assert_eq!(seq.num_active_steps(), 3);
    }

    #[test]
    fn test_change_step() {
        let mut seq = GridActivations {
            active: vec![false, false, false, false, false],
            thresh: vec![0, 1, 2, 3, 4],
            row_lengths: vec![1, 2, 3],
            normalized_density: 0.0,
        };

        let density: usize = 1;

        // smallest density only has one active step
        seq.set_activations_for_new_density(density);
        assert_eq!(seq.active, vec![true, false, false, false, false]);

        // now set step 4 to active
        seq.change_step_update_thresholds(4, true);

        //expect that step 4 will get thresh of 1 and density qill be 2
        assert_eq!(seq.active, vec![true, false, false, false, true]);
        assert_eq!(seq.thresh, vec![0, 4, 2, 3, 1]);

        // turn off step 0
        seq.change_step_update_thresholds(0, false);
        // expect that step 0 will be turned off
        assert_eq!(seq.active, vec![false, false, false, false, true]);
        // and the will be set to 1, swapped with the last density 0
        assert_eq!(seq.thresh, vec![1, 4, 2, 3, 0]);
    }

    #[test]
    fn test_create_new_distribution_given_active_steps() {
        let mut seq = GridActivations {
            active: vec![false, true, false, false, true],
            thresh: vec![0, 1, 2, 3, 4],
            row_lengths: vec![1, 2, 3],
            normalized_density: 0.0,
        };

        // 2, 0, 4, 3, 1
        seq.create_new_distribution_given_active_steps();

        assert!(seq.thresh[0] >= 2);
        assert!(seq.thresh[1] < 2);
        assert!(seq.thresh[4] < 2);
    }

    #[test]
    fn test_flatten_grid_into_single_row() {
        let rows = vec![vec![1], vec![2, 3], vec![4, 5, 6]];
        let result = flatten(rows);
        assert_eq!(result, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn unflatten_rows_into_grid() {
        let flat: Vec<usize> = vec![1, 2, 3, 4, 5, 6];
        let row_lengths: Vec<usize> = vec![1, 2, 3];
        let grid = unflatten(&flat, &row_lengths);

        let expected = vec![vec![1], vec![2, 3], vec![4, 5, 6]];

        assert_eq!(grid, expected);
    }

    #[test]
    fn test_index_conversions() {
        let row_lengths: Vec<usize> = vec![1, 2, 3];
        {
            let flat_index = 5;
            let result = flat_index_to_grid_index(flat_index, &row_lengths);
            assert_eq!(result, (2, 2));
            assert_eq!(grid_index_to_flat_index(result, &row_lengths), flat_index);
        }
        {
            let flat_index = 0;
            let result = flat_index_to_grid_index(flat_index, &row_lengths);
            assert_eq!(result, (0, 0));
            assert_eq!(grid_index_to_flat_index(result, &row_lengths), flat_index);
        }
        {
            let flat_index = 1;
            let result = flat_index_to_grid_index(flat_index, &row_lengths);
            assert_eq!(result, (1, 0));
            assert_eq!(grid_index_to_flat_index(result, &row_lengths), flat_index);
        }
        {
            let row_lengths: Vec<usize> = vec![0, 0, 1];

            let flat_index: usize = 0;
            let result = flat_index_to_grid_index(flat_index, &row_lengths);

            assert_eq!(result, (2, 0));
            assert_eq!(grid_index_to_flat_index(result, &row_lengths), flat_index);
        }
    }

    #[test]
    fn test_append_steps() {
        let mut seq = GridActivations {
            active: vec![true, true, true, true, true, true],
            thresh: vec![0, 1, 2, 3, 4, 5],
            row_lengths: vec![1, 2, 3],
            normalized_density: 0.0,
        };

        // insert a step at end of second row
        seq.append_steps(1, 3);

        let expected_active = vec![true, true, true, false, true, true, true];
        assert_eq!(seq.active, expected_active);

        let expected_thresh: Vec<usize> = vec![0, 1, 2, 6, 3, 4, 5];
        assert_eq!(seq.thresh, expected_thresh);

        let expected_row_lengths: Vec<usize> = std::vec![1, 3, 3];
        assert_eq!(seq.row_lengths, expected_row_lengths);

        assert_eq!(seq.get_row_length(2), 3);
        assert_eq!(seq.get_row(2), vec![true, true, true]);

        assert_eq!(seq.normalized_density, 6.0 / 7.0);
    }

    #[test]
    fn test_append_steps_edge_cases() {
        let mut seq = GridActivations {
            active: vec![],
            thresh: vec![],
            row_lengths: vec![0, 0, 0],
            normalized_density: 0.0,
        };

        // insert a step at end of second row
        seq.append_steps(1, 1);
        let expected_active = vec![false];
        assert_eq!(seq.active, expected_active);
        assert_eq!(seq.normalized_density, 0.0);
    }

    #[test]
    fn test_remove_steps() {
        let mut seq = GridActivations {
            active: vec![true, true, true, false, false, false],
            thresh: vec![0, 1, 2, 3, 4, 5],
            row_lengths: vec![1, 2, 3],
            normalized_density: 0.0,
        };

        // remove the second element of the second row, the third in the flat list
        seq.remove_steps(1, 1);

        let expected_active = vec![true, true, false, false, false];
        assert_eq!(seq.active, expected_active);

        let expected_thresh: Vec<usize> = vec![0, 1, 2, 3, 4];
        assert_eq!(seq.thresh, expected_thresh);

        let expected_row_lengths: Vec<usize> = vec![1, 1, 3];
        assert_eq!(seq.row_lengths, expected_row_lengths);

        assert_eq!(seq.normalized_density, 2.0 / 5.0);
    }
}
