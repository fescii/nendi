// Proto serialization helpers.
//
// These functions convert between the internal `ChangeEvent` type and
// the protobuf-generated types. The actual generated code will come from
// `tonic-build` in a build.rs step — these are the manual conversion
// helpers that bridge the gap.

use crate::event::{ChangeEvent, ColumnValue, OpType, RowData, TableId};

/// Encode a `ChangeEvent` into a protobuf-compatible byte vector.
///
/// Uses a simple length-prefixed binary format:
///   [8 bytes lsn][4 bytes xid][1 byte op][2 bytes schema_len][schema_bytes]
///   [2 bytes table_len][table_bytes][8 bytes fingerprint][8 bytes ts]
///   [payload...]
///
/// This is used for the wire format between the daemon and SDK when
/// the full protobuf codegen is not yet integrated.
pub fn encode_event(event: &ChangeEvent) -> Vec<u8> {
    let mut buf = Vec::with_capacity(event.estimated_size());

    // LSN
    buf.extend_from_slice(&event.lsn.to_be_bytes());
    // XID
    buf.extend_from_slice(&event.xid.to_be_bytes());
    // Op type
    buf.push(op_to_byte(event.op));
    // Schema name
    let schema_bytes = event.table.schema.as_bytes();
    buf.extend_from_slice(&(schema_bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(schema_bytes);
    // Table name
    let table_bytes = event.table.table.as_bytes();
    buf.extend_from_slice(&(table_bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(table_bytes);
    // Fingerprint
    buf.extend_from_slice(&event.schema_fingerprint.to_be_bytes());
    // Commit timestamp
    buf.extend_from_slice(&event.commit_timestamp_us.to_be_bytes());
    // Row data (after)
    encode_row_data(&mut buf, &event.after);
    // Row data (before)
    encode_row_data(&mut buf, &event.before);

    buf
}

/// Decode a `ChangeEvent` from the binary format produced by `encode_event`.
pub fn decode_event(data: &[u8]) -> Result<ChangeEvent, String> {
    let mut pos = 0;

    if data.len() < 8 + 4 + 1 + 2 {
        return Err("buffer too short for event header".to_string());
    }

    // LSN
    let lsn = u64::from_be_bytes(data[pos..pos + 8].try_into().map_err(|_| "lsn")?);
    pos += 8;

    // XID
    let xid = u32::from_be_bytes(data[pos..pos + 4].try_into().map_err(|_| "xid")?);
    pos += 4;

    // Op
    let op = byte_to_op(data[pos]).ok_or("unknown op byte")?;
    pos += 1;

    // Schema
    let schema_len =
        u16::from_be_bytes(data[pos..pos + 2].try_into().map_err(|_| "schema_len")?) as usize;
    pos += 2;
    let schema = std::str::from_utf8(&data[pos..pos + schema_len])
        .map_err(|_| "invalid utf8 in schema")?
        .to_string();
    pos += schema_len;

    // Table
    let table_len =
        u16::from_be_bytes(data[pos..pos + 2].try_into().map_err(|_| "table_len")?) as usize;
    pos += 2;
    let table = std::str::from_utf8(&data[pos..pos + table_len])
        .map_err(|_| "invalid utf8 in table")?
        .to_string();
    pos += table_len;

    // Fingerprint
    let schema_fingerprint =
        u64::from_be_bytes(data[pos..pos + 8].try_into().map_err(|_| "fingerprint")?);
    pos += 8;

    // Commit timestamp
    let commit_timestamp_us =
        i64::from_be_bytes(data[pos..pos + 8].try_into().map_err(|_| "timestamp")?);
    pos += 8;

    // After row
    let (after, new_pos) = decode_row_data(data, pos)?;
    pos = new_pos;

    // Before row
    let (before, _) = decode_row_data(data, pos)?;

    Ok(ChangeEvent {
        lsn,
        xid,
        op,
        table: TableId { schema, table },
        schema_fingerprint,
        after,
        before,
        commit_timestamp_us,
    })
}

fn op_to_byte(op: OpType) -> u8 {
    match op {
        OpType::Insert => b'I',
        OpType::Update => b'U',
        OpType::Delete => b'D',
        OpType::Truncate => b'T',
    }
}

fn byte_to_op(b: u8) -> Option<OpType> {
    match b {
        b'I' => Some(OpType::Insert),
        b'U' => Some(OpType::Update),
        b'D' => Some(OpType::Delete),
        b'T' => Some(OpType::Truncate),
        _ => None,
    }
}

fn encode_row_data(buf: &mut Vec<u8>, row: &Option<RowData>) {
    match row {
        None => buf.push(0),
        Some(row) => {
            buf.push(1);
            buf.extend_from_slice(&(row.columns.len() as u16).to_be_bytes());
            for col in &row.columns {
                // Column name
                let name = col.name.as_bytes();
                buf.extend_from_slice(&(name.len() as u16).to_be_bytes());
                buf.extend_from_slice(name);
                // Type OID
                buf.extend_from_slice(&col.type_oid.to_be_bytes());
                // Value
                match &col.value {
                    None => buf.push(0), // NULL
                    Some(v) => {
                        buf.push(1);
                        let vb = v.as_bytes();
                        buf.extend_from_slice(&(vb.len() as u32).to_be_bytes());
                        buf.extend_from_slice(vb);
                    }
                }
            }
        }
    }
}

fn decode_row_data(data: &[u8], mut pos: usize) -> Result<(Option<RowData>, usize), String> {
    if pos >= data.len() {
        return Ok((None, pos));
    }
    let flag = data[pos];
    pos += 1;
    if flag == 0 {
        return Ok((None, pos));
    }

    let col_count =
        u16::from_be_bytes(data[pos..pos + 2].try_into().map_err(|_| "col_count")?) as usize;
    pos += 2;

    let mut columns = Vec::with_capacity(col_count);
    for _ in 0..col_count {
        let name_len =
            u16::from_be_bytes(data[pos..pos + 2].try_into().map_err(|_| "name_len")?) as usize;
        pos += 2;
        let name = std::str::from_utf8(&data[pos..pos + name_len])
            .map_err(|_| "invalid utf8 in col name")?
            .to_string();
        pos += name_len;

        let type_oid = u32::from_be_bytes(data[pos..pos + 4].try_into().map_err(|_| "type_oid")?);
        pos += 4;

        let val_flag = data[pos];
        pos += 1;
        let value = if val_flag == 0 {
            None
        } else {
            let val_len =
                u32::from_be_bytes(data[pos..pos + 4].try_into().map_err(|_| "val_len")?) as usize;
            pos += 4;
            let v = std::str::from_utf8(&data[pos..pos + val_len])
                .map_err(|_| "invalid utf8 in col value")?
                .to_string();
            pos += val_len;
            Some(v)
        };

        columns.push(ColumnValue {
            name,
            type_oid,
            value,
        });
    }

    Ok((Some(RowData { columns }), pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encode_decode() {
        let event = ChangeEvent {
            lsn: 123456,
            xid: 42,
            op: OpType::Insert,
            table: TableId::new("public", "orders"),
            schema_fingerprint: 0xdeadbeef,
            after: Some(RowData {
                columns: vec![
                    ColumnValue {
                        name: "id".to_string(),
                        type_oid: 23,
                        value: Some("1".to_string()),
                    },
                    ColumnValue {
                        name: "status".to_string(),
                        type_oid: 25,
                        value: Some("pending".to_string()),
                    },
                ],
            }),
            before: None,
            commit_timestamp_us: 1700000000_000_000,
        };

        let encoded = encode_event(&event);
        let decoded = decode_event(&encoded).unwrap();

        assert_eq!(decoded.lsn, event.lsn);
        assert_eq!(decoded.xid, event.xid);
        assert_eq!(decoded.op, event.op);
        assert_eq!(decoded.table.schema, "public");
        assert_eq!(decoded.table.table, "orders");
        assert_eq!(decoded.schema_fingerprint, event.schema_fingerprint);
        assert!(decoded.before.is_none());
        let after = decoded.after.unwrap();
        assert_eq!(after.columns.len(), 2);
        assert_eq!(after.get_text("id"), Some("1"));
    }
}
