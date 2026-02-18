use crossbeam::queue::ArrayQueue;
use std::sync::Arc;

/// A bounded, lock-free, multi-producer multi-consumer ring buffer.
///
/// Wraps `crossbeam::queue::ArrayQueue` to provide the ingestion ↔
/// storage handoff buffer described in the architecture docs. When the
/// buffer is full, producers receive backpressure (a `Full` error)
/// rather than blocking.
pub struct RingBuffer<T> {
    inner: Arc<ArrayQueue<T>>,
    capacity: usize,
}

impl<T> RingBuffer<T> {
    /// Create a new ring buffer with the given capacity.
    ///
    /// # Panics
    /// Panics if `capacity` is zero.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "ring buffer capacity must be > 0");
        Self {
            inner: Arc::new(ArrayQueue::new(capacity)),
            capacity,
        }
    }

    /// Try to push an item into the buffer.
    ///
    /// Returns `Err(item)` if the buffer is full (backpressure signal).
    pub fn try_push(&self, item: T) -> Result<(), T> {
        self.inner.push(item).map_err(|e| e)
    }

    /// Try to pop an item from the buffer.
    ///
    /// Returns `None` if the buffer is empty.
    pub fn try_pop(&self) -> Option<T> {
        self.inner.pop()
    }

    /// Returns the number of items currently in the buffer.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns `true` if the buffer is full.
    pub fn is_full(&self) -> bool {
        self.inner.len() >= self.capacity
    }

    /// Returns the total capacity of the buffer.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the current fill ratio (0.0–1.0).
    pub fn fill_ratio(&self) -> f64 {
        self.inner.len() as f64 / self.capacity as f64
    }

    /// Returns a clone of the inner `Arc` for shared ownership.
    pub fn handle(&self) -> Arc<ArrayQueue<T>> {
        Arc::clone(&self.inner)
    }
}

impl<T> Clone for RingBuffer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            capacity: self.capacity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_roundtrip() {
        let rb = RingBuffer::new(4);
        assert!(rb.try_push(1).is_ok());
        assert!(rb.try_push(2).is_ok());
        assert_eq!(rb.len(), 2);
        assert_eq!(rb.try_pop(), Some(1));
        assert_eq!(rb.try_pop(), Some(2));
        assert!(rb.is_empty());
    }

    #[test]
    fn backpressure_when_full() {
        let rb = RingBuffer::new(2);
        assert!(rb.try_push(1).is_ok());
        assert!(rb.try_push(2).is_ok());
        assert!(rb.is_full());
        assert!(rb.try_push(3).is_err());
    }
}
