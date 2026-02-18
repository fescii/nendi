use serde::{Deserialize, Serialize};
use std::fmt;

/// The type of DML operation that produced this change event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpType {
    Insert,
    Update,
    Delete,
    Truncate,
}

impl OpType {
    /// Returns the short string tag used in protobuf and filter expressions.
    pub fn as_str(&self) -> &'static str {
        match self {
            OpType::Insert => "insert",
            OpType::Update => "update",
            OpType::Delete => "delete",
            OpType::Truncate => "truncate",
        }
    }

    /// Parse from a string tag.
    pub fn from_str_tag(s: &str) -> Option<Self> {
        match s {
            "insert" | "INSERT" | "I" => Some(OpType::Insert),
            "update" | "UPDATE" | "U" => Some(OpType::Update),
            "delete" | "DELETE" | "D" => Some(OpType::Delete),
            "truncate" | "TRUNCATE" | "T" => Some(OpType::Truncate),
            _ => None,
        }
    }
}

impl fmt::Display for OpType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Identifies a table within a schema.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableId {
    /// PostgreSQL schema name (e.g. `"public"`).
    pub schema: String,
    /// Table name (e.g. `"orders"`).
    pub table: String,
}

impl TableId {
    pub fn new(schema: impl Into<String>, table: impl Into<String>) -> Self {
        Self {
            schema: schema.into(),
            table: table.into(),
        }
    }

    /// Returns the fully-qualified name: `schema.table`.
    pub fn qualified(&self) -> String {
        format!("{}.{}", self.schema, self.table)
    }

    /// Parse from `"schema.table"` format.
    pub fn from_qualified(s: &str) -> Option<Self> {
        let (schema, table) = s.split_once('.')?;
        Some(Self {
            schema: schema.to_string(),
            table: table.to_string(),
        })
    }
}

impl fmt::Display for TableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.schema, self.table)
    }
}

/// A single column value within a change event row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnValue {
    /// Column name.
    pub name: String,
    /// PostgreSQL type OID.
    pub type_oid: u32,
    /// Raw text representation of the value, `None` if SQL NULL.
    pub value: Option<String>,
}

/// Represents the before/after state of a row in a change event.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RowData {
    /// Column values for this row snapshot.
    pub columns: Vec<ColumnValue>,
}

impl RowData {
    /// Look up a column value by name.
    pub fn get(&self, name: &str) -> Option<&ColumnValue> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// Returns the text value of a column, or `None` if the column is missing
    /// or NULL.
    pub fn get_text(&self, name: &str) -> Option<&str> {
        self.get(name)?.value.as_deref()
    }
}
