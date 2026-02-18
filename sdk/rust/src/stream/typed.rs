//! Typed event stream.

use crate::deserialize::NendiDeserialize;
use crate::error::NendiError;
use crate::event::TypedEvent;
use crate::offset::Offset;
use crate::stream::NendiStream;

/// A typed stream that deserializes events into `T` automatically.
///
/// Created via `SubscriptionBuilder::typed::<T>().subscribe()`.
pub struct TypedStream<T: NendiDeserialize> {
    inner: NendiStream,
    _marker: std::marker::PhantomData<T>,
}

impl<T: NendiDeserialize> TypedStream<T> {
    /// Create a new typed stream wrapping a raw stream.
    pub fn new(inner: NendiStream) -> Self {
        Self {
            inner,
            _marker: std::marker::PhantomData,
        }
    }

    /// Receive the next typed event from the stream.
    pub async fn next(&mut self) -> Result<Option<TypedEvent<T>>, NendiError> {
        match self.inner.next().await? {
            Some(event) => {
                let data: T = event.payload()?;
                Ok(Some(TypedEvent::new(event, data)))
            }
            None => Ok(None),
        }
    }

    /// Commit an offset.
    pub async fn commit(&mut self, offset: Offset) -> Result<(), NendiError> {
        self.inner.commit(offset).await
    }

    /// Returns the stream ID.
    pub fn stream_id(&self) -> &str {
        self.inner.stream_id()
    }
}
