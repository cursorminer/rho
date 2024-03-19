extern crate rand;

use crate::rand::prelude::SliceRandom;
use rand::thread_rng;

pub fn wrap(i: usize, max: usize) -> usize {
    if max == 0 {
        return 0;
    }
    i % (max + 1)
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

// When the density is changed, the active steps change according to their threshold
pub fn set_activations_for_new_density(
    activations: &mut Vec<bool>,
    step_thresh: &Vec<usize>,
    density: usize,
) {
    for i in 0..activations.len() {
        activations[i] = step_thresh[i] < density;
    }
}

fn num_active_steps(active: &Vec<bool>) -> usize {
    active
        .iter()
        .fold(0, |acc, x| if *x { acc + 1 } else { acc })
}

//  adjust distribution  whilst respecting the changed step (step at index)
// if something changed, returns true
pub fn change_step_update_thresholds(
    thresh: &mut Vec<usize>,
    active: &mut Vec<bool>,
    step_index: usize,
    on: bool,
) -> bool {
    if active[step_index] == on {
        return false;
    }

    active[step_index] = on;

    //  find the index of the step that would have changed as a result of the new density
    // and swap thresholds of the step we want to change with that
    let density = if on {
        num_active_steps(&active) - 1
    } else {
        num_active_steps(&active)
    };

    let i = thresh.iter().position(|&x| x == density).unwrap();

    thresh.swap(step_index, i);
    true
}

// a new random distribution, generate thresholds where only the provided steps exceed the threshold.
// The threshold is returned as a density
pub fn create_new_distribution_given_active_steps(active: &Vec<bool>) -> (Vec<usize>, usize) {
    let n: usize = active.len().try_into().unwrap();
    let mut result = create_new_distribution(n);
    let mut dummy_active = vec![false; active.len()];

    // now make sure lowest thresholds correspond to active steps activating steps one by
    // one
    // need to randomise order to avoid consecutive thresholds
    let indices = create_new_distribution(n);
    for i in indices {
        change_step_update_thresholds(&mut result, &mut dummy_active, i, active[i]);
    }

    (result, num_active_steps(active))
}

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

// appending a new step to the end of a row will change the steps arrays, the thresh arrays etc.
// the new step is always inactive
pub fn append_steps(
    active: &mut Vec<bool>,
    thresh: &mut Vec<usize>,
    row_lengths: &mut Vec<usize>,
    row_to_append: usize,
    new_length: usize,
) {
    let num_to_insert = new_length - row_lengths[row_to_append];

    // we need to insert the thresholds that do not exist yet, they're always the biggest
    // (should they be? yes, because we want to preseve the patterns in the other bit)

    let old_flat_length = row_lengths.iter().sum::<usize>();
    let mut thresh_to_insert: Vec<_> = (old_flat_length..old_flat_length + num_to_insert).collect();

    let mut rng = thread_rng();
    thresh_to_insert.shuffle(&mut rng);

    let active_to_insert = vec![false; num_to_insert];

    let insert_position = grid_index_to_flat_index((row_to_append + 1, 0), row_lengths);

    active.splice(insert_position..insert_position, active_to_insert);
    thresh.splice(insert_position..insert_position, thresh_to_insert);

    debug_assert!(active.len() == thresh.len());

    row_lengths[row_to_append] = new_length;
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
    fn can_create_new_distribution() {
        assert_eq!(create_new_distribution(5).len(), 5);
    }

    #[test]
    fn test_set_activations_for_new_density() {
        let thresh: Vec<usize> = vec![0, 1, 2, 4, 3];
        let mut active = vec![false, false, false, false, false];
        set_activations_for_new_density(&mut active, &thresh, 0);
        assert_eq!(active, vec![false, false, false, false, false]);

        set_activations_for_new_density(&mut active, &thresh, 4);
        assert_eq!(active, vec![true, true, true, false, true]);

        set_activations_for_new_density(&mut active, &thresh, 5);
        assert_eq!(active, vec![true, true, true, true, true]);
    }

    #[test]
    fn test_num_active_steps() {
        assert_eq!(num_active_steps(&vec![false, true, false, false, true]), 2);
        assert_eq!(num_active_steps(&vec![true, true, false, false, true]), 3);
    }

    #[test]
    fn test_change_step() {
        let mut thresh: Vec<usize> = vec![0, 1, 2, 3, 4];
        let mut active = vec![false, false, false, false, false];
        let density: usize = 1;

        // smallest density only has one active step
        set_activations_for_new_density(&mut active, &thresh, density);
        assert_eq!(active, vec![true, false, false, false, false]);

        // now set step 4 to active
        change_step_update_thresholds(&mut thresh, &mut active, 4, true);

        //expect that step 4 will get thresh of 1 and density qill be 2
        assert_eq!(active, vec![true, false, false, false, true]);
        assert_eq!(thresh, vec![0, 4, 2, 3, 1]);

        // turn off step 0
        change_step_update_thresholds(&mut thresh, &mut active, 0, false);
        // expect that step 0 will be turned off
        assert_eq!(active, vec![false, false, false, false, true]);
        // and the will be set to 1, swapped with the last density 0
        assert_eq!(thresh, vec![1, 4, 2, 3, 0]);
    }

    #[test]
    fn test_create_new_distribution_given_active_steps() {
        let active = vec![false, true, false, false, true];
        // 2, 0, 4, 3, 1
        let (thresh, density) = create_new_distribution_given_active_steps(&active);

        assert!(thresh[0] >= 2);
        assert!(thresh[1] < 2);
        assert!(thresh[4] < 2);
        assert!(density == 2);
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
        let mut thresh: Vec<usize> = vec![0, 1, 2, 3, 4, 5];
        let mut active = vec![true, true, true, true, true, true];

        let mut row_lengths: Vec<usize> = vec![1, 2, 3];

        // insert a step at end of second row
        append_steps(&mut active, &mut thresh, &mut row_lengths, 1, 3);

        let expected_active = vec![true, true, true, false, true, true, true];
        assert_eq!(active, expected_active);

        let expected_thresh: Vec<usize> = vec![0, 1, 2, 6, 3, 4, 5];
        assert_eq!(thresh, expected_thresh);

        let expected_row_lengths: Vec<usize> = std::vec![1, 3, 3];
        assert_eq!(row_lengths, expected_row_lengths);
    }
}
