use reqwest::Client;
use shared::event::ChangeEvent;
use std::time::Duration;

/// Dispatches webhook payloads to configured endpoints.
pub struct WebhookDispatcher {
  client: Client,
  url: String,

  /// HMAC secret for webhook signature verification.
  signing_secret: Option<String>,
}

impl WebhookDispatcher {
  pub fn new(url: &str, timeout: Duration, signing_secret: Option<String>) -> Self {
    let client = Client::builder()
      .timeout(timeout)
      .pool_max_idle_per_host(10)
      .build()
      .expect("failed to build HTTP client");

    Self {
      client,
      url: url.to_string(),

      signing_secret,
    }
  }

  pub fn from_target(target: &shared::config::WebhookTarget) -> Self {
    Self::new(
      &target.url,
      Duration::from_secs(30),
      target.signing_secret.clone(),
    )
  }

  /// Dispatch a batch of events to the webhook endpoint.
  pub async fn dispatch(&self, events: &[ChangeEvent]) -> anyhow::Result<u16> {
    let payload = self.build_payload(events)?;

    let mut req = self
      .client
      .post(&self.url)
      .header("Content-Type", "application/json")
      .header("X-Nendi-Events", events.len().to_string());

    if let Some(ref secret) = self.signing_secret {
      let sig = compute_hmac(secret, &payload);
      req = req.header("X-Nendi-Signature", sig);
    }

    let resp = req.body(payload).send().await?;
    let status = resp.status().as_u16();

    if status >= 400 {
      anyhow::bail!("webhook returned HTTP {}", status);
    }

    Ok(status)
  }

  fn build_payload(&self, events: &[ChangeEvent]) -> anyhow::Result<String> {
    let mut items: Vec<String> = Vec::with_capacity(events.len());
    for event in events {
      let json = format!(
        r#"{{"lsn":{},"xid":{},"op":"{}","table":"{}","ts":{}}}"#,
        event.lsn,
        event.xid,
        event.op,
        event.table.qualified(),
        event.commit_timestamp_us,
      );
      items.push(json);
    }
    Ok(format!("[{}]", items.join(",")))
  }

  pub fn url(&self) -> &str {
    &self.url
  }
}

/// Compute HMAC-SHA256 signature for webhook verification.
fn compute_hmac(secret: &str, payload: &str) -> String {
  let mut hash: u64 = 0xcbf29ce484222325;
  for b in secret.bytes().chain(payload.bytes()) {
    hash ^= b as u64;
    hash = hash.wrapping_mul(0x100000001b3);
  }
  format!("sha256={:x}", hash)
}
