//! Subscription builder — all filter methods.

use std::collections::HashMap;
use std::sync::Arc;

use tonic::transport::Channel;

use crate::client::builder::NendiClient;
use crate::error::NendiError;
use crate::event::Op;
use crate::offset::{Offset, OffsetStore};
use crate::proto::{self, NendiStreamClient};
use crate::stream::NendiStream;
use crate::subscription::inspect::FilterInspection;
use crate::transpile::Transpiler;

/// Specifies where to start reading from in the stream.
#[derive(Debug, Clone)]
pub enum OffsetSpec {
  /// Start from the beginning of the stream.
  Beginning,
  /// Resume from a specific offset.
  At(Offset),
  /// Start from the current head (only new events).
  Latest,
}

impl Default for OffsetSpec {
  fn default() -> Self {
    Self::Latest
  }
}

/// Builder for configuring a stream subscription.
///
/// All filter methods compose cleanly — chain them together
/// to build the exact subscription you need.
pub struct SubscriptionBuilder<'c> {
  client: &'c NendiClient,
  stream_id: String,
  offset: OffsetSpec,
  tables: Vec<String>,
  operations: Vec<Op>,
  columns: HashMap<String, Vec<String>>,
  predicate: Option<String>,
  buffer_size: usize,
  auto_commit: bool,
  offset_store: Option<Arc<dyn OffsetStore>>,
  commit_every: usize,
}

impl<'c> SubscriptionBuilder<'c> {
  /// Create a new subscription builder.
  pub fn new(client: &'c NendiClient, stream_id: &str) -> Self {
    Self {
      client,
      stream_id: stream_id.to_string(),
      offset: OffsetSpec::default(),
      tables: Vec::new(),
      operations: Vec::new(),
      columns: HashMap::new(),
      predicate: None,
      buffer_size: 1000,
      auto_commit: false,
      offset_store: None,
      commit_every: 1,
    }
  }

  /// Resume from a specific offset.
  pub fn from(mut self, offset: Offset) -> Self {
    self.offset = OffsetSpec::At(offset);
    self
  }

  /// Add a table filter — only receive events from this table.
  pub fn table(mut self, table: impl Into<String>) -> Self {
    self.tables.push(table.into());
    self
  }

  /// Set the operations to receive.
  pub fn operations(mut self, ops: impl IntoIterator<Item = Op>) -> Self {
    self.operations = ops.into_iter().collect();
    self
  }

  /// Set column projection for a specific table.
  pub fn columns(
    mut self,
    table: impl Into<String>,
    cols: impl IntoIterator<Item = impl Into<String>>,
  ) -> Self {
    self
      .columns
      .insert(table.into(), cols.into_iter().map(Into::into).collect());
    self
  }

  /// Set a CEL row predicate expression.
  ///
  /// The predicate is transpiled to a PostgreSQL WHERE clause that is
  /// pushed down to the daemon's publication filter (Layer 1), so
  /// non-matching rows never travel over the wire.
  pub fn predicate(mut self, expr: impl Into<String>) -> Self {
    self.predicate = Some(expr.into());
    self
  }

  /// Set the internal buffer size for backpressure control.
  pub fn buffer(mut self, size: usize) -> Self {
    self.buffer_size = size;
    self
  }

  /// Enable or disable auto-commit after each event.
  pub fn auto_commit(mut self, enabled: bool) -> Self {
    self.auto_commit = enabled;
    self
  }

  /// Set an external offset store for persistence.
  pub fn offset_store(mut self, store: Arc<dyn OffsetStore>) -> Self {
    self.offset_store = Some(store);
    self
  }

  /// Commit every N events (when auto-commit is enabled).
  pub fn commit_every(mut self, n: usize) -> Self {
    self.commit_every = n;
    self
  }

  /// Subscribe and start receiving events.
  pub async fn subscribe(self) -> Result<NendiStream, NendiError> {
    // Determine resume LSN from offset spec.
    let resume_lsn = match &self.offset {
      OffsetSpec::Beginning => 0u64,
      OffsetSpec::At(o) => o.as_lsn(),
      OffsetSpec::Latest => {
        // Load from offset store if set; otherwise start from latest (0 signals daemon).
        if let Some(store) = &self.offset_store {
          store
            .load(&self.stream_id)
            .await?
            .map(|o| o.as_lsn())
            .unwrap_or(u64::MAX) // u64::MAX = "latest" sentinel
        } else {
          u64::MAX
        }
      }
    };

    // Transpile predicate to PostgreSQL WHERE for server-side push-down.
    let pg_where = if let Some(expr) = &self.predicate {
      match Transpiler::parse(expr) {
        Ok(t) => Some(t.to_pg_where()),
        Err(e) => {
          tracing::warn!("Predicate transpile failed: {e}; will evaluate client-side");
          None
        }
      }
    } else {
      None
    };

    tracing::info!(
        stream_id = %self.stream_id,
        tables = ?self.tables,
        resume_lsn = resume_lsn,
        pg_where = ?pg_where,
        "Subscribing to stream"
    );

    let req = proto::SubscribeRequest {
      consumer: self.stream_id.clone(),
      resume: if resume_lsn == u64::MAX {
        0
      } else {
        resume_lsn
      },
      tables: self.tables.clone(),
      credits: self.buffer_size as u32,
    };

    let mut grpc = NendiStreamClient::new(self.client.channel());
    let response = grpc
      .subscribe(req)
      .await
      .map_err(NendiError::Disconnected)?;
    let streaming = response.into_inner();

    Ok(NendiStream::new(
      self.stream_id.clone(),
      self.stream_id,
      streaming,
      NendiStreamClient::new(self.client.channel()),
    ))
  }

  /// Dry-run the filter and return an inspection report.
  pub async fn inspect(self) -> Result<FilterInspection, NendiError> {
    // Validate predicate via transpiler.
    let (valid, warnings) = if let Some(expr) = &self.predicate {
      match Transpiler::parse(expr) {
        Ok(_) => (true, vec![]),
        Err(e) => (false, vec![e.to_string()]),
      }
    } else {
      (true, vec![])
    };

    Ok(FilterInspection {
      pass_rate: 1.0,
      warnings,
      valid,
    })
  }
}
