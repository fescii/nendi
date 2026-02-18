/// Types of PostgreSQL event triggers that Nendi installs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventTriggerKind {
  /// Fires on DDL commands (ALTER TABLE, etc.).
  DdlCommand,
  /// Fires when a table is dropped.
  SqlDrop,
  /// Fires when a table is renamed.
  TableRewrite,
}

impl EventTriggerKind {
  /// Returns the PostgreSQL event name for this trigger.
  pub fn pg_event_name(&self) -> &'static str {
    match self {
      EventTriggerKind::DdlCommand => "ddl_command_end",
      EventTriggerKind::SqlDrop => "sql_drop",
      EventTriggerKind::TableRewrite => "table_rewrite",
    }
  }
}

/// Status of an event trigger in the database.
#[derive(Debug, Clone)]
pub struct EventTriggerStatus {
  pub name: String,
  pub kind: EventTriggerKind,
  pub enabled: bool,
  pub function_name: String,
}
