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
pub fn create_new_distribution(n: usize) -> Vec<usize> {
    // vec has ascending integers 0-N
    // then shuffle it randomly
    let mut v = Vec::from_iter(0..n);
    let mut rng = thread_rng();
    v.shuffle(&mut rng);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_wraps() {
        assert_eq!(wrap(5, 3), 1);
        // CHECK(wrap(5, 4) == 0);
        // CHECK(wrap(4, 4) == 4);
        // CHECK(wrap(4, 0) == 0);
        assert_eq!(wrap(5, 4), 0);
    }

    #[test]
    fn can_create_new_distribution() {
        assert_eq!(create_new_distribution(5).len(), 5);
    }
}
