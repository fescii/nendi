# Nendi Rust SDK

> **Document location:** `docs/architecture/sdk/rust.md`
> **Status:** Approved for implementation
> **Crate name:** `nendi`
> **Published at:** `crates.io/crates/nendi`
> **Principle:** A developer should be able to consume a stream in five lines
> of Rust. Everything else — reconnection, offset tracking, backpressure,
> deserialization, filtering — is the SDK's problem, not theirs.

---

## Table of Contents

1. [Why an SDK](#why-an-sdk)
2. [What the SDK Owns](#what-the-sdk-owns)
3. [Quickstart](#quickstart)
4. [Client Construction](#client-construction)
5. [Subscribing to a Stream](#subscribing-to-a-stream)
6. [The Event Type](#the-event-type)
7. [Filtering at Subscription Time](#filtering-at-subscription-time)
8. [Offset Management](#offset-management)
9. [Typed Deserialization](#typed-deserialization)
10. [Error Handling](#error-handling)
11. [Reconnection and Resilience](#reconnection-and-resilience)
12. [Multi-Stream Consumption](#multi-stream-consumption)
13. [Backpressure and Flow Control](#backpressure-and-flow-control)
14. [SDK Crate Structure](#sdk-crate-structure)
15. [Implementation Guide](#implementation-guide)

---

## Why an SDK

Without the SDK, consuming a Nendi stream looks like this:

```rust
// Without SDK — what a developer has to write today
let channel = tonic::transport::Channel::from_static("http://nendi:50051")
    .connect()
    .await?;

let mut client = StreamServiceClient::new(channel);

let request = SubscribeRequest {
    stream_id: "orders".to_string(),
    start_offset: vec![],
    table_filters: vec!["public.orders".to_string()],
    column_projection: HashMap::new(),
    row_predicate: String::new(),
    operations: vec![],
    encryption: None,
    delivery_mode: 0,
    batch_size: 0,
};

let mut stream = client.subscribe(request).await?.into_inner();

loop {
    match stream.message().await {
        Ok(Some(event)) => {
            // Deserialize bytes manually
            // Track offset manually
            // Handle schema changes manually
            // No type safety on the payload
        }
        Ok(None) => break,
        Err(e) => {
            // Reconnect manually
            // Restore offset manually
            // Back off manually
        }
    }
}
```

With the SDK, the same thing looks like this:

```rust
// With SDK
let client = NendiClient::new("http://nendi:50051").await?;

let mut stream = client.subscribe("orders").await?;

while let Some(event) = stream.next().await? {
    println!("{:?}", event);
}
```

The SDK is what turns Nendi from infrastructure into a developer tool.

---

## What the SDK Owns

The SDK is responsible for everything below the application's event handler:

| Concern | Without SDK | With SDK |
|---|---|---|
| gRPC client construction | Developer | SDK |
| Reconnection on disconnect | Developer | SDK |
| Exponential backoff | Developer | SDK |
| Offset persistence and restore | Developer | SDK |
| Offset commit after processing | Developer | SDK |
| Subscription filter construction | Developer (raw proto) | SDK (builder API) |
| Payload deserialization | Developer (raw bytes) | SDK (typed generics) |
| Schema change events | Developer (parse manually) | SDK (typed DDLEvent) |
| Backpressure signaling | Developer (HTTP/2 internals) | SDK (yield-based) |
| Health checking | Developer | SDK |
| TLS configuration | Developer | SDK (one line) |

---

## Quickstart

Add to `Cargo.toml`:

```toml
[dependencies]
nendi   = "0.1"
tokio   = { version = "1", features = ["full"] }
serde   = { version = "1", features = ["derive"] }
```

Consume a stream:

```rust
use nendi::NendiClient;
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = NendiClient::new("http://localhost:50051").await?;

    let mut stream = client.subscribe("orders").await?;

    while let Some(event) = stream.next().await? {
        println!(
            "[{}] {} on {}.{}",
            event.lsn(),
            event.op(),
            event.schema(),
            event.table(),
        );
    }

    Ok(())
}
```

---

## Client Construction

### Basic

```rust
use nendi::NendiClient;

let client = NendiClient::new("http://localhost:50051").await?;
```

### With TLS

```rust
let client = NendiClient::builder()
    .endpoint("https://nendi.internal:50051")
    .tls_from_pem(include_str!("certs/ca.pem"))
    .await?;
```

### With Authentication

```rust
let client = NendiClient::builder()
    .endpoint("https://nendi.internal:50051")
    .tls_from_pem(include_str!("certs/ca.pem"))
    .auth_token("your-api-token")          // Bearer token
    .await?;
```

### With Full Config

```rust
use nendi::{NendiClient, ClientConfig, RetryPolicy};
use std::time::Duration;

let client = NendiClient::builder()
    .endpoint("http://nendi.internal:50051")
    .connect_timeout(Duration::from_secs(5))
    .request_timeout(Duration::from_secs(30))
    .retry_policy(RetryPolicy::exponential(
        Duration::from_millis(100),   // initial backoff
        Duration::from_secs(30),      // max backoff
        5,                            // max attempts before surfacing error
    ))
    .keep_alive_interval(Duration::from_secs(10))
    .keep_alive_timeout(Duration::from_secs(5))
    .await?;
```

### From Environment

Reads `NENDI_ENDPOINT`, `NENDI_TOKEN`, `NENDI_TLS_CA` from environment:

```rust
let client = NendiClient::from_env().await?;
```

---

## Subscribing to a Stream

### Minimal Subscribe

```rust
// Subscribe from the current head (latest events only)
let mut stream = client.subscribe("orders").await?;
```

### Subscribe from Beginning

```rust
use nendi::Offset;

let mut stream = client
    .subscription("orders")
    .from(Offset::beginning())
    .subscribe()
    .await?;
```

### Subscribe from Saved Offset

```rust
let saved_offset: Offset = load_offset_from_db().await?;

let mut stream = client
    .subscription("orders")
    .from(saved_offset)
    .subscribe()
    .await?;
```

### Consuming Events

`NendiStream` implements `futures::Stream` so it works with all standard
async stream combinators:

```rust
use tokio_stream::StreamExt;

// Basic iteration
while let Some(event) = stream.next().await? {
    process(event).await?;
}

// With timeout per event
use std::time::Duration;

while let Some(event) = stream.next().timeout(Duration::from_secs(60)).await?? {
    process(event).await?;
}

// Take only N events (useful for testing)
let events: Vec<_> = stream.take(100).collect().await;

// Filter at the async stream level (client-side, after SDK filter)
let inserts = stream.filter(|e| e.op() == Op::Insert);
```

---

## The Event Type

Every event received from a Nendi stream is a `ChangeEvent`. It carries
the operation type, table identity, the row payload, and the offset for
acknowledgment.

```rust
/// A single change event received from a Nendi stream
pub struct ChangeEvent {
    // Unique position of this event in the stream
    // Use this to resume after a restart
    offset: Offset,

    // The type of database operation
    op: Op,

    // Source table identity
    schema: String,
    table: String,

    // Raw payload bytes (rkyv-serialized by daemon)
    // Access via .payload::<T>() for typed deserialization
    raw: Bytes,

    // Previous row state — only populated for UPDATE events
    // Access via .old_payload::<T>()
    raw_old: Option<Bytes>,

    // Schema fingerprint at time of this event
    // Changes when the table schema changes
    schema_fingerprint: [u8; 16],

    // Wall-clock time the event was committed in PostgreSQL
    committed: DateTime<Utc>,

    // PostgreSQL transaction ID
    xid: u32,
}

pub enum Op {
    Insert,
    Update,
    Delete,
    Truncate,
    Ddl,
}
```

### Accessing Event Data

```rust
while let Some(event) = stream.next().await? {
    // Identity
    println!("Table:  {}.{}", event.schema(), event.table());
    println!("Op:     {}", event.op());
    println!("Offset: {}", event.offset());
    println!("Time:   {}", event.committed());

    // Raw bytes (if you want to handle deserialization yourself)
    let raw: &[u8] = event.raw_bytes();

    // Typed payload (see Typed Deserialization section)
    let order: Order = event.payload::<Order>()?;

    // Previous state for updates
    if event.op() == Op::Update {
        let old_order: Order = event.old_payload::<Order>()?;
        println!("Status changed: {} → {}", old_order.status, order.status);
    }

    // Schema change events
    if event.op() == Op::Ddl {
        let ddl: DdlEvent = event.ddl()?;
        println!("DDL: {}", ddl.sql());
    }
}
```

---

## Filtering at Subscription Time

Filters declared on the subscription are sent to the Nendi daemon and
evaluated server-side. Events that do not match are never sent over the
network — this is not client-side filtering.

### Table Filter

```rust
// Only receive events for this specific table
let mut stream = client
    .subscription("orders")
    .table("public.orders")
    .subscribe()
    .await?;

// Multiple tables from the same stream
let mut stream = client
    .subscription("orders")
    .table("public.orders")
    .table("public.payments")
    .subscribe()
    .await?;
```

### Operation Filter

```rust
use nendi::Op;

// Only inserts and updates — no deletes
let mut stream = client
    .subscription("orders")
    .operations([Op::Insert, Op::Update])
    .subscribe()
    .await?;

// Inserts only
let mut stream = client
    .subscription("orders")
    .operations([Op::Insert])
    .subscribe()
    .await?;
```

### Column Projection

Only receive specific columns in the payload. Columns not listed are
stripped by the daemon before sending — reducing network transfer:

```rust
// Only receive these columns from the orders table
let mut stream = client
    .subscription("orders")
    .columns("public.orders", ["id", "status", "amount", "updated"])
    .subscribe()
    .await?;

// Different projections per table
let mut stream = client
    .subscription("orders")
    .columns("public.orders",   ["id", "status", "amount"])
    .columns("public.payments", ["id", "order_id", "status"])
    .subscribe()
    .await?;
```

### Row Predicate

Server-side CEL expression — only matching rows are sent:

```rust
// Only receive high-value completed orders
let mut stream = client
    .subscription("orders")
    .predicate("row.status == 'completed' && row.amount > 10000")
    .subscribe()
    .await?;

// Watch for status changes on update events only
let mut stream = client
    .subscription("orders")
    .predicate("op == 'update' && row.status != old.status")
    .subscribe()
    .await?;
```

### Combined Filter

All filter methods compose cleanly:

```rust
let mut stream = client
    .subscription("orders")
    .from(Offset::beginning())
    .table("public.orders")
    .operations([Op::Insert, Op::Update])
    .columns("public.orders", ["id", "status", "amount", "customer", "updated"])
    .predicate("row.status in ['completed', 'failed'] && row.amount > 0")
    .subscribe()
    .await?;
```

### Inspecting the Filter Before Subscribing

Dry-run a filter to see its estimated pass rate and catch config errors
before connecting to the live stream:

```rust
let inspection = client
    .subscription("orders")
    .predicate("row.amount > 10000 && row.status == 'completed'")
    .columns("public.orders", ["id", "amount", "status"])
    .inspect()
    .await?;

println!("Estimated pass rate: {:.1}%", inspection.pass_rate * 100.0);
println!("Warnings: {:?}", inspection.warnings);

// Only subscribe if the filter looks right
if inspection.warnings.is_empty() {
    let mut stream = client
        .subscription("orders")
        .predicate("row.amount > 10000 && row.status == 'completed'")
        .columns("public.orders", ["id", "amount", "status"])
        .subscribe()
        .await?;
}
```

---

## Offset Management

Offsets are how the SDK knows where to resume after a restart. The SDK
provides three strategies out of the box, and you can implement your own.

### Manual Offset Commit

The simplest model — you decide when to commit:

```rust
while let Some(event) = stream.next().await? {
    // Process the event
    process_order(&event).await?;

    // Commit this offset — tells Nendi it's safe to GC events before here
    stream.commit(event.offset()).await?;
}
```

### Auto-Commit

Automatically commits the offset after each successful `next()` call:

```rust
let mut stream = client
    .subscription("orders")
    .auto_commit(true)
    .subscribe()
    .await?;

// Offset is committed automatically after each event is yielded
while let Some(event) = stream.next().await? {
    process_order(&event).await?;
    // No manual commit needed
}
```

> **Warning:** Auto-commit provides at-least-once semantics — if your
> process crashes between receiving and finishing processing, the event
> will be redelivered on reconnect. For exactly-once semantics, commit
> manually after your processing is confirmed durable.

### Offset Store — Persist Offsets Externally

For production use, store offsets in your own database so they survive
process restarts independent of the Nendi daemon:

```rust
use nendi::{OffsetStore, Offset};
use async_trait::async_trait;

// Implement the OffsetStore trait against your database
struct PostgresOffsetStore {
    pool: sqlx::PgPool,
}

#[async_trait]
impl OffsetStore for PostgresOffsetStore {
    async fn load(&self, stream_id: &str) -> Result<Option<Offset>, nendi::Error> {
        let row = sqlx::query!(
            "SELECT offset_bytes FROM nendi_offsets WHERE stream_id = $1",
            stream_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Offset::from_bytes(r.offset_bytes)))
    }

    async fn save(&self, stream_id: &str, offset: &Offset) -> Result<(), nendi::Error> {
        sqlx::query!(
            "INSERT INTO nendi_offsets (stream_id, offset_bytes, updated)
             VALUES ($1, $2, NOW())
             ON CONFLICT (stream_id) DO UPDATE
             SET offset_bytes = $2, updated = NOW()",
            stream_id,
            offset.as_bytes(),
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

// Wire it up
let store = PostgresOffsetStore { pool };

let mut stream = client
    .subscription("orders")
    .offset_store(Arc::new(store))
    // Auto-save offset after every N events (batched for efficiency)
    .commit_every(100)
    .subscribe()
    .await?;

// SDK automatically loads the saved offset on subscribe,
// and saves it every 100 events without you managing it
while let Some(event) = stream.next().await? {
    process_order(&event).await?;
}
```

### Built-in Offset Stores

The SDK ships with common implementations so you don't have to write your own:

```rust
use nendi::offset::{FileOffsetStore, RedisOffsetStore, NendiOffsetStore};

// Store offset in a local file (good for single-process consumers)
let store = FileOffsetStore::new("/var/lib/myapp/nendi-offset.bin");

// Store offset in Redis
let store = RedisOffsetStore::new("redis://localhost:6379", "myapp:orders:offset").await?;

// Store offset back in Nendi daemon itself (default — no external dependency)
// This is what happens when you call stream.commit()
let store = NendiOffsetStore::default();
```

---

## Typed Deserialization

The SDK supports strongly typed event payloads. Derive `NendiDeserialize`
on your domain types and the SDK handles the bytes-to-struct conversion:

### Deriving Deserialization

```rust
use nendi::NendiDeserialize;
use serde::Deserialize;

#[derive(Debug, NendiDeserialize, Deserialize)]
#[nendi(table = "public.orders")]
pub struct Order {
    pub id: i64,
    pub status: OrderStatus,
    pub amount: i64,      // stored as cents
    pub currency: String,
    pub customer: i64,
    pub created: chrono::DateTime<chrono::Utc>,
    pub updated: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Draft,
    Processing,
    Completed,
    Failed,
    Refunded,
}
```

### Consuming Typed Events

```rust
use nendi::TypedEvent;

// Subscribe with a typed stream — SDK deserializes automatically
let mut stream = client
    .subscription("orders")
    .typed::<Order>()    // all events in this stream are deserialized as Order
    .table("public.orders")
    .subscribe()
    .await?;

while let Some(event) = stream.next().await? {
    // event is TypedEvent<Order>
    let order: &Order = event.data();

    println!("Order #{}: {} for {}{}",
        order.id,
        order.status,
        order.currency,
        order.amount,
    );

    if event.op() == Op::Update {
        let old: &Order = event.old_data();
        if old.status != order.status {
            println!("  Status changed: {:?} → {:?}", old.status, order.status);
        }
    }
}
```

### Multi-Table Typed Streams

When a stream covers multiple tables, use an enum to represent all possible
event types:

```rust
use nendi::{NendiDeserialize, MultiTableEvent};

#[derive(Debug, NendiDeserialize)]
#[nendi(table = "public.orders")]
pub struct Order { /* ... */ }

#[derive(Debug, NendiDeserialize)]
#[nendi(table = "public.payments")]
pub struct Payment { /* ... */ }

// The SDK dispatches to the correct variant by table name
#[derive(Debug, NendiEvent)]
pub enum AppEvent {
    #[nendi(table = "public.orders")]
    Order(Order),
    #[nendi(table = "public.payments")]
    Payment(Payment),
}

let mut stream = client
    .subscription("orders")
    .typed::<AppEvent>()
    .table("public.orders")
    .table("public.payments")
    .subscribe()
    .await?;

while let Some(event) = stream.next().await? {
    match event.data() {
        AppEvent::Order(order)     => handle_order(order).await?,
        AppEvent::Payment(payment) => handle_payment(payment).await?,
    }
}
```

### Schema Evolution — Unknown Fields

When PostgreSQL adds a new column, the SDK defaults to ignoring unknown
fields during deserialization (serde `#[serde(deny_unknown_fields)]` is
NOT set by default). Your struct just doesn't see the new column until you
add it. No panic, no error.

When a column is removed from PostgreSQL that your struct expects, the
SDK returns a `DeserializeError::MissingColumn` which you can handle
explicitly or let propagate as a stream error.

---

## Error Handling

### Error Type

```rust
#[derive(Debug, thiserror::Error)]
pub enum NendiError {
    // Connection to daemon failed
    #[error("Connection failed: {0}")]
    Connection(#[from] tonic::transport::Error),

    // Stream ended unexpectedly (not a clean shutdown)
    #[error("Stream disconnected: {0}")]
    Disconnected(tonic::Status),

    // Event payload could not be deserialized into the target type
    #[error("Deserialization failed for {table}: {source}")]
    Deserialize { table: String, source: Box<dyn std::error::Error + Send + Sync> },

    // Consumer filter references a column not present in stream storage
    #[error("Filter conflict: {0}")]
    FilterConflict(String),

    // Offset was not found in the stream (event GC'd or stream reset)
    #[error("Offset not found — stream may have been reset. Resync required.")]
    OffsetNotFound { requested: Offset, earliest_available: Offset },

    // Daemon returned an error response
    #[error("Daemon error [{code}]: {message}")]
    Daemon { code: u32, message: String },

    // Offset store backend failed
    #[error("Offset store error: {0}")]
    OffsetStore(Box<dyn std::error::Error + Send + Sync>),
}
```

### Handling Specific Errors

```rust
use nendi::NendiError;

while let Some(result) = stream.next().await {
    match result {
        Ok(event) => {
            process_order(&event).await?;
        }

        // Offset was GC'd — need to resync from snapshot
        Err(NendiError::OffsetNotFound { earliest_available, .. }) => {
            eprintln!("Stream reset. Resyncing from offset {:?}", earliest_available);
            stream = client
                .subscription("orders")
                .from(earliest_available)
                .subscribe()
                .await?;
        }

        // Deserialization failure — log and skip, don't crash
        Err(NendiError::Deserialize { table, source }) => {
            tracing::warn!(table, error = %source, "Skipping malformed event");
        }

        // Transient disconnect — SDK reconnects automatically,
        // but you can also intercept here if you need custom logic
        Err(NendiError::Disconnected(status)) => {
            tracing::warn!(code = ?status.code(), "Stream disconnected — SDK will retry");
        }

        Err(e) => return Err(e.into()),
    }
}
```

---

## Reconnection and Resilience

The SDK handles reconnection automatically. When the connection to the
Nendi daemon drops, the SDK:

1. Waits for the configured backoff duration
2. Re-establishes the gRPC connection
3. Re-subscribes from the last committed offset
4. Resumes yielding events — the application sees no gap

From the application's perspective, `stream.next()` just takes longer to
return during a reconnection. It does not error unless all retry attempts
are exhausted.

```rust
use nendi::{NendiClient, RetryPolicy};
use std::time::Duration;

let client = NendiClient::builder()
    .endpoint("http://nendi:50051")
    .retry_policy(RetryPolicy::exponential(
        Duration::from_millis(50),    // start: wait 50ms before first retry
        Duration::from_secs(60),      // cap: never wait more than 60s
        0,                            // max_attempts: 0 = retry forever
    ))
    .await?;

let mut stream = client.subscribe("orders").await?;

// This loop runs forever, surviving any number of daemon restarts
// or network interruptions transparently
while let Some(event) = stream.next().await? {
    process_order(&event).await?;
    stream.commit(event.offset()).await?;
}
```

### Retry Policies

```rust
// Retry forever with exponential backoff (recommended for production)
RetryPolicy::exponential(initial, max, 0)

// Retry a fixed number of times then surface the error
RetryPolicy::exponential(initial, max, 5)

// Retry at a fixed interval (simpler, less friendly to overloaded daemon)
RetryPolicy::fixed(Duration::from_secs(5), 0)

// No retry — error immediately on any disconnect
RetryPolicy::none()
```

---

## Multi-Stream Consumption

For services that need to consume multiple streams concurrently, the SDK
provides a multiplexer that fan-ins multiple streams into a single async
stream:

```rust
use nendi::StreamMux;
use tokio_stream::StreamExt;

let client = NendiClient::new("http://nendi:50051").await?;

// Subscribe to multiple streams concurrently
let orders   = client.subscribe("orders").await?;
let payments = client.subscribe("payments").await?;
let customers = client.subscribe("customers").await?;

// Merge into a single stream — events arrive in arrival order
let mut mux = StreamMux::new()
    .add("orders",    orders)
    .add("payments",  payments)
    .add("customers", customers);

while let Some((stream_id, event)) = mux.next().await? {
    match stream_id {
        "orders"    => handle_order(&event).await?,
        "payments"  => handle_payment(&event).await?,
        "customers" => handle_customer(&event).await?,
        _ => unreachable!(),
    }
}
```

### Tokio Task Per Stream

For completely independent processing with no fan-in, spawn a task per stream:

```rust
use tokio::task::JoinSet;

let client = Arc::new(NendiClient::new("http://nendi:50051").await?);
let mut tasks = JoinSet::new();

// Orders consumer task
{
    let client = Arc::clone(&client);
    tasks.spawn(async move {
        let mut stream = client.subscribe("orders").await?;
        while let Some(event) = stream.next().await? {
            handle_order(&event).await?;
            stream.commit(event.offset()).await?;
        }
        Ok::<_, NendiError>(())
    });
}

// Payments consumer task
{
    let client = Arc::clone(&client);
    tasks.spawn(async move {
        let mut stream = client.subscribe("payments").await?;
        while let Some(event) = stream.next().await? {
            handle_payment(&event).await?;
            stream.commit(event.offset()).await?;
        }
        Ok::<_, NendiError>(())
    });
}

// Wait for all tasks — if any fail, the error is surfaced here
while let Some(result) = tasks.join_next().await {
    result??;
}
```

---

## Backpressure and Flow Control

When your application processes events slower than Nendi delivers them,
the SDK applies backpressure automatically via HTTP/2 window control.
You do not need to do anything to enable this — it is the default behavior.

When `stream.next()` is not called (because your processing logic is busy),
the SDK stops reading from the gRPC receive buffer. When that buffer fills,
HTTP/2 flow control signals the daemon to pause sending. When you call
`next()` again, the pause lifts and delivery resumes.

For explicit control over the receive buffer size:

```rust
let mut stream = client
    .subscription("orders")
    // How many events the SDK buffers internally before applying backpressure
    // Smaller = tighter memory usage, more frequent backpressure signals
    // Larger  = smoother throughput, more memory
    // Default: 1000
    .buffer(500)
    .subscribe()
    .await?;
```

---

## SDK Crate Structure

The SDK lives in its own crate in the Nendi workspace:

```
sdk/
└── rust/
    ├── Cargo.toml
    └── src/
        ├── lib.rs                  # Public API surface — re-exports everything
        │
        ├── client/
        │   ├── mod.rs
        │   ├── builder.rs          # NendiClient builder pattern
        │   ├── config.rs           # ClientConfig struct
        │   └── health.rs           # Health check method on client
        │
        ├── subscription/
        │   ├── mod.rs
        │   ├── builder.rs          # Subscription builder — all filter methods
        │   ├── filter.rs           # Filter struct and proto conversion
        │   └── inspect.rs          # InspectFilter dry-run implementation
        │
        ├── stream/
        │   ├── mod.rs
        │   ├── raw.rs              # NendiStream<ChangeEvent> — untyped
        │   ├── typed.rs            # NendiStream<TypedEvent<T>> — typed
        │   └── mux.rs              # StreamMux multi-stream fan-in
        │
        ├── event/
        │   ├── mod.rs
        │   ├── change.rs           # ChangeEvent type and accessors
        │   ├── typed.rs            # TypedEvent<T> wrapper
        │   ├── ddl.rs              # DdlEvent for schema change events
        │   └── op.rs               # Op enum
        │
        ├── offset/
        │   ├── mod.rs
        │   ├── types.rs            # Offset newtype — opaque bytes wrapper
        │   ├── store.rs            # OffsetStore trait
        │   ├── file.rs             # FileOffsetStore implementation
        │   ├── redis.rs            # RedisOffsetStore implementation
        │   └── nendi.rs            # NendiOffsetStore (daemon-side commit)
        │
        ├── deserialize/
        │   ├── mod.rs
        │   ├── traits.rs           # NendiDeserialize trait
        │   └── derive.rs           # Re-export of nendi-derive proc macro
        │
        ├── retry/
        │   ├── mod.rs
        │   └── policy.rs           # RetryPolicy enum and backoff logic
        │
        └── error/
            ├── mod.rs
            └── types.rs            # NendiError enum
```

The proc-macro crate that powers `#[derive(NendiDeserialize)]` and
`#[derive(NendiEvent)]` lives separately:

```
sdk/
└── rust-derive/
    ├── Cargo.toml
    └── src/
        └── lib.rs                  # Proc macro implementations
```

---

## Implementation Guide

### Workspace Cargo.toml Addition

```toml
[workspace]
members = [
    # ... existing members ...
    "sdk/rust",
    "sdk/rust-derive",
]
```

### SDK Cargo.toml

```toml
[package]
name        = "nendi"
version     = "0.1.0"
edition     = "2021"
description = "Official Rust client SDK for the Nendi change data capture daemon"
license     = "Apache-2.0"
repository  = "https://github.com/fescii/nendi"
keywords    = ["cdc", "streaming", "postgres", "database", "realtime"]
categories  = ["database", "asynchronous", "network-programming"]

[features]
default   = ["tls", "offset-file"]
tls       = ["tonic/tls"]
offset-file  = []
offset-redis = ["redis"]

[dependencies]
# gRPC
tonic       = { version = "0.11", features = ["transport"] }
prost       = "0.12"

# Async
tokio       = { version = "1", features = ["sync", "time"] }
tokio-stream = "0.1"
futures     = "0.3"
async-trait = "0.1"

# Serialization
bytes       = "1"
serde       = { version = "1", features = ["derive"] }

# Proc macro (derive support)
nendi-derive = { path = "../rust-derive", version = "0.1" }

# Error handling
thiserror   = "1"

# Offset stores
redis       = { version = "0.25", optional = true, features = ["tokio-comp"] }

# Utilities
chrono      = { version = "0.4", features = ["serde"] }
tracing     = "0.1"
```

### The Subscription Builder Implementation

```rust
pub struct SubscriptionBuilder<'c> {
    client: &'c NendiClient,
    stream_id: String,
    offset: OffsetSpec,
    tables: Vec<String>,
    operations: Vec<Op>,
    columns: HashMap<String, Vec<String>>,
    predicate: Option<String>,
    buffer_size: usize,
    auto_commit: bool,
    offset_store: Option<Arc<dyn OffsetStore>>,
    commit_every: usize,
}

impl<'c> SubscriptionBuilder<'c> {
    pub fn from(mut self, offset: Offset) -> Self {
        self.offset = OffsetSpec::At(offset);
        self
    }

    pub fn table(mut self, table: impl Into<String>) -> Self {
        self.tables.push(table.into());
        self
    }

    pub fn operations(mut self, ops: impl IntoIterator<Item = Op>) -> Self {
        self.operations = ops.into_iter().collect();
        self
    }

    pub fn columns(
        mut self,
        table: impl Into<String>,
        cols: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.columns.insert(
            table.into(),
            cols.into_iter().map(Into::into).collect(),
        );
        self
    }

    pub fn predicate(mut self, expr: impl Into<String>) -> Self {
        self.predicate = Some(expr.into());
        self
    }

    pub fn buffer(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    pub fn auto_commit(mut self, enabled: bool) -> Self {
        self.auto_commit = enabled;
        self
    }

    pub fn offset_store(mut self, store: Arc<dyn OffsetStore>) -> Self {
        self.offset_store = Some(store);
        self
    }

    pub fn commit_every(mut self, n: usize) -> Self {
        self.commit_every = n;
        self
    }

    // Typed variant — wraps stream in TypedEvent<T> deserializer
    pub fn typed<T: NendiDeserialize>(self) -> TypedSubscriptionBuilder<'c, T> {
        TypedSubscriptionBuilder::from(self)
    }

    pub async fn subscribe(self) -> Result<NendiStream, NendiError> {
        self.client.open_stream(self).await
    }

    pub async fn inspect(self) -> Result<FilterInspection, NendiError> {
        self.client.inspect_filter(self).await
    }
}
```

### Integration Test Example

```rust
// tests/integration/src/sdk/subscribe.rs

#[tokio::test]
async fn test_subscribe_receives_insert_events() {
    let harness = TestHarness::start().await;

    let client = NendiClient::new(&harness.nendi_addr()).await.unwrap();

    let mut stream = client
        .subscription("orders")
        .from(Offset::beginning())
        .table("public.orders")
        .operations([Op::Insert])
        .subscribe()
        .await
        .unwrap();

    // Insert a row in PostgreSQL
    harness.pg().execute(
        "INSERT INTO orders (status, amount) VALUES ('completed', 5000)"
    ).await;

    // Assert the SDK receives it
    let event = tokio::time::timeout(
        Duration::from_secs(5),
        stream.next(),
    )
    .await
    .expect("timeout waiting for event")
    .unwrap()
    .unwrap();

    assert_eq!(event.op(), Op::Insert);
    assert_eq!(event.table(), "orders");

    let order: Order = event.payload().unwrap();
    assert_eq!(order.status, "completed");
    assert_eq!(order.amount, 5000);
}

#[tokio::test]
async fn test_predicate_filters_server_side() {
    let harness = TestHarness::start().await;
    let client = NendiClient::new(&harness.nendi_addr()).await.unwrap();

    let mut stream = client
        .subscription("orders")
        .from(Offset::beginning())
        .predicate("row.amount > 10000")
        .subscribe()
        .await
        .unwrap();

    // Insert one that should pass
    harness.pg().execute(
        "INSERT INTO orders (status, amount) VALUES ('completed', 50000)"
    ).await;

    // Insert one that should be filtered out
    harness.pg().execute(
        "INSERT INTO orders (status, amount) VALUES ('completed', 100)"
    ).await;

    let event = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await.unwrap().unwrap().unwrap();

    let order: Order = event.payload().unwrap();
    assert_eq!(order.amount, 50000);  // only the high-value one arrived

    // Confirm the low-value one never arrives
    let timeout = tokio::time::timeout(Duration::from_millis(500), stream.next()).await;
    assert!(timeout.is_err(), "Low-value order should not have been received");
}
```
