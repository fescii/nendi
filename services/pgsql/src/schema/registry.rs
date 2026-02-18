use dashmap::DashMap;

use tracing::{debug, info, warn};

/// Tracks the current schema state for all monitored tables.
///
/// The registry is populated from Relation messages in the replication
/// stream and can be augmented with explicit schema queries. It stores
/// fingerprints so that consumers can detect schema drift.
pub struct SchemaRegistry {
  /// Table name → schema info.
  tables: DashMap<String, TableSchema>,
}

/// Schema information for a single table.
#[derive(Debug, Clone)]
pub struct TableSchema {
  pub schema: String,
  pub table: String,
  pub columns: Vec<ColumnSchema>,
  pub fingerprint: u64,
}

/// Schema information for a single column.
#[derive(Debug, Clone)]
pub struct ColumnSchema {
  pub name: String,
  pub type_oid: u32,
  pub type_name: String,
  pub is_nullable: bool,
  pub is_primary_key: bool,
  pub ordinal_position: i32,
}

impl SchemaRegistry {
  pub fn new() -> Self {
    Self {
      tables: DashMap::new(),
    }
  }

  /// Register or update a table schema.
  pub fn upsert(&self, qualified_name: &str, schema: TableSchema) {
    let old_fp = self.tables.get(qualified_name).map(|s| s.fingerprint);

    if let Some(old) = old_fp {
      if old != schema.fingerprint {
        info!(
          table = qualified_name,
          old_fp = old,
          new_fp = schema.fingerprint,
          "schema changed"
        );
      }
    } else {
      debug!(
        table = qualified_name,
        fp = schema.fingerprint,
        cols = schema.columns.len(),
        "registered new table schema"
      );
    }

    self.tables.insert(qualified_name.to_string(), schema);
  }

  /// Look up the schema for a table.
  pub fn get(&self, qualified_name: &str) -> Option<TableSchema> {
    self.tables.get(qualified_name).map(|r| r.clone())
  }

  /// Get the fingerprint for a table.
  pub fn fingerprint(&self, qualified_name: &str) -> Option<u64> {
    self.tables.get(qualified_name).map(|r| r.fingerprint)
  }

  /// Check if a fingerprint matches the current schema.
  pub fn validate_fingerprint(&self, qualified_name: &str, fp: u64) -> bool {
    self
      .tables
      .get(qualified_name)
      .map(|r| r.fingerprint == fp)
      .unwrap_or(false)
  }

  /// List all tracked tables.
  pub fn table_names(&self) -> Vec<String> {
    self.tables.iter().map(|r| r.key().clone()).collect()
  }

  /// Total number of tracked tables.
  pub fn len(&self) -> usize {
    self.tables.len()
  }

  /// Returns true if no tables are tracked.
  pub fn is_empty(&self) -> bool {
    self.tables.is_empty()
  }
}

/// Load the schema for all tables in a publication from PostgreSQL.
pub async fn load_publication_schemas(
  client: &crate::connector::PgClient,
  publication_name: &str,
  registry: &SchemaRegistry,
) -> anyhow::Result<usize> {
  let tables = client.publication_tables(publication_name).await?;
  let mut count = 0;

  for qualified in &tables {
    let parts: Vec<&str> = qualified.splitn(2, '.').collect();
    if parts.len() != 2 {
      warn!(table = qualified, "invalid qualified table name");
      continue;
    }

    let rows = client
      .inner()
      .query(
        "SELECT column_name, udt_name, is_nullable, ordinal_position
                 FROM information_schema.columns
                 WHERE table_schema = $1 AND table_name = $2
                 ORDER BY ordinal_position",
        &[&parts[0], &parts[1]],
      )
      .await?;

    // Get primary key columns
    let pk_rows = client
      .inner()
      .query(
        "SELECT a.attname
                 FROM pg_index i
                 JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
                 WHERE i.indrelid = ($1 || '.' || $2)::regclass AND i.indisprimary",
        &[&parts[0], &parts[1]],
      )
      .await
      .unwrap_or_default();

    let pk_names: Vec<String> = pk_rows.iter().map(|r| r.get(0)).collect();

    let columns: Vec<ColumnSchema> = rows
      .iter()
      .map(|r| {
        let name: String = r.get(0);
        let type_name: String = r.get(1);
        let nullable: String = r.get(2);
        let ordinal: i32 = r.get(3);
        ColumnSchema {
          is_primary_key: pk_names.contains(&name),
          name,
          type_oid: 0, // OID not available from information_schema
          type_name,
          is_nullable: nullable == "YES",
          ordinal_position: ordinal,
        }
      })
      .collect();

    let fingerprint = super::fingerprint::compute(&parts[0], &parts[1], &columns);

    registry.upsert(
      qualified,
      TableSchema {
        schema: parts[0].to_string(),
        table: parts[1].to_string(),
        fingerprint,
        columns,
      },
    );
    count += 1;
  }

  info!(count = count, "loaded publication schemas");
  Ok(count)
}
