# Nendi Daemon (`cmd/nendi`)

The executable binary that wires up all components of the system.

## Responsibilities

1. **Bootstrapping**: Loads configuration, setups logging/tracing.
2. **Lifecycle Management**: Starts the pipeline, connectors, and API servers. Handles graceful shutdown on SIGINT/SIGTERM.
3. **Runtime Configuration**: Sets up the Tokio runtime, thread pools, and memory limits.

## Operational Commands

```bash
# Start the daemon
nendi

# Specify config file
nendi -c /etc/nendi/config.toml

# Override environment
NENDI_ENV=production nendi
```

## Logging

Structured logging is provided via `tracing`.

* **Format**: JSON (production) or human-readable (dev).
* **Levels**: Controlled via `RUST_LOG` env var (e.g., `RUST_LOG=info,nendi_pgsql=debug`).
