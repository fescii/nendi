//! Typed event wrapper.

use crate::event::ChangeEvent;
use crate::offset::Offset;

/// A change event with a deserialized, typed payload.
///
/// Wraps a `ChangeEvent` and provides direct access to the
/// pre-deserialized data of type `T`.
#[derive(Debug)]
pub struct TypedEvent<T> {
  /// The underlying raw event.
  inner: ChangeEvent,
  /// The deserialized payload.
  data: T,
}

impl<T> TypedEvent<T> {
  /// Create a new typed event from a raw event and deserialized data.
  pub fn new(inner: ChangeEvent, data: T) -> Self {
    Self { inner, data }
  }

  /// Returns a reference to the deserialized data.
  pub fn data(&self) -> &T {
    &self.data
  }

  /// Returns the offset of this event.
  pub fn offset(&self) -> Offset {
    self.inner.offset()
  }

  /// Returns a reference to the underlying raw event.
  pub fn inner(&self) -> &ChangeEvent {
    &self.inner
  }

  /// Consume this typed event and return the deserialized data.
  pub fn into_data(self) -> T {
    self.data
  }
}
