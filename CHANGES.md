# SSE Streaming Performance and Memory Usage Improvements

## Summary

This document describes the changes made to improve SSE streaming performance, memory usage, and reliability in the API Router.

## Changes Made

### 1. Configuration Extensions

**File**: `src/config.rs`

Added new configuration structures for streaming:

```rust
#[derive(Debug, Clone, Deserialize, Default)]
pub struct StreamConfig {
    #[serde(rename = "bufferSize", default = "default_buffer_size")]
    pub buffer_size: usize,
    #[serde(rename = "heartbeatIntervalSecs", default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: u64,
}
```

- `bufferSize`: Configurable buffer size (default: 8192 bytes)
- `heartbeatIntervalSecs`: Heartbeat interval in seconds (default: 30)

Extended `ApiConfig` and `EndpointConfig` to support:
- Global `streamConfig` in `ApiConfig`
- Per-endpoint `streamConfig` in `EndpointConfig`

### 2. HTTP Client Improvements

**File**: `src/http_client.rs`

Completely rewrote `handle_streaming_request()` with:

#### Incremental Read/Write
- Uses configurable buffer sizes instead of hardcoded 4096 bytes
- Reads data in chunks and immediately writes to client
- No accumulation of entire response in memory

#### Backpressure Support
- Waits for client write operations to complete before reading more data
- Prevents memory buildup when clients are slow
- Flushes after each write to ensure immediate delivery

#### Heartbeat Keep-Alive
- Tracks last activity time
- Sends SSE comment heartbeats (`: heartbeat\n\n`) when idle
- Configurable heartbeat interval
- Keeps connections alive during slow upstream responses

#### Graceful Shutdown
- Detects client disconnects (`BrokenPipe`, `ConnectionReset`)
- Stops reading from upstream when client disconnects
- Proper cleanup and resource management
- Logs disconnect events for monitoring

#### Implementation Details
- New function: `stream_with_backpressure_and_heartbeat()`
- Generic over reader and writer types
- Works with both TLS and plain TCP streams
- Uses `smol::future::or` for timeout-based heartbeats
- Proper error handling with graceful shutdown paths

### 3. Handler Updates

**File**: `src/handlers.rs`

Updated streaming request handlers to:
- Pass `StreamConfig` to `handle_streaming_request()`
- Support endpoint-specific configuration overriding global config
- Apply to both `/v1/chat/completions` and `/v1/completions` endpoints

### 4. Comprehensive Tests

**File**: `tests/streaming_tests.rs` (NEW)

Added 7 comprehensive integration tests:

1. **`streaming_preserves_chunk_order_and_content`**: Validates chunks arrive in correct order
2. **`streaming_sends_heartbeats_on_slow_upstream`**: Tests heartbeat delivery during slow upstream
3. **`streaming_uses_custom_buffer_size`**: Validates buffer size configuration works
4. **`streaming_handles_client_early_disconnect_gracefully`**: Tests graceful shutdown on client disconnect
5. **`streaming_supports_endpoint_specific_config`**: Verifies endpoint config overrides global config
6. **`streaming_handles_large_chunks_with_backpressure`**: Tests backpressure with chunks larger than buffer
7. **`streaming_completions_endpoint_works`**: Validates streaming works for completions endpoint

All tests use mock upstream providers to:
- Control chunk timing and delays
- Validate ordering and content
- Test resource cleanup
- Verify configuration handling

### 5. Test Infrastructure Improvements

**File**: `tests/common/fixtures.rs`

Added `from_value()` method to `ConfigFixture` to support creating fixtures from JSON values, enabling dynamic configuration in tests.

### 6. Documentation

**Files**: `STREAMING.md` (NEW), `README.md` (UPDATED)

Created comprehensive documentation covering:
- Configuration options and examples
- Feature descriptions
- Performance considerations
- Memory usage characteristics
- Implementation details
- Usage examples
- Testing guide

Updated main README with:
- Highlighted streaming features in feature list
- Added streaming configuration section
- Link to detailed streaming documentation

## Benefits

### Performance
- **Lower latency**: Immediate forwarding of chunks without buffering
- **Better throughput**: Configurable buffer sizes for different scenarios
- **Efficient I/O**: Minimal data copying and transformation

### Memory Usage
- **Bounded memory**: Memory usage limited by buffer size (~8-16 KB per connection)
- **No accumulation**: Doesn't accumulate entire responses
- **Scalable**: Suitable for long-running streams and large responses

### Reliability
- **Connection stability**: Heartbeats prevent timeouts on slow upstreams
- **Graceful degradation**: Proper handling of client disconnects
- **Error resilience**: Distinguishes between different error types
- **Resource cleanup**: Proper cleanup on all exit paths

### Configurability
- **Global defaults**: Set defaults for all endpoints
- **Endpoint overrides**: Fine-tune behavior per endpoint
- **Runtime tuning**: Adjust without code changes
- **Backward compatible**: Works with existing configs (uses defaults)

## Testing

All tests pass:
- 19 unit tests (existing functionality)
- 4 integration tests (existing functionality)
- 7 new streaming-specific integration tests

```bash
# Run all tests
cargo test

# Run streaming tests only
cargo test --test streaming_tests

# Run with verbose output
cargo test -- --nocapture
```

## Backward Compatibility

All changes are backward compatible:
- New configuration fields are optional with sensible defaults
- Existing configurations continue to work unchanged
- No breaking changes to API or behavior
- Only enhancements to streaming performance and reliability

## Configuration Example

```json
{
  "baseUrl": "https://api.example.com",
  "streamConfig": {
    "bufferSize": 8192,
    "heartbeatIntervalSecs": 30
  },
  "endpoints": {
    "/v1/chat/completions": {
      "streamSupport": true,
      "streamConfig": {
        "bufferSize": 4096,
        "heartbeatIntervalSecs": 15
      }
    }
  }
}
```

## Future Improvements

Potential future enhancements:
- Adaptive buffer sizing based on connection speed
- Connection pooling for upstream requests
- Compression support for streaming responses
- Metrics and monitoring integration
- Advanced error recovery strategies
