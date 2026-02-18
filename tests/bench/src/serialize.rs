use criterion::{black_box, criterion_group, criterion_main, Criterion};
use shared::event::{ChangeEvent, ColumnValue, OpType, RowData, TableId};

fn benchmark_serialize(c: &mut Criterion) {
  let mut group = c.benchmark_group("serialize");

  let event = ChangeEvent {
    lsn: 123456789,
    op: OpType::Insert,
    xid: 100,
    table: TableId::new("public", "users"),
    schema_fingerprint: 0,
    commit_timestamp_us: 1678888888,
    before: None,
    after: Some(RowData {
      columns: vec![
        ColumnValue {
          name: "id".to_string(),
          type_oid: 23, // int4
          value: Some("1".to_string()),
        },
        ColumnValue {
          name: "name".to_string(),
          type_oid: 25, // text
          value: Some("John Doe".to_string()),
        },
        ColumnValue {
          name: "email".to_string(),
          type_oid: 25, // text
          value: Some("john@example.com".to_string()),
        },
      ],
    }),
  };

  group.bench_function("json_serialize", |b| {
    b.iter(|| {
      serde_json::to_vec(black_box(&event)).unwrap();
    })
  });

  let serialized = serde_json::to_vec(&event).unwrap();
  group.bench_function("json_deserialize", |b| {
    b.iter(|| {
      let _: ChangeEvent = serde_json::from_slice(black_box(&serialized)).unwrap();
    })
  });

  group.finish();
}

criterion_group!(benches, benchmark_serialize);
criterion_main!(benches);
