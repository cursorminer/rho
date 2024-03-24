#[derive(Debug, Clone)]
pub struct LoopingSequence<T> {
    pub data: Vec<T>,
    counter: usize,
}

impl<T> LoopingSequence<T>
where
    T: Clone,
    T: Copy,
{
    pub fn new(data: Vec<T>) -> Self {
        Self {
            data: data,
            counter: 0,
        }
    }

    pub fn reset(&mut self) {
        self.counter = 0;
    }

    pub fn resize(&mut self, new_length: usize, value: T) {
        // adjust counter to be within bounds
        if self.counter >= new_length {
            self.counter = self.counter % new_length;
        }
        self.data.resize(new_length, value);
    }

    pub fn set_data(&mut self, data: Vec<T>) {
        self.data = data.clone();
    }
}

// is there a point to doing this? could it be useful?
impl<T> Iterator for LoopingSequence<T>
where
    T: Clone + Copy,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.data.is_empty() {
            None
        } else {
            let value = self.data[self.counter];
            self.counter += 1;
            if self.counter >= self.data.len() {
                self.counter = 0;
            }
            Some(value)
        }
    }
}

impl<T> ExactSizeIterator for LoopingSequence<T>
where
    T: Clone + Copy,
{
    fn len(&self) -> usize {
        self.data.len()
    }
}

impl<T> Default for LoopingSequence<T>
where
    T: Clone + Copy,
{
    fn default() -> Self {
        let default_data = vec![];
        Self {
            data: default_data,
            counter: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looping_state() {
        let mut s = LoopingSequence::new(vec![10, 20, 30]);

        assert_eq!(s.next(), Some(10));
        assert_eq!(s.next(), Some(20));
        assert_eq!(s.next(), Some(30));
        assert_eq!(s.next(), Some(10));
        s.reset();
        assert_eq!(s.next(), Some(10));
    }
}
