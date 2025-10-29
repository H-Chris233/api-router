# SSE Streaming Improvements

This document describes the enhanced Server-Sent Events (SSE) streaming implementation in the API Router.

## Overview

The streaming implementation has been improved with the following features:
- **Incremental read/write** with configurable buffer sizes
- **Backpressure handling** to avoid overwhelming slow clients
- **Heartbeat keep-alive** support to maintain connections during slow upstream responses
- **Graceful shutdown** on client disconnect
- **Memory efficient** processing without accumulating entire responses

## Configuration

### Global Stream Configuration

You can configure streaming behavior globally in your configuration file:

```json
{
  "baseUrl": "https://api.example.com",
  "streamConfig": {
    "bufferSize": 8192,
    "heartbeatIntervalSecs": 30
  }
}
```

### Endpoint-Specific Configuration

You can override global settings for specific endpoints:

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

## Configuration Parameters

### `bufferSize`
- **Type**: Integer
- **Default**: 8192 (8 KB)
- **Description**: The size of the internal buffer used for reading from upstream and writing to clients. Smaller buffers use less memory but may require more system calls. Larger buffers can improve throughput for high-bandwidth streams.
- **Range**: Recommended between 1024 (1 KB) and 65536 (64 KB)

### `heartbeatIntervalSecs`
- **Type**: Integer (seconds)
- **Default**: 30
- **Description**: The interval at which heartbeat messages are sent to keep the connection alive when no data is flowing from upstream. Heartbeats are sent as SSE comments (`: heartbeat\n\n`) which are ignored by standard SSE clients but keep the connection active.
- **Range**: Recommended between 5 and 300 seconds

## Features

### Incremental Read/Write

The implementation uses a streaming approach that:
- Reads data from upstream in chunks (up to `bufferSize`)
- Immediately writes received chunks to the client
- Does not accumulate the entire response in memory
- Processes data as it arrives

### Backpressure Handling

The implementation includes proper backpressure:
- Waits for client write operations to complete before reading more data
- Prevents memory buildup when clients are slower than upstream
- Uses `flush()` after each write to ensure data is sent immediately

### Heartbeat Keep-Alive

To prevent connection timeouts during slow upstream responses:
- Tracks the time since last activity (read or write)
- Sends heartbeat comments when idle time exceeds `heartbeatIntervalSecs`
- Heartbeats are SSE comments that don't affect the data stream
- Format: `: heartbeat\n\n`

### Graceful Client Disconnect

The implementation handles client disconnects gracefully:
- Detects `BrokenPipe` and `ConnectionReset` errors during writes
- Stops reading from upstream when client disconnects
- Cleans up resources properly
- Logs the disconnect event for monitoring

### Error Handling

The streaming implementation handles various error conditions:
- **Upstream connection loss**: Stops gracefully, logs warning
- **Client disconnect**: Stops reading from upstream, cleans up
- **Write failures**: Distinguishes between client disconnect and other errors
- **Read timeouts**: Sends heartbeats to maintain connection

## Performance Considerations

### Memory Usage

- Memory usage is bounded by the buffer size
- Typical memory footprint per streaming connection: ~8-16 KB
- No accumulation of response data in memory
- Suitable for long-running streams and large responses

### Throughput

- Buffer size affects throughput:
  - Larger buffers (16 KB - 64 KB): Better for high-bandwidth streams
  - Smaller buffers (1 KB - 4 KB): Better for memory-constrained environments
- Flush after each write ensures low latency

### CPU Usage

- Minimal CPU overhead
- No data transformation or buffering beyond the configured buffer
- Efficient async I/O using smol runtime

## Testing

The implementation includes comprehensive tests:

1. **Chunk ordering**: Verifies chunks arrive in correct order
2. **Heartbeat delivery**: Tests heartbeat messages during slow upstream
3. **Custom buffer sizes**: Validates buffer size configuration
4. **Client disconnect**: Tests graceful shutdown on early disconnect
5. **Endpoint-specific config**: Verifies endpoint overrides work
6. **Large chunks**: Tests backpressure with chunks larger than buffer
7. **Completions endpoint**: Validates streaming works for both chat and completions

Run tests with:
```bash
cargo test --test streaming_tests
```

## Examples

### Basic Streaming Request

```bash
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d '{
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Hello"}],
    "stream": true
  }'
```

### Expected Response Format

```
HTTP/1.1 200 OK
Content-Type: text/event-stream
Cache-Control: no-cache
Connection: keep-alive
X-Accel-Buffering: no

data: {"id":"chunk-1","choices":[{"delta":{"content":"Hello"}}]}

data: {"id":"chunk-2","choices":[{"delta":{"content":" there"}}]}

: heartbeat

data: {"id":"chunk-3","choices":[{"delta":{"content":"!"}}]}

data: [DONE]

```

## Implementation Details

The streaming is implemented in `src/http_client.rs`:

- `handle_streaming_request()`: Main entry point, handles HTTPS/HTTP setup
- `stream_with_backpressure_and_heartbeat()`: Core streaming logic with backpressure and heartbeat support

Key implementation details:
- Uses `smol::future::or` for timeout-based heartbeats
- Generic over reader and writer types (works with TLS and TCP streams)
- Tracks last activity time to determine when to send heartbeats
- Proper error handling with graceful shutdown paths
