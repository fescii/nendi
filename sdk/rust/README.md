# Nendi Rust SDK

The official Rust client for consuming Nendi streams.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
nendi-sdk = "0.1"
```

## Usage

```rust
use nendi_sdk::consumer::ConsumerBuilder;
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut consumer = ConsumerBuilder::new("http://localhost:50051")
        .stream("orders-active")
        .build()
        .await?;

    println!("Connected! Streaming events...");

    while let Some(event) = consumer.next().await {
        match event {
            Ok(change) => {
                println!("Received change for table: {}", change.table);
                // Simple commit on every message (for demonstration)
                consumer.commit(&change.offset).await?;
            }
            Err(e) => eprintln!("Error: {}", e),
        }
    }

    Ok(())
}
```

## Features

* **Automatic Reconnect**: Handles connection drops transparently.
* **Backpressure**: Respects server flow control.
* **Type-Safe**: Maps gRPC protobuf messages to Rust structs.
