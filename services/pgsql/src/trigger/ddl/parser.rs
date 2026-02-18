use crate::schema::buffer::DdlKind;

/// Parsed DDL message from the PostgreSQL event trigger.
pub struct ParsedDdl {
  pub table: String,
  pub kind: DdlKind,
  pub statement: String,
}

/// Parse a DDL notification message.
///
/// The expected format is a simple text protocol from the trigger function:
/// `DDL_TYPE|schema.table|statement`
///
/// For example:
/// `ALTER_TABLE|public.orders|ALTER TABLE public.orders ADD COLUMN status text`
pub fn parse_ddl_message(content: &str) -> anyhow::Result<ParsedDdl> {
  let parts: Vec<&str> = content.splitn(3, '|').collect();
  if parts.len() < 3 {
    anyhow::bail!("invalid DDL message format: {}", content);
  }

  let kind = match parts[0] {
    "ALTER_TABLE" => DdlKind::AlterTable,
    "DROP_TABLE" => DdlKind::DropTable,
    "RENAME_TABLE" => {
      // For rename, the statement contains the new name.
      let new_name = extract_new_name(parts[2]).unwrap_or_default();
      DdlKind::RenameTable { new_name }
    }
    "ADD_COLUMN" => {
      let col = extract_column_name(parts[2]).unwrap_or_default();
      DdlKind::AddColumn { column: col }
    }
    "DROP_COLUMN" => {
      let col = extract_column_name(parts[2]).unwrap_or_default();
      DdlKind::DropColumn { column: col }
    }
    "ALTER_COLUMN" => {
      let col = extract_column_name(parts[2]).unwrap_or_default();
      DdlKind::AlterColumn { column: col }
    }
    _ => DdlKind::Other,
  };

  Ok(ParsedDdl {
    table: parts[1].to_string(),
    kind,
    statement: parts[2].to_string(),
  })
}

/// Extract the new table name from a RENAME statement.
fn extract_new_name(stmt: &str) -> Option<String> {
  // Pattern: "ALTER TABLE ... RENAME TO new_name"
  let upper = stmt.to_uppercase();
  if let Some(pos) = upper.find("RENAME TO ") {
    let after = &stmt[pos + 10..];
    let name = after.split_whitespace().next()?;
    Some(name.trim_end_matches(';').to_string())
  } else {
    None
  }
}

/// Extract the column name from an ADD/DROP/ALTER COLUMN statement.
fn extract_column_name(stmt: &str) -> Option<String> {
  let upper = stmt.to_uppercase();
  // Pattern: "... COLUMN column_name ..."
  if let Some(pos) = upper.find("COLUMN ") {
    let after = &stmt[pos + 7..];
    let name = after.split_whitespace().next()?;
    Some(name.trim_end_matches(';').to_string())
  } else {
    None
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_alter_table() {
    let msg = "ALTER_TABLE|public.orders|ALTER TABLE public.orders ADD COLUMN status text";
    let parsed = parse_ddl_message(msg).unwrap();
    assert_eq!(parsed.table, "public.orders");
    assert!(matches!(parsed.kind, DdlKind::AlterTable));
  }

  #[test]
  fn parse_add_column() {
    let msg = "ADD_COLUMN|public.users|ALTER TABLE public.users ADD COLUMN email text";
    let parsed = parse_ddl_message(msg).unwrap();
    assert_eq!(parsed.table, "public.users");
    match parsed.kind {
      DdlKind::AddColumn { column } => assert_eq!(column, "email"),
      _ => panic!("expected AddColumn"),
    }
  }
}
