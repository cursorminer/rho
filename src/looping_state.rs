pub struct LoopingSequence<T> {
    data: Vec<T>,
    counter: usize,
}

impl<T> LoopingSequence<T> {
    pub fn new(data: Vec<T>) -> Self {
        Self {
            data: data,
            counter: 0,
        }
    }

    pub fn reset(&mut self) {
        self.counter = 0;
    }

    pub fn tick(&mut self) -> T {
        self.counter += 1;
        if self.counter >= self.data.len() {
            self.counter = 0;
        }
        self.data[self.counter]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looping_state() {
        let s = LoopingState::new(vec![10, 20, 30]);
    }
}
