use shared::event::ChangeEvent;
use shared::lsn::Lsn;
use tokio::sync::mpsc;

/// Manages a single consumer's event stream.
///
/// Events flowing from the pipeline are sent to the consumer via
/// an mpsc channel. The stream tracks the latest sent LSN for
/// offset management.
pub struct ConsumerStream {
  consumer_id: String,
  tx: mpsc::Sender<ChangeEvent>,
  last_sent_lsn: Lsn,
  events_sent: u64,
}

impl ConsumerStream {
  pub fn new(consumer_id: &str, buffer_size: usize) -> (Self, mpsc::Receiver<ChangeEvent>) {
    let (tx, rx) = mpsc::channel(buffer_size);
    let stream = Self {
      consumer_id: consumer_id.to_string(),
      tx,
      last_sent_lsn: Lsn::ZERO,
      events_sent: 0,
    };
    (stream, rx)
  }

  /// Send an event to the consumer.
  ///
  /// Returns `Err` if the consumer has disconnected.
  pub async fn send(&mut self, event: ChangeEvent) -> Result<(), ChangeEvent> {
    let lsn = event.lsn();
    match self.tx.send(event).await {
      Ok(()) => {
        self.last_sent_lsn = lsn;
        self.events_sent += 1;
        Ok(())
      }
      Err(mpsc::error::SendError(event)) => Err(event),
    }
  }

  /// Try to send without blocking.
  pub fn try_send(&mut self, event: ChangeEvent) -> Result<(), ChangeEvent> {
    let lsn = event.lsn();
    match self.tx.try_send(event) {
      Ok(()) => {
        self.last_sent_lsn = lsn;
        self.events_sent += 1;
        Ok(())
      }
      Err(mpsc::error::TrySendError::Full(event)) => Err(event),
      Err(mpsc::error::TrySendError::Closed(event)) => Err(event),
    }
  }

  pub fn consumer_id(&self) -> &str {
    &self.consumer_id
  }

  pub fn last_sent_lsn(&self) -> Lsn {
    self.last_sent_lsn
  }

  pub fn events_sent(&self) -> u64 {
    self.events_sent
  }
}
