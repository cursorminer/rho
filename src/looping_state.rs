use arrayvec::ArrayVec;

// a fixed capacity sequence that loops round forever, does not allocate
#[derive(Debug, Clone)]
pub struct LoopingSequence<T, const N: usize> {
    data: ArrayVec<T, N>,
    counter: usize,
}

impl<T, const N: usize> LoopingSequence<T, N>
where
    T: Clone,
    T: Copy,
{
    // anything past the capacity is dropped
    pub fn new(data: &[T]) -> Self {
        let mut s = Self::default();
        s.set_data(data);
        s
    }

    pub fn clear(&mut self) {
        self.counter = 0;
        self.data.clear();
    }

    pub fn reset(&mut self) {
        self.counter = 0;
    }

    // returns false if full
    pub fn append(&mut self, value: T) -> bool {
        self.data.try_push(value).is_ok()
    }

    // replace the contents but keep our place in the loop, anything past the capacity is dropped
    pub fn set_data(&mut self, data: &[T]) {
        self.data.clear();
        let n = data.len().min(N);
        // can't fail, we just checked the length
        let _ = self.data.try_extend_from_slice(&data[..n]);
    }

    pub fn data(&self) -> &[T] {
        &self.data
    }

    // allocates, not for the audio thread
    pub fn clone_data(&self) -> Vec<T> {
        self.data.to_vec()
    }
}

impl<T, const N: usize> Iterator for LoopingSequence<T, N>
where
    T: Clone + Copy,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.counter >= self.data.len() {
            self.counter = 0;
        }
        if self.data.is_empty() {
            None
        } else {
            let value = self.data[self.counter];
            self.counter += 1;
            Some(value)
        }
    }
}

impl<T, const N: usize> ExactSizeIterator for LoopingSequence<T, N>
where
    T: Clone + Copy,
{
    fn len(&self) -> usize {
        self.data.len()
    }
}

impl<T, const N: usize> Default for LoopingSequence<T, N>
where
    T: Clone + Copy,
{
    fn default() -> Self {
        Self {
            data: ArrayVec::new(),
            counter: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looping_state() {
        let mut s: LoopingSequence<i32, 4> = LoopingSequence::new(&[10, 20, 30]);

        assert_eq!(s.next(), Some(10));
        assert_eq!(s.next(), Some(20));
        assert_eq!(s.next(), Some(30));
        assert_eq!(s.next(), Some(10));
        s.reset();
        assert_eq!(s.next(), Some(10));
        assert_eq!(s.next(), Some(20));
        s.set_data(&[10]);
        assert_eq!(s.next(), Some(10));
        assert_eq!(s.next(), Some(10));
    }

    #[test]
    fn set_data_keeps_place() {
        let mut s: LoopingSequence<i32, 4> = LoopingSequence::new(&[10, 20, 30]);
        assert_eq!(s.next(), Some(10));

        s.set_data(&[10, 20, 30, 40]);
        assert_eq!(s.next(), Some(20));
    }

    #[test]
    fn drops_values_past_capacity() {
        let mut s: LoopingSequence<i32, 2> = LoopingSequence::new(&[10, 20, 30]);
        assert_eq!(s.data(), &[10, 20]);
        assert!(!s.append(40));
    }
}
