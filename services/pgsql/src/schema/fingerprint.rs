use super::registry::ColumnSchema;

/// Compute a stable schema fingerprint for drift detection.
///
/// The fingerprint is a hash of the schema name, table name, and all
/// column names + types. When a consumer sees a different fingerprint
/// than expected, it knows the schema has changed and can re-fetch
/// the column mapping.
///
/// Uses FNV-1a for speed (this is not a security hash).
pub fn compute(schema: &str, table: &str, columns: &[ColumnSchema]) -> u64 {
  let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis

  for b in schema.bytes() {
    hash ^= b as u64;
    hash = hash.wrapping_mul(0x100000001b3); // FNV prime
  }
  hash ^= b'.' as u64;
  hash = hash.wrapping_mul(0x100000001b3);
  for b in table.bytes() {
    hash ^= b as u64;
    hash = hash.wrapping_mul(0x100000001b3);
  }

  for col in columns {
    // Separator
    hash ^= b'|' as u64;
    hash = hash.wrapping_mul(0x100000001b3);

    for b in col.name.bytes() {
      hash ^= b as u64;
      hash = hash.wrapping_mul(0x100000001b3);
    }
    hash ^= b':' as u64;
    hash = hash.wrapping_mul(0x100000001b3);
    for b in col.type_name.bytes() {
      hash ^= b as u64;
      hash = hash.wrapping_mul(0x100000001b3);
    }
  }

  hash
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn same_schema_same_fingerprint() {
    let cols = vec![
      ColumnSchema {
        name: "id".to_string(),
        type_oid: 23,
        type_name: "int4".to_string(),
        is_nullable: false,
        is_primary_key: true,
        ordinal_position: 1,
      },
      ColumnSchema {
        name: "name".to_string(),
        type_oid: 25,
        type_name: "text".to_string(),
        is_nullable: true,
        is_primary_key: false,
        ordinal_position: 2,
      },
    ];
    let fp1 = compute("public", "users", &cols);
    let fp2 = compute("public", "users", &cols);
    assert_eq!(fp1, fp2);
  }

  #[test]
  fn different_schema_different_fingerprint() {
    let cols1 = vec![ColumnSchema {
      name: "id".to_string(),
      type_oid: 23,
      type_name: "int4".to_string(),
      is_nullable: false,
      is_primary_key: true,
      ordinal_position: 1,
    }];
    let cols2 = vec![ColumnSchema {
      name: "id".to_string(),
      type_oid: 20,
      type_name: "int8".to_string(),
      is_nullable: false,
      is_primary_key: true,
      ordinal_position: 1,
    }];
    let fp1 = compute("public", "users", &cols1);
    let fp2 = compute("public", "users", &cols2);
    assert_ne!(fp1, fp2);
  }
}
