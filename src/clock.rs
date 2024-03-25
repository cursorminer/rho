#![allow(dead_code)]

struct Clock {
    duty_cycle: f32,
    gate_on: bool,
    phase: f32,
    phase_inc: f32,
}

impl Clock {
    pub fn new() -> Self {
        Clock {
            duty_cycle: 0.5,
            gate_on: false,
            phase: 0.0,
            phase_inc: 0.1,
        }
    }

    pub fn set_rate(&mut self, rate: f32, sample_rate: f32) {
        self.phase_inc = rate / sample_rate;
    }

    pub fn set_duty_cycle(&mut self, duty: f32) {
        self.duty_cycle = duty;
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    // returns Some when the clock switches low or high
    fn tick(&mut self) -> Option<bool> {
        self.phase += self.phase_inc;
        if self.phase > 1.0 {
            self.phase -= 1.0;
        }

        let new_gate_on = self.phase <= self.duty_cycle;
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
