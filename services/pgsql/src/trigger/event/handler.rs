use super::types::{EventTriggerKind, EventTriggerStatus};
use tracing::info;

/// Manages the installation and status of PostgreSQL event triggers.
///
/// Nendi installs event triggers to detect DDL changes on monitored
/// tables. The triggers call the `nendi_capture_ddl()` function which
/// emits a logical decoding message containing the DDL details.
pub struct EventTriggerHandler;

impl EventTriggerHandler {
  /// Install all required event triggers.
  pub async fn install_triggers(client: &crate::connector::PgClient) -> anyhow::Result<()> {
    // Install the DDL command end trigger
    let ddl_trigger_sql = r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM pg_event_trigger WHERE evtname = 'nendi_ddl_trigger'
                ) THEN
                    CREATE EVENT TRIGGER nendi_ddl_trigger
                        ON ddl_command_end
                        EXECUTE FUNCTION nendi_capture_ddl();
                END IF;
            END $$;
        "#;
    client.execute(ddl_trigger_sql).await?;
    info!("installed DDL command trigger");

    // Install the SQL drop trigger
    let drop_trigger_sql = r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM pg_event_trigger WHERE evtname = 'nendi_drop_trigger'
                ) THEN
                    CREATE EVENT TRIGGER nendi_drop_trigger
                        ON sql_drop
                        EXECUTE FUNCTION nendi_capture_ddl();
                END IF;
            END $$;
        "#;
    client.execute(drop_trigger_sql).await?;
    info!("installed SQL drop trigger");

    Ok(())
  }

  /// Check the status of installed event triggers.
  pub async fn check_triggers(
    client: &crate::connector::PgClient,
  ) -> anyhow::Result<Vec<EventTriggerStatus>> {
    let rows = client
      .inner()
      .query(
        "SELECT evtname, evtevent, evtenabled, evtfoid::regproc::text
                 FROM pg_event_trigger
                 WHERE evtname LIKE 'nendi_%'",
        &[],
      )
      .await?;

    let triggers: Vec<EventTriggerStatus> = rows
      .iter()
      .map(|r| {
        let name: String = r.get(0);
        let event: String = r.get(1);
        let enabled: String = r.get(2);
        let func: String = r.get(3);

        let kind = match event.as_str() {
          "ddl_command_end" => EventTriggerKind::DdlCommand,
          "sql_drop" => EventTriggerKind::SqlDrop,
          "table_rewrite" => EventTriggerKind::TableRewrite,
          _ => EventTriggerKind::DdlCommand,
        };

        EventTriggerStatus {
          name,
          kind,
          enabled: enabled != "D",
          function_name: func,
        }
      })
      .collect();

    Ok(triggers)
  }

  /// Remove all Nendi event triggers.
  pub async fn remove_triggers(client: &crate::connector::PgClient) -> anyhow::Result<()> {
    client
      .execute("DROP EVENT TRIGGER IF EXISTS nendi_ddl_trigger")
      .await?;
    client
      .execute("DROP EVENT TRIGGER IF EXISTS nendi_drop_trigger")
      .await?;
    info!("removed all Nendi event triggers");
    Ok(())
  }
}
