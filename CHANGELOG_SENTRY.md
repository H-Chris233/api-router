# Sentry Integration Changelog

## Summary

Added optional Sentry error tracking and alerting integration to API Router. This enables production-grade error monitoring with zero overhead when disabled.

## Changes

### New Files

1. **`src/error_tracking.rs`** (309 lines)
   - `SentryConfig`: Configuration from environment variables
   - `init_sentry()`: Initialize Sentry client with options
   - `capture_error_with_context()`: Capture errors with rich context
   - `track_upstream_failure()`: Track repeated upstream failures for alerting
   - Automatic cleanup of old failure trackers
   - Comprehensive unit tests

2. **`SENTRY.md`** (507 lines)
   - Complete integration guide
   - Configuration examples
   - Alert setup instructions
   - Troubleshooting guide
   - Production deployment examples (Docker, Kubernetes)
   - Security best practices

3. **`.env.example`** (16 lines)
   - Example environment variable configuration
   - Documents all configurable options

4. **`tests/error_tracking_integration.rs`** (46 lines)
   - Integration tests for Sentry configuration
   - Validates default behavior
   - Tests sample rate clamping

### Modified Files

1. **`Cargo.toml`**
   - Added `sentry` (v0.34) with minimal features: backtrace, contexts, panic, rustls
   - Added `sentry-tracing` (v0.34) for automatic log integration

2. **`src/lib.rs`**
   - Exported new `error_tracking` module

3. **`src/main.rs`**
   - Initialize Sentry on startup with `SentryConfig::from_env()`
   - Keep `_sentry_guard` in scope for entire runtime

4. **`src/handlers/router.rs`**
   - Capture config load errors with context
   - Capture route handler errors with full context (request_id, API key, provider)
   - Import `capture_error_with_context` function

5. **`src/handlers/routes.rs`**
   - Track upstream failures for alerting
   - Call `track_upstream_failure()` for Upstream and TLS errors
   - Import `track_upstream_failure` function

6. **`README.md`**
   - Added Sentry to feature list
   - Documented dependencies (sentry, sentry-tracing)
   - Added configuration section for error tracking
   - Link to detailed SENTRY.md documentation

7. **`.gitignore`**
   - Added `.env` and `.env.local` to ignore list

## Features

### Error Capture
- Automatic capture of all unhandled errors
- Rich context includes:
  - Request ID (32-character hex string)
  - Route path
  - Anonymized API key (first 8 chars only)
  - Provider information (qwen, openai, anthropic, etc.)
  - Error type classification
  - Full stack traces

### Error Severity
- **Error**: Config errors, TLS failures, I/O errors
- **Warning**: Upstream API failures
- **Info**: Bad request errors (client-side issues)

### Repeated Upstream Failure Alerting
- Tracks failures per provider
- Alert threshold: 5 failures within 5 minutes
- Alert throttling: Max 1 alert per minute per provider
- Automatic cleanup of old trackers
- Emits structured logs + Sentry events

### Zero Overhead When Disabled
- No initialization if `SENTRY_DSN` not set
- All capture functions check for client existence
- No network calls
- No performance impact

## Configuration

### Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `SENTRY_DSN` | Sentry project DSN | None | Yes (to enable) |
| `SENTRY_SAMPLE_RATE` | Event sampling rate (0.0-1.0) | 1.0 | No |
| `SENTRY_ENVIRONMENT` | Environment tag | production | No |

### Example Configuration

**Enable Sentry:**
```bash
export SENTRY_DSN="https://your-key@o1234567.ingest.sentry.io/9876543"
export SENTRY_SAMPLE_RATE="1.0"
export SENTRY_ENVIRONMENT="production"
cargo run
```

**Disable Sentry (default):**
```bash
unset SENTRY_DSN
cargo run
# Output: "Sentry error tracking is disabled (no SENTRY_DSN configured)"
```

## Testing

### Unit Tests
- `cargo test error_tracking` - Run error tracking tests (7 tests)
- All tests pass, including:
  - Config loading from environment
  - Sample rate clamping
  - Upstream failure threshold detection
  - Alert throttling

### Integration Tests
- `cargo test --test error_tracking_integration` - Integration tests (4 tests)
- Validates configuration behavior
- Tests disabled state by default

### Manual Testing
```bash
# Test with Sentry disabled (default)
cargo run

# Test with invalid config to trigger error
curl -X POST http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"invalid": "request"}'
# Should log error but not send to Sentry
```

## Documentation

- **SENTRY.md**: Complete integration guide (507 lines)
  - Setup instructions
  - Alert configuration
  - Troubleshooting
  - Production examples
  - Security best practices

- **README.md**: Updated with Sentry feature overview
- **.env.example**: Example environment configuration

## Backwards Compatibility

✅ **Fully backwards compatible**
- Zero changes to existing API behavior
- No breaking changes to configuration
- Existing deployments continue working without modification
- Sentry is opt-in via environment variable

## Security Considerations

- **API keys anonymized**: Only first 8 characters logged
- **PII disabled**: `send_default_pii: false` in Sentry client
- **Request bodies not logged**: Only metadata captured
- **HTTPS only**: All Sentry communication encrypted
- **No default DSN**: Must be explicitly configured

## Performance Impact

- **When disabled (default)**: Zero overhead
  - No initialization
  - No network calls
  - No allocations

- **When enabled**: Minimal overhead
  - Async event submission
  - Background processing
  - In-memory queuing
  - Configurable sampling

## Metrics Integration

Sentry complements existing Prometheus metrics:
- **Prometheus**: Real-time metrics, dashboards, thresholds
- **Sentry**: Detailed error context, stack traces, event replay

Both systems work together:
```yaml
# Prometheus alert triggers
- alert: HighUpstreamErrorRate
  expr: rate(upstream_errors_total[5m]) > 1
  annotations:
    description: "Check Sentry for detailed error context"
```

## Future Enhancements

Possible future improvements:
- Performance monitoring (transaction tracking)
- User feedback collection
- Release tracking with git commits
- Custom error grouping rules
- Automatic screenshot capture (for web UIs)

## Migration Guide

To enable Sentry in existing deployments:

1. **Create Sentry account**: https://sentry.io
2. **Create project**: Choose "Rust" platform
3. **Copy DSN**: From project settings
4. **Set environment variable**:
   ```bash
   export SENTRY_DSN="your-dsn-here"
   ```
5. **Restart service**: `systemctl restart api-router`
6. **Verify in logs**: Look for "Sentry error tracking initialized successfully"
7. **Test error capture**: Trigger an error and check Sentry dashboard

See SENTRY.md for detailed setup instructions.

## Dependencies Added

- `sentry = { version = "0.34", default-features = false, features = ["backtrace", "contexts", "panic", "rustls"] }`
- `sentry-tracing = "0.34"`

Total binary size impact: ~400 KB (with LTO enabled)
Compile time impact: ~8 seconds additional

## Code Quality

- ✅ All existing tests pass (158 unit + 15 integration tests)
- ✅ New tests added (11 tests total for error tracking)
- ✅ Zero compiler warnings introduced
- ✅ Follows existing code style and patterns
- ✅ Comprehensive documentation
- ✅ Backwards compatible

## References

- Sentry Rust SDK: https://docs.sentry.io/platforms/rust/
- API Router Documentation: README.md, SENTRY.md
- Integration Tests: tests/error_tracking_integration.rs
