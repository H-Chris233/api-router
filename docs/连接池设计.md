# HTTP Connection Pool Implementation

## Overview

This document describes the HTTP connection pooling and reuse optimization implemented in the API Router to reduce connection churn and improve performance.

## Motivation

Previously, the router created a new TCP/TLS connection for every HTTP request and immediately closed it after receiving the response (using `Connection: close` header). This approach had several drawbacks:

1. **High Overhead**: TCP handshake (3-way) + TLS handshake on every request
2. **Increased Latency**: Connection setup time adds 50-200ms per request
3. **Resource Waste**: Constant creation/destruction of connections
4. **Socket Exhaustion**: Risk of TIME_WAIT socket buildup under high load
5. **No TLS Session Reuse**: Lost opportunity to resume TLS sessions

## Solution: Connection Pooling

We've implemented a sophisticated connection pooling system that:

- Maintains pools of reusable HTTP/HTTPS connections per `(scheme, host, port)` tuple
- Uses `async-channel` for efficient connection queue management (smol-compatible)
- Automatically manages connection lifecycle (creation, reuse, expiration)
- Handles errors gracefully by recycling failed connections
- Supports both HTTP and HTTPS (with TLS session reuse)

## Architecture

### Key Components

```
┌─────────────────────────────────────────────────────────┐
│                  CONNECTION_POOL (global)               │
│  DashMap<ConnectionKey, Arc<ConnectionPoolInner>>      │
└─────────────────────────────────────────────────────────┘
                          │
                          ├─ (https, api.example.com, 443)
                          │   └─ ConnectionPoolInner
                          │       ├─ async_channel (bounded queue)
                          │       ├─ config (max_size, idle_timeout)
                          │       └─ active_count (atomic counter)
                          │
                          ├─ (http, localhost, 8080)
                          │   └─ ConnectionPoolInner
                          │
                          └─ ...
```

### Data Structures

#### `ConnectionKey`
```rust
struct ConnectionKey {
    scheme: String,  // "http" or "https"
    host: String,    // hostname or IP
    port: u16,       // port number
}
```

Uniquely identifies a connection pool. Different schemes, hosts, or ports get separate pools.

#### `PooledConnection`
```rust
struct PooledConnection {
    stream: PooledStream,  // Tcp or Tls stream
    last_used: Instant,    // For idle timeout
    connection_id: u64,    // For tracing/debugging
}

enum PooledStream {
    Tcp(TcpStream),
    Tls(TlsStream<TcpStream>),
}
```

Wraps the actual network stream with metadata for lifecycle management.

#### `ConnectionPoolInner`
```rust
struct ConnectionPoolInner {
    sender: Sender<PooledConnection>,      // Return connections here
    receiver: Receiver<PooledConnection>,  // Acquire connections from here
    config: PoolConfig,                    // Pool configuration
    active_count: Arc<AtomicUsize>,        // Total active connections
    next_connection_id: Arc<AtomicU64>,    // ID generator
}
```

Manages a pool for a specific connection key.

#### `PoolConfig`
```rust
pub struct PoolConfig {
    pub max_size: usize,        // Default: 10
    pub idle_timeout: Duration, // Default: 60s
}
```

Configurable pool parameters.

### Connection Lifecycle

#### 1. Acquisition

```rust
async fn acquire(&self, key: &ConnectionKey) -> RouterResult<PooledConnection>
```

Flow:
1. Try to get connection from queue (non-blocking)
2. If found:
   - Check if expired (idle > timeout)
   - If expired: drop it and retry
   - If valid: update `last_used` and return
3. If queue empty:
   - Check if under `max_size` limit
   - If yes: create new connection
   - If no: wait for connection to become available

#### 2. Use

```rust
async fn send_request_on_connection(
    conn: &mut PooledConnection,
    request_bytes: &[u8],
) -> RouterResult<Vec<u8>>
```

Flow:
1. Write HTTP request with `Connection: keep-alive` header
2. Read response headers to determine content length
3. Read exact content length (enables connection reuse)
4. Return response body

#### 3. Return

```rust
async fn return_connection(&self, key: &ConnectionKey, conn: PooledConnection)
```

Flow:
1. Try to send connection back to queue
2. If queue full: drop connection and decrement `active_count`
3. Connection is now available for next request

#### 4. Recycle (on error)

```rust
fn recycle_connection(&self, key: &ConnectionKey)
```

Flow:
1. Decrement `active_count` (connection is dropped)
2. Next request will create a fresh connection

### HTTP/1.1 Keep-Alive

Changed request headers from:
```http
Connection: close
```

To:
```http
Connection: keep-alive
```

This signals the upstream server to keep the connection open for reuse.

### Response Parsing

Enhanced response reading to properly detect end of response:

1. Parse headers to extract `Content-Length`
2. Read exactly `Content-Length` bytes of body
3. Stop reading (don't close connection)

This enables the same TCP stream to be reused for multiple request/response cycles.

## Configuration

Default values (can be overridden):

```rust
const DEFAULT_POOL_MAX_SIZE: usize = 10;
const DEFAULT_POOL_IDLE_TIMEOUT_SECS: u64 = 60;
```

To customize:

```rust
use api_router::PoolConfig;
use std::time::Duration;

let config = PoolConfig {
    max_size: 20,
    idle_timeout: Duration::from_secs(120),
};
```

Currently uses global default config. Future enhancement could allow per-host configuration.

## TLS Session Reuse

TLS session reuse happens implicitly when connections are pooled:

1. Initial TLS handshake establishes session
2. TLS library caches session internally
3. Subsequent uses of same connection may resume session
4. Reduces TLS handshake to 1-RTT (or 0-RTT with TLS 1.3)

The `async-tls` + `rustls` stack handles this automatically.

## Benefits

### Performance Improvements

1. **Reduced Latency**
   - Eliminates TCP handshake (1 RTT ~20-50ms)
   - Eliminates TLS handshake (2 RTT ~40-100ms)
   - Total savings: 60-150ms per reused connection

2. **Higher Throughput**
   - Less CPU time spent on connection setup
   - More requests handled per second
   - Lower system call overhead

3. **Better Resource Usage**
   - Controlled connection limits (max_size)
   - Automatic cleanup of idle connections
   - Prevents socket exhaustion

4. **Lower Memory Usage**
   - TLS sessions stay in memory (can be reused)
   - Fewer allocations for connection objects

### Reliability Improvements

1. **Error Handling**
   - Automatic connection recycling on errors
   - Pool continues working even if some connections fail
   - Graceful degradation under load

2. **Resource Protection**
   - `max_size` prevents DoS via connection explosion
   - Idle timeout prevents connection leaks
   - Per-destination pools isolate failures

## Monitoring & Observability

### Tracing

Connection pool operations are logged with `tracing`:

```rust
trace!(connection_id = conn.connection_id, "Reusing pooled connection");
trace!(connection_id = connection_id, "Creating new connection");
trace!("Connection pool full, dropping connection");
```

Enable trace logging:
```bash
RUST_LOG=trace cargo run
```

### Metrics (Future)

Potential metrics to add:

- `http_connections_created_total` - Counter
- `http_connections_reused_total` - Counter  
- `http_connection_pool_size` - Gauge (per destination)
- `http_connection_pool_acquire_duration` - Histogram
- `http_connection_idle_time` - Histogram

## Testing

### Unit Tests

Located in `src/http_client.rs`:

- `connection_key_from_url_https` - Key generation for HTTPS
- `connection_key_from_url_http` - Key generation for HTTP
- `pool_config_default_values` - Default configuration
- `parse_http_response_valid` - Response parsing
- Existing tests updated for `keep-alive` header

### Integration Tests

Located in `tests/connection_pool_test.rs`:

- Configuration validation
- Pool behavior under load
- Connection reuse verification

### Benchmark

Run the connection pool benchmark:

```bash
./benchmarks/connection_pool_bench.sh
```

This will:
1. Start server with trace logging
2. Send sequential requests to measure reuse
3. Run load tests with varying concurrency
4. Monitor TCP connection states
5. Generate summary report

Expected results:
- High connection reuse ratio (90%+ for sequential requests)
- Lower latency for reused connections
- Controlled connection count (≤ max_size under normal load)

## Implementation Details

### Concurrency Safety

- `DashMap` provides concurrent access to pools
- `async-channel` is lock-free and async-native
- `AtomicUsize`/`AtomicU64` for counters
- No mutexes in hot path

### Smol Compatibility

All async operations use smol-compatible primitives:
- `smol::net::TcpStream` - Async TCP
- `async-tls` - Async TLS built on smol
- `async-channel` - Async-native channel (smol-compatible)
- No tokio or async-std dependencies

### Error Handling

Errors are propagated and connections are recycled:

```rust
match send_request_on_connection(&mut conn, &request_bytes).await {
    Ok(response) => {
        CONNECTION_POOL.return_connection(&key, conn).await;
        Ok(extract_body_from_response(response))
    }
    Err(e) => {
        CONNECTION_POOL.recycle_connection(&key);
        Err(e)
    }
}
```

This ensures failed connections don't pollute the pool.

### Streaming Support

Streaming requests also use pooled connections:

```rust
pub async fn handle_streaming_request(
    client_stream: &mut TcpStream,
    url: &str,
    method: &str,
    path: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
    stream_config: Option<&StreamConfig>,
) -> RouterResult<()>
```

Same acquire/return/recycle pattern as regular requests.

## Limitations & Future Work

### Current Limitations

1. **No HTTP/2**: Only HTTP/1.1 with keep-alive
2. **No Connection Warming**: Connections created on-demand only
3. **Simple Expiration**: Time-based only, doesn't detect broken connections
4. **Global Config**: Same config for all destinations

### Future Enhancements

1. **HTTP/2 Multiplexing**
   - Single connection can handle multiple concurrent requests
   - Requires HTTP/2 client library

2. **Connection Health Checks**
   - Ping connections before reuse
   - Detect half-closed connections

3. **Per-Destination Config**
   - Different `max_size` for different upstreams
   - Priority pools for critical services

4. **Connection Warming**
   - Pre-establish connections to known destinations
   - Faster response to cold start

5. **Advanced Expiration**
   - Max lifetime (not just idle time)
   - Max requests per connection

6. **Pool Metrics**
   - Expose pool statistics via `/metrics`
   - Track reuse rate, queue depth, etc.

## References

- [RFC 7230 - HTTP/1.1 Message Syntax and Routing](https://tools.ietf.org/html/rfc7230#section-6.3)
- [async-channel Documentation](https://docs.rs/async-channel/)
- [rustls TLS Library](https://docs.rs/rustls/)
- [DashMap Concurrent HashMap](https://docs.rs/dashmap/)

## See Also

- `CLAUDE.md` - Project architecture overview
- `benchmarks/connection_pool_bench.sh` - Benchmark script
- `tests/connection_pool_test.rs` - Integration tests
