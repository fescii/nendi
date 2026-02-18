use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Instant;

pub fn main() {
  let mut base_path = env::current_dir().unwrap();
  if base_path.ends_with("tests/bench") {
    base_path.pop();
    base_path.pop();
  }
  let csv_dir = base_path.join("tests/csvs/tests");
  if csv_dir.exists() {
    fs::remove_dir_all(&csv_dir).unwrap();
  }
  fs::create_dir_all(&csv_dir).unwrap();

  let raw_path = csv_dir.join("raw.csv");
  let summary_path = csv_dir.join("summary.csv");

  let mut raw_file = File::create(&raw_path).unwrap();
  let mut summary_file = File::create(&summary_path).unwrap();

  writeln!(raw_file, "test_name,status").unwrap();
  writeln!(summary_file, "total,passed,failed,ignored,total_duration_s").unwrap();

  println!("Running tests...");
  let start_time = Instant::now();

  // Run cargo test with standard output, capturing stdout
  // We use --no-fail-fast to run all tests even if some fail
  let mut child = Command::new("cargo")
    .arg("test")
    .arg("--workspace")
    .arg("--")
    .arg("--test-threads=1") // Force sequential to avoid interleaved output issues (optional but safer for parsing)
    .arg("--nocapture") // We want to see the output to parse it? No, piped stdout is enough.
    .stdout(Stdio::piped())
    .stderr(Stdio::piped()) // output often goes to stderr or stdout depending on harness
    .spawn()
    .expect("Failed to start cargo test");

  let stdout = child.stdout.take().unwrap();
  let reader = BufReader::new(stdout);

  let mut total = 0;
  let mut passed = 0;
  let mut failed = 0;
  let mut ignored = 0;

  // Standard rust test output lines look like:
  // test package::module::test_name ... ok
  // test package::module::test_name ... FAILED
  // test package::module::test_name ... ignored

  for line in reader.lines() {
    let line = line.unwrap();
    // println!("DEBUG: {}", line); // Optional: print line to see progress

    let trimmed = line.trim();
    if trimmed.starts_with("test ")
      && (trimmed.ends_with("... ok")
        || trimmed.ends_with("... FAILED")
        || trimmed.ends_with("... ignored"))
    {
      let parts: Vec<&str> = trimmed.split_whitespace().collect();
      if parts.len() >= 3 {
        // parts[0] = "test"
        // parts[1] = test_name
        // ...
        // last part = status

        let name = parts[1];
        let status_raw = parts.last().unwrap();
        let status = match *status_raw {
          "ok" => "passed",
          "FAILED" => "failed",
          "ignored" => "ignored",
          _ => "unknown",
        };

        writeln!(raw_file, "{},{}", name, status).unwrap();

        total += 1;
        match status {
          "passed" => passed += 1,
          "failed" => failed += 1,
          "ignored" => ignored += 1,
          _ => {}
        }
      }
    }
  }

  let _ = child.wait();
  let total_duration = start_time.elapsed().as_secs_f64();

  writeln!(
    summary_file,
    "{},{},{},{},{:.2}",
    total, passed, failed, ignored, total_duration
  )
  .unwrap();

  println!("Test report generated.");
  println!("Raw: {}", raw_path.display());
  println!("Summary: {}", summary_path.display());
}
