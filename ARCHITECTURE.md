# Architecture

## Runtime Model
- **Async runtime**: [`smol`](https://docs.rs/smol) drives the entire service. The binary boots the runtime via `smol::block_on` in `src/main.rs` and spawns lightweight tasks for each accepted TCP connection.
- **Networking**: `smol::net` supplies the TCP primitives, with TLS handshakes handled through `async-tls` + `rustls`, keeping the stack independent from tokio.
- **Concurrency**: Task management and background work (e.g., connection handling, request forwarding) rely on `smol::spawn` plus channels from `async-channel`.

## Request Lifecycle
1. `main.rs` binds the TCP listener, accepts connections, and delegates parsing/dispatch to `handlers::router`.
2. `handlers/` parse HTTP requests, normalize them into OpenAI-compatible payloads, and route to upstream provider definitions loaded from `config.rs`.
3. Outbound calls reuse `http_client::send_http_request`, which manages a per-destination connection pool (`smol::net::TcpStream` sockets wrapped in optional TLS) to forward requests and stream responses.
4. Responses are streamed back to the caller, supporting both JSON payloads and SSE streaming via `futures-lite` readers.

## Configuration & Mapping
- `config.rs` loads JSON transformer files from `transformer/` and exposes provider-specific settings (base URLs, headers, model mapping, endpoint overrides).
- Runtime selection occurs at startup through CLI arguments, with `DEFAULT_API_KEY` supplying fallbacks.

## Telemetry & Resilience
- `metrics.rs` registers Prometheus counters/gauges without pulling tokio feature gates.
- `error_tracking.rs` wires Sentry reporting using the feature-reduced `sentry` crate (no `reqwest`/tokio transport).
- `rate_limit.rs` implements a token-bucket limiter backed by `dashmap` for lock-free concurrency under smol.

## Tokio Independence
- All dependencies are sourced from the smol / async-io ecosystem; optional features that would pull tokio (e.g., `reqwest`, `tokio-rustls`, `tokio-util`) are kept disabled.
- Manual HTTP client implementation plus `async-tls` remove any need for tokio-based runtimes or compatibility layers.
