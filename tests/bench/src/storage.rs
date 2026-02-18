use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use nendi_core::storage::wal::WalWriter;
use shared::event::{ChangeEvent, ColumnValue, OpType, RowData, TableId};
use tempfile::tempdir;

fn benchmark_wal_append(c: &mut Criterion) {
  let mut group = c.benchmark_group("storage");
  group.throughput(Throughput::Elements(1));

  let dir = tempdir().unwrap();
  let mut wal = WalWriter::new(dir.path().to_path_buf(), 1024 * 1024).unwrap();

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

  // Pre-serialize event like the real pipeline would
  let payload = serde_json::to_vec(&event).unwrap();
  let lsn = shared::lsn::Lsn::new(1);

  group.bench_function("wal_append", |b| {
    b.iter(|| {
      wal.append(lsn, &payload).unwrap();
    })
  });

  group.finish();
}

criterion_group!(benches, benchmark_wal_append);
criterion_main!(benches);
