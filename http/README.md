# Nendi HTTP & gRPC

The `nendi-http` crate provides the external interfaces for the daemon.

## Components

### gRPC Server (Port 50051)

* **Service**: `nendi.stream.v1.StreamService`
* **Functionality**:
  * `Subscribe`: Bidirectional streaming of change events.
  * `CommitOffset`: Consumer acknowledgment.
* **Performance**: Uses `tonic` for high-throughput HTTP/2 streaming.

### Admin API (Port 9090)

REST API for management and observability.

* `POST /admin/config/reload`: Hot-reload configuration.
* `GET /health`: System health status.
* `GET /metrics`: Prometheus metrics endpoint.

### Webhook Dispatcher

For non-gRPC consumers.

* **Retries**: Exponential backoff for failed delivery.
* **Circuit Breaker**: Protects system resources from failing endpoints.
* **Batching**: Groups multiple events into single HTTP POST requests for efficiency.
