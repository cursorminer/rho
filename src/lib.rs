extern crate rand;

use crate::rand::prelude::SliceRandom;
use rand::thread_rng;

pub fn wrap(i: i32, max: i32) -> i32 {
    if max == 0 {
        return 0;
    }
    i % (max + 1)
}

// create a new bunch of thresholds
pub fn create_new_distribution(n: i32) -> Vec<i32> {
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
    step_thresh: &Vec<i32>,
    density: i32,
) {
    for i in 0..activations.len() {
        activations[i] = step_thresh[i] < density;
    }
}

fn num_active_steps(active: &Vec<bool>) -> i32 {
    active
        .iter()
        .fold(0, |acc, x| if *x { acc + 1 } else { acc })
}

//  adjust distribution  whilst respecting the changed step (step at index)
// if something changed, returns true
pub fn change_step_update_thresholds(
    thresh: &mut Vec<i32>,
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
        let thresh: Vec<i32> = vec![0, 1, 2, 4, 3];
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
        let mut thresh: Vec<i32> = vec![0, 1, 2, 3, 4];
        let mut active = vec![false, false, false, false, false];
        let density: i32 = 1;

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
}
