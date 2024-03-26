#![allow(dead_code)]

use crate::phasor;
use phasor::Phasor;

pub struct Clock {
    duty_cycle: f32,
    gate_on: bool,
    phasor: Phasor,
}

impl Clock {
    pub fn new() -> Self {
        Clock {
            duty_cycle: 0.5,
            gate_on: false,
            phasor: Phasor::new(),
        }
    }

    pub fn set_rate(&mut self, rate: f32, sample_rate: f32) {
        self.phasor.set_rate(rate, sample_rate);
    }

    pub fn set_duty_cycle(&mut self, duty: f32) {
        self.duty_cycle = duty;
    }

    pub fn reset(&mut self) {
        self.phasor.reset();
    }

    // returns Some when the clock switches low or high
    pub fn tick(&mut self) -> Option<bool> {
        let phase = self.phasor.phase();
        let new_gate_on = phase < self.duty_cycle;
        let _ = self.phasor.tick();

        if new_gate_on != self.gate_on {
            self.gate_on = new_gate_on;
            Some(self.gate_on)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock() {
        let mut clock = Clock::new();
        clock.set_rate(2.0, 8.0);
        assert_eq!(clock.tick(), Some(true));
        assert_eq!(clock.tick(), None);
        assert_eq!(clock.tick(), Some(false));
        assert_eq!(clock.tick(), None);
        assert_eq!(clock.tick(), Some(true));
    }
}
