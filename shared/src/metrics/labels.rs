// Prometheus metric label constants.
//
// Centralizing label names prevents typo-induced cardinality explosions
// and makes grep-able dashboards easier to build.

/// Label for the operation type (insert, update, delete, truncate).
pub const LABEL_OP: &str = "op";

/// Label for the fully-qualified table name.
pub const LABEL_TABLE: &str = "table";

/// Label for the schema name.
pub const LABEL_SCHEMA: &str = "schema";

/// Label for the consumer/subscription ID.
pub const LABEL_CONSUMER: &str = "consumer";

/// Label for the delivery mode (streaming, polling, webhook).
pub const LABEL_DELIVERY_MODE: &str = "delivery_mode";

/// Label for the egress type (grpc, webhook).
pub const LABEL_EGRESS_TYPE: &str = "egress_type";

/// Label for the pipeline stage.
pub const LABEL_STAGE: &str = "stage";

/// Label for the error category.
pub const LABEL_ERROR: &str = "error";

/// Label for the tenant ID.
pub const LABEL_TENANT: &str = "tenant";

/// Label for webhook target name.
pub const LABEL_WEBHOOK_TARGET: &str = "webhook_target";

/// Label indicating circuit breaker state (closed, open, half_open).
pub const LABEL_CIRCUIT_STATE: &str = "circuit_state";
