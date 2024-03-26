// A synth that goes bloop

pub struct Blooper {
    phasor: Phasor,
    sample_rate: f32,
}

impl Blooper {
    pub fn new(sample_rate: f32) -> Self {
        Blooper {
            phasor: Phasor::new(),
            sample_rate: sample_rate,
        }
    }

    pub fn note_on(&mut self, note_number: usize, velocity: usize) {
        let f = note_number_to_frequency(note_number as f32);
        phasor.set_rate(f, sample_rate)
    }
}
