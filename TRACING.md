# Structured Logging and Tracing

API Router uses the `tracing` framework to provide structured, contextual logging with built-in latency monitoring.

## Features

- **Request ID Tracking**: Each request receives a unique UUID for end-to-end traceability
- **Latency Monitoring**: Automatic measurement of request and upstream latencies in milliseconds
- **Structured Metadata**: Rich context including client IP, route, method, status codes, and upstream provider
- **JSON Output**: Optional JSON-formatted logs for easy ingestion into log aggregation systems
- **Span-Based Tracing**: Hierarchical spans for request processing and upstream calls

## Configuration

### Log Level

Control log verbosity using the `RUST_LOG` environment variable:

```bash
# Info level (default)
export RUST_LOG=info
cargo run

# Debug level (shows detailed trace information)
export RUST_LOG=debug
cargo run

# Warn level only
export RUST_LOG=warn
cargo run

# Module-specific filtering
export RUST_LOG=api_router=debug,hyper=warn
cargo run
```

### Log Format

Choose between human-readable and JSON output using the `LOG_FORMAT` environment variable:

```bash
# Human-readable format (default)
cargo run

# JSON format (for log aggregation)
export LOG_FORMAT=json
cargo run
```

## Log Fields

### Request Span (`http_request`)

Each inbound HTTP request creates a span with the following fields:

- `request_id`: Unique UUID identifying this request
- `client_ip`: IP address of the client
- `method`: HTTP method (GET, POST, etc.)
- `route`: Request path (e.g., `/v1/chat/completions`)
- `status_code`: HTTP status code returned to client
- `latency_ms`: Total request latency in milliseconds
- `provider`: Upstream provider name (qwen, openai, anthropic, etc.)

### Upstream Request Span (`upstream_request`)

API calls to upstream providers create nested spans with:

- `request_id`: Parent request ID for correlation
- `provider`: Upstream provider (qwen, openai, anthropic, cohere, gemini)
- `upstream_latency_ms`: Time spent waiting for upstream response
- `response_size`: Size of response body in bytes (non-streaming)

### Example Logs

**Human-readable format:**
```
2024-01-15T10:23:45.123Z  INFO http_request{request_id=a1b2c3d4-e5f6-7890-abcd-ef1234567890 client_ip=192.168.1.100 method=POST route=/v1/chat/completions status_code=200 latency_ms=234.56}: Request completed successfully provider=qwen
```

**JSON format:**
```json
{
  "timestamp": "2024-01-15T10:23:45.123Z",
  "level": "INFO",
  "fields": {
    "message": "Request completed successfully"
  },
  "target": "api_router::handlers::router",
  "span": {
    "name": "http_request",
    "request_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "client_ip": "192.168.1.100",
    "method": "POST",
    "route": "/v1/chat/completions",
    "status_code": 200,
    "latency_ms": 234.56
  },
  "provider": "qwen"
}
```

## Monitoring and Observability

### Latency Metrics

Monitor request latencies by parsing the `latency_ms` and `upstream_latency_ms` fields:

```bash
# Example: Extract latency metrics from JSON logs
cat logs.json | jq '.span.latency_ms' | awk '{sum+=$1; count++} END {print "Avg latency:", sum/count, "ms"}'
```

### Request Tracking

Track a specific request across log entries using the `request_id`:

```bash
# Follow a specific request through the system
grep "a1b2c3d4-e5f6-7890-abcd-ef1234567890" api-router.log
```

### Status Code Distribution

Monitor HTTP status codes for errors:

```bash
# Count status codes from JSON logs
cat logs.json | jq -r '.span.status_code' | sort | uniq -c
```

## Integration with Observability Platforms

### Datadog

```bash
export LOG_FORMAT=json
cargo run | datadog-agent
```

### Elasticsearch / Logstash

```bash
export LOG_FORMAT=json
cargo run 2>&1 | logstash -f logstash.conf
```

### Grafana Loki

```bash
export LOG_FORMAT=json
cargo run 2>&1 | promtail --config.file=promtail-config.yaml
```

## Performance Considerations

- Structured logging adds minimal overhead (~5-10 microseconds per log statement)
- JSON formatting is slightly more expensive than human-readable format
- Debug-level logs include more details but may impact throughput at very high request rates
- Use `info` level in production for optimal balance

## Troubleshooting

### No logs appearing

Ensure `RUST_LOG` is set:
```bash
export RUST_LOG=api_router=info
```

### JSON parsing errors

Verify `LOG_FORMAT` is set correctly:
```bash
export LOG_FORMAT=json  # lowercase
```

### Missing fields in logs

Some fields are only populated for specific routes or conditions:
- `upstream_latency_ms`: Only for proxied API calls
- `provider`: Only for routes that forward to upstream APIs
- `status_code`: Populated at end of request processing

## Development

### Running with debug logs

```bash
RUST_LOG=debug cargo run
```

### Testing log capture

See `src/tracing_tests.rs` for examples of capturing and validating log output in tests.
