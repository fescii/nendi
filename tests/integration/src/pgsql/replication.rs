#![cfg(test)]
use super::schema::load_test_config;
use futures::StreamExt;
use nendi_pgsql::connector::{ConnectorConfig, PgClient};
use tokio_postgres::NoTls;

#[tokio::test]
#[ignore]
async fn test_logical_replication_stream() {
  let cfg = load_test_config();
  let connector_config = ConnectorConfig::from_nendi_config(&cfg);

  // 1. Setup (reuse schema logic roughly, or assume clean state from other tests - careful with parallel tests)
  // For robust tests, we should self-contain or use a shared setup fixture.
  // We'll do a quick cleanup here.
  let (client, connection) = PgClient::connect(connector_config.clone()).await.unwrap();
  tokio::spawn(async move {
    connection.await.unwrap();
  });

  client
    .execute(&format!(
      "DROP PUBLICATION IF EXISTS {}",
      cfg.publication.name
    ))
    .await
    .unwrap();
  let _ = client
    .execute(&format!(
      "SELECT pg_drop_replication_slot('{}')",
      cfg.slot.name
    ))
    .await;

  // Apply schema before creating publication
  super::schema::apply_fixtures(client.inner()).await;

  client
    .execute(&format!(
      "CREATE PUBLICATION {} FOR TABLE public.customers, public.orders, public.payments",
      cfg.publication.name
    ))
    .await
    .unwrap();
  client
    .inner()
    .execute(
      &format!(
        "SELECT pg_create_logical_replication_slot('{}', '{}')",
        cfg.slot.name, cfg.slot.output_plugin
      ),
      &[],
    )
    .await
    .unwrap();

  // 2. Connect in Replication Mode
  let conn_str = &cfg.source.connection_string;
  // Append replication=database to the connection string
  // Assumes conn_str doesn't already have it
  let rpl_conn_str = format!("{} replication=database", conn_str);

  let pg_config: tokio_postgres::Config = rpl_conn_str.parse().unwrap();

  let (rpl_client, rpl_connection) = pg_config.connect(NoTls).await.unwrap();
  tokio::spawn(async move {
    rpl_connection.await.unwrap();
  });

  // 3. Start Streaming
  let query = format!(
    "START_REPLICATION SLOT {} LOGICAL 0/0 (proto_version '1', publication_names '{}')",
    cfg.slot.name, cfg.publication.name
  );

  // copy_out returns a Stream of messages
  // START_REPLICATION uses CopyBoth, but maybe copy_out works if we only read?
  // If this fails at runtime, we know we need a different approach.
  let stream = rpl_client.copy_out(&query).await.unwrap();
  let stream = Box::pin(stream);

  // 4. Insert Data (on regular connection)
  let (regular_client, regular_conn) = PgClient::connect(connector_config.clone()).await.unwrap();
  tokio::spawn(async move {
    regular_conn.await.unwrap();
  });

  // Ensure table exists
  regular_client
    .inner()
    .batch_execute(
      "CREATE TABLE IF NOT EXISTS public.test_repl (id SERIAL PRIMARY KEY, note TEXT);",
    )
    .await
    .unwrap();

  regular_client
    .inner()
    .execute(
      "INSERT INTO public.test_repl (note) VALUES ($1)",
      &[&"hello world"],
    )
    .await
    .unwrap();

  // 5. Verify Receipt
  // We need to poll the stream.
  // Note: This is low-level byte stream (pgoutput protocol).
  // We won't parse the whole pgoutput protocol here (too complex),
  // but we can check if we receive *something*.

  // We might wrap this in a timeout
  let item = tokio::time::timeout(std::time::Duration::from_secs(5), stream.into_future()).await;

  match item {
    Ok((Some(Ok(_msg)), _)) => {
      println!("Received replication message!");
    }
    Ok((None, _)) => panic!("Stream ended explicitly"),
    Ok((Some(Err(e)), _)) => panic!("Stream error: {}", e),
    Err(_) => panic!("Timed out waiting for replication message"),
  }
}
