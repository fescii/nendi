use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use nendi_core::pipeline::ingestion::IngestionFilter;
use shared::event::{ChangeEvent, ColumnValue, OpType, RowData, TableId};

fn benchmark_ingestion_filter(c: &mut Criterion) {
  let mut group = c.benchmark_group("ingestion");
  group.throughput(Throughput::Elements(1));

  let filter = IngestionFilter::new();
  // In a real scenario, we'd add rules to the filter.
  // filter.add_rule(...)

  let event = ChangeEvent {
    lsn: 1,
    op: OpType::Insert,
    xid: 1,
    table: TableId::new("public", "bench"),
    schema_fingerprint: 0,
    commit_timestamp_us: 1,
    before: None,
    after: Some(RowData {
      columns: vec![ColumnValue {
        name: "data".to_string(),
        type_oid: 25,
        value: Some("test".to_string()),
      }],
    }),
  };

  group.bench_function("filter_accept", |b| {
    b.iter(|| {
      black_box(filter.accept(black_box(&event)));
    })
  });

  group.finish();
}

criterion_group!(benches, benchmark_ingestion_filter);
criterion_main!(benches);
