#![allow(dead_code)]

pub struct Phasor {
    phase: f32,
    phase_inc: f32,
}

impl Phasor {
    pub fn new() -> Self {
        Phasor {
            phase: 0.0,
            phase_inc: 0.1,
        }
    }

    pub fn set_rate(&mut self, rate: f32, sample_rate: f32) {
        self.phase_inc = rate / sample_rate;
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    // returns Some when the clock switches low or high
    pub fn tick(&mut self) -> f32 {
        self.phase += self.phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        self.phase
    }

    pub fn phase(&self) -> f32 {
        self.phase
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phasor() {
        let mut phasor = Phasor::new();
        phasor.set_rate(2.0, 8.0);
        assert_eq!(phasor.tick(), 0.25);
        assert_eq!(phasor.tick(), 0.5);
        assert_eq!(phasor.tick(), 0.75);
        assert_eq!(phasor.tick(), 0.0);
    }
}
