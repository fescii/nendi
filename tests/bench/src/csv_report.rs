use nendi_core::pipeline::ingestion::IngestionFilter;
use nendi_core::storage::wal::WalWriter;
use shared::event::{ChangeEvent, ColumnValue, OpType, RowData, TableId};
use shared::lsn::Lsn;
use std::fs::{self, File};
use std::io::Write;
use std::time::Instant;
use tempfile::tempdir;

pub fn main() {
  // Ensure base results directory exists
  let mut base_path = std::env::current_dir().unwrap();
  // Adjust path if running from crate directory
  if base_path.ends_with("tests/bench") {
    base_path.pop();
    base_path.pop();
  }
  let base_dir = base_path.join("tests/csvs");

  if base_dir.exists() {
    fs::remove_dir_all(&base_dir).unwrap(); // Clean up previous results
  }
  fs::create_dir_all(&base_dir).unwrap();

  let event = ChangeEvent {
    lsn: 1,
    op: OpType::Insert,
    xid: 100,
    table: TableId::new("public", "bench"),
    schema_fingerprint: 0,
    commit_timestamp_us: 1678888888,
    before: None,
    after: Some(RowData {
      columns: vec![
        ColumnValue {
          name: "id".to_string(),
          type_oid: 23,
          value: Some("1".to_string()),
        },
        ColumnValue {
          name: "data".to_string(),
          type_oid: 25,
          value: Some("test_data_payload_for_benchmark".to_string()),
        },
      ],
    }),
  };

  let iterations = 10_000;

  // --- JSON Benchmarks (Serialize/Deserialize) ---
  let json_dir = format!("{}/json", base_dir.display());
  fs::create_dir_all(&json_dir).unwrap();

  let mut raw_file = File::create(format!("{}/raw.csv", json_dir)).unwrap();
  let mut summary_file = File::create(format!("{}/summary.csv", json_dir)).unwrap();

  writeln!(raw_file, "operation,iteration,latency_ns").unwrap();
  writeln!(
    summary_file,
    "operation,count,min_ns,max_ns,mean_ns,p50_ns,p90_ns,p99_ns"
  )
  .unwrap();

  // Serialize
  let mut measurements = Vec::with_capacity(iterations);
  for i in 0..iterations {
    let start = Instant::now();
    serde_json::to_vec(&event).unwrap();
    let duration = start.elapsed().as_nanos();
    measurements.push(duration);
    writeln!(raw_file, "serialize,{},{}", i, duration).unwrap();
  }
  write_summary(&mut summary_file, "serialize", &measurements);

  // Deserialize
  let serialized = serde_json::to_vec(&event).unwrap();
  measurements.clear();
  for i in 0..iterations {
    let start = Instant::now();
    let _: ChangeEvent = serde_json::from_slice(&serialized).unwrap();
    let duration = start.elapsed().as_nanos();
    measurements.push(duration);
    writeln!(raw_file, "deserialize,{},{}", i, duration).unwrap();
  }
  write_summary(&mut summary_file, "deserialize", &measurements);

  // --- Ingestion Benchmarks ---
  let ingest_dir = format!("{}/ingest", base_dir.display());
  fs::create_dir_all(&ingest_dir).unwrap();

  let mut raw_file = File::create(format!("{}/raw.csv", ingest_dir)).unwrap();
  let mut summary_file = File::create(format!("{}/summary.csv", ingest_dir)).unwrap();

  writeln!(raw_file, "operation,iteration,latency_ns").unwrap();
  writeln!(
    summary_file,
    "operation,count,min_ns,max_ns,mean_ns,p50_ns,p90_ns,p99_ns"
  )
  .unwrap();

  let filter = IngestionFilter::new();
  measurements.clear();
  for i in 0..iterations {
    let start = Instant::now();
    let _ = filter.accept(&event);
    let duration = start.elapsed().as_nanos();
    measurements.push(duration);
    writeln!(raw_file, "filter,{},{}", i, duration).unwrap();
  }
  write_summary(&mut summary_file, "filter", &measurements);

  // --- WAL/Storage Benchmarks ---
  let wal_dir = format!("{}/wal", base_dir.display());
  fs::create_dir_all(&wal_dir).unwrap();

  let mut raw_file = File::create(format!("{}/raw.csv", wal_dir)).unwrap();
  let mut summary_file = File::create(format!("{}/summary.csv", wal_dir)).unwrap();

  writeln!(raw_file, "operation,iteration,latency_ns").unwrap();
  writeln!(
    summary_file,
    "operation,count,min_ns,max_ns,mean_ns,p50_ns,p90_ns,p99_ns"
  )
  .unwrap();

  let dir = tempdir().unwrap();
  let mut wal = WalWriter::new(dir.path().to_path_buf(), 1024 * 1024).unwrap();
  let payload = serialized.clone();

  measurements.clear();
  let mut current_lsn = 1;

  for i in 0..iterations {
    let lsn = Lsn::new(current_lsn);
    let start = Instant::now();
    wal.append(lsn, &payload).unwrap();
    let duration = start.elapsed().as_nanos();

    current_lsn += 100;

    measurements.push(duration);
    writeln!(raw_file, "append,{},{}", i, duration).unwrap();
  }
  write_summary(&mut summary_file, "append", &measurements);

  println!("Benchmark completed. Results saved to tests/csvs/{{json,ingest,wal}}/");
}

fn write_summary(file: &mut File, op: &str, data: &[u128]) {
  let mut sorted = data.to_vec();
  sorted.sort();

  let count = sorted.len();
  if count == 0 {
    return;
  }

  let min = sorted[0];
  let max = sorted[count - 1];
  let sum: u128 = sorted.iter().sum();
  let mean = sum / count as u128;
  let p50 = sorted[count * 50 / 100];
  let p90 = sorted[count * 90 / 100];
  let p99 = sorted[count * 99 / 100];

  writeln!(
    file,
    "{},{},{},{},{},{},{},{}",
    op, count, min, max, mean, p50, p90, p99
  )
  .unwrap();
}
