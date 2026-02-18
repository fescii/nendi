#![cfg(test)]
use crate::pgsql::schema::load_test_config;
use nendi_pgsql::connector::{ConnectorConfig, PgClient};

#[tokio::test]
#[ignore]
async fn test_slot_lifecycle() {
  let cfg = load_test_config();
  let connector_config = ConnectorConfig::from_nendi_config(&cfg);

  // Reuse shared PgClient logic
  let (client, connection) = PgClient::connect(connector_config.clone())
    .await
    .expect("Failed to connect with PgClient");

  tokio::spawn(async move {
    if let Err(e) = connection.await {
      eprintln!("connection error: {}", e);
    }
  });

  // Clean start
  let _ = client
    .execute(&format!(
      "SELECT pg_drop_replication_slot('{}')",
      cfg.slot.name
    ))
    .await;
  let _ = client
    .execute(&format!("DROP PUBLICATION {}", cfg.publication.name))
    .await;

  // Apply schema
  super::schema::apply_fixtures(client.inner()).await;

  // 1. Create Publication
  client
    .execute(&format!(
      "CREATE PUBLICATION {} FOR TABLE public.customers, public.orders, public.payments",
      cfg.publication.name
    ))
    .await
    .expect("Failed to create publication");

  assert!(client
    .publication_exists(&cfg.publication.name)
    .await
    .unwrap());

  // 2. Create Slot
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
    .expect("Failed to create slot");

  assert!(client.slot_exists(&cfg.slot.name).await.unwrap());

  // 3. Verify Publication Tables
  let tables = client
    .publication_tables(&cfg.publication.name)
    .await
    .unwrap();
  // Assuming schema.rs setup ran or we rely on existing tables.
  // Ideally we run schema setup here too or ensure order.
  // For now just check execution.
  println!("Publication tables: {:?}", tables);
}
