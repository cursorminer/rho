#![allow(dead_code)]

struct Clock {
    rate: f32,
    swing_amount: f32,
    gate_on: bool,
}

impl Clock {
    pub fn setRate(rate: f32) {
        self.rate = rate;
    }

    // returns Some when the clock switches low or high
    fn tick() -> Option<gate_on> {
        Some(gate_on)
    }
}
