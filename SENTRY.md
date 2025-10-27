# Sentry Error Tracking & Alerting

API Router supports optional integration with Sentry for centralized error tracking and alerting. When enabled, the service automatically captures unhandled errors, high-severity logs, and repeated upstream failures with rich context.

## Features

### Error Capture with Context
- **Request tracking**: Each error is tagged with request ID, route, and HTTP method
- **Provider information**: Upstream provider identification (qwen, openai, anthropic, etc.)
- **API key anonymization**: Only the first 8 characters of API keys are logged
- **Error classification**: Errors are categorized by type (upstream, TLS, config, etc.)
- **Stack traces**: Full backtraces for debugging production issues

### Severity Levels
- **Error**: Configuration errors, TLS failures, general I/O errors
- **Warning**: Upstream API failures, rate limiting issues
- **Info**: Bad request errors (client-side issues)

### Repeated Upstream Failure Alerting
The service automatically tracks upstream failures and generates alerts when:
- **Threshold**: 5 or more failures from the same provider
- **Time window**: Within a 5-minute window
- **Alert throttling**: Maximum one alert per minute per provider
- **Auto-cleanup**: Old failure trackers are automatically cleaned up

## Configuration

### Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `SENTRY_DSN` | Sentry Data Source Name from your project settings | None | Yes (to enable) |
| `SENTRY_SAMPLE_RATE` | Percentage of events to send (0.0 to 1.0) | 1.0 | No |
| `SENTRY_ENVIRONMENT` | Environment name (production, staging, development) | production | No |

### Enabling Sentry

1. **Create a Sentry account** at https://sentry.io (free tier available)

2. **Create a new project** in Sentry:
   - Choose "Rust" as the platform
   - Copy the DSN from project settings

3. **Set environment variables**:
   ```bash
   export SENTRY_DSN="https://your-key@o1234567.ingest.sentry.io/9876543"
   export SENTRY_SAMPLE_RATE="1.0"
   export SENTRY_ENVIRONMENT="production"
   ```

4. **Start the service**:
   ```bash
   cargo run
   ```

5. **Verify initialization**: Look for the log message:
   ```
   Initializing Sentry error tracking
   Sentry error tracking initialized successfully
   ```

### Disabling Sentry

Simply omit the `SENTRY_DSN` environment variable. The service will run normally with zero overhead:

```bash
unset SENTRY_DSN
cargo run
```

You'll see: `Sentry error tracking is disabled (no SENTRY_DSN configured)`

## Sample Rate Configuration

Control the volume of events sent to Sentry:

```bash
# Send all events (default)
export SENTRY_SAMPLE_RATE="1.0"

# Send 50% of events
export SENTRY_SAMPLE_RATE="0.5"

# Send 10% of events (recommended for high-traffic production)
export SENTRY_SAMPLE_RATE="0.1"
```

## Environment Configuration

Tag events by environment for better filtering in Sentry:

```bash
# Production
export SENTRY_ENVIRONMENT="production"

# Staging
export SENTRY_ENVIRONMENT="staging"

# Development
export SENTRY_ENVIRONMENT="development"
```

## Captured Error Context

Each error captured by Sentry includes:

### Tags (for filtering)
- `request_id`: Unique UUID for the request
- `route`: API endpoint path (e.g., `/v1/chat/completions`)
- `error_type`: Classification (upstream_error, tls_error, etc.)
- `provider`: Upstream provider (qwen, openai, anthropic, etc.)
- `alert_type`: For alerts (e.g., `repeated_upstream_failures`)

### Extra Context
- `api_key_prefix`: First 8 characters of the API key (e.g., `sk-proj...`)
- `error_message`: Full error message
- `error_details`: Additional error information

### Automatic Context
- Stack traces with source code context
- Release version (from Cargo.toml)
- Server name and OS information
- Timestamp and timezone

## Alert Configuration in Sentry

### Setting Up Upstream Failure Alerts

1. **Navigate to your project** in Sentry dashboard

2. **Go to Alerts** → **Create Alert**

3. **Configure alert rule**:
   - **Type**: Issues
   - **Conditions**:
     - `alert_type` equals `repeated_upstream_failures`
     - OR `error_type` equals `upstream_error` with frequency > 10 in 1 hour
   - **Actions**:
     - Send notification to Slack/email/PagerDuty
     - Create incident in incident management tool

4. **Example alert rules**:

   **High-priority upstream failures**:
   ```
   WHEN tag alert_type equals "repeated_upstream_failures"
   THEN send notification to #ops-critical
   ```

   **Provider-specific alerts**:
   ```
   WHEN tag error_type equals "upstream_error"
     AND tag provider equals "qwen"
     AND event count > 5 in 5 minutes
   THEN send notification to #qwen-alerts
   ```

   **TLS/connectivity issues**:
   ```
   WHEN tag error_type equals "tls_error"
     AND event count > 2 in 10 minutes
   THEN send notification to #security-alerts
   ```

### Recommended Alert Integrations

- **Slack**: Get instant notifications in dedicated channels
- **PagerDuty**: Page on-call engineers for critical issues
- **Email**: Digest for lower-priority issues
- **OpsGenie**: Incident management and escalation
- **Microsoft Teams**: Enterprise team notifications

## Verifying Sentry Integration

### Test Error Capture

1. **Trigger a test error** by sending a malformed request:
   ```bash
   curl -X POST http://localhost:8000/v1/chat/completions \
     -H "Content-Type: application/json" \
     -d '{"invalid": "request"}'
   ```

2. **Check Sentry dashboard**: You should see a new issue with:
   - Error message: "JSON error: missing field `model`"
   - Tags: request_id, route, error_type
   - Full context and stack trace

### Test Upstream Failure Tracking

1. **Configure an invalid upstream** in `transformer/test.json`:
   ```json
   {
     "baseUrl": "https://nonexistent-api.invalid",
     ...
   }
   ```

2. **Send multiple requests** to trigger threshold:
   ```bash
   for i in {1..6}; do
     curl -X POST http://localhost:8000/v1/chat/completions \
       -H "Authorization: Bearer test-key" \
       -H "Content-Type: application/json" \
       -d '{"model": "test", "messages": [{"role": "user", "content": "hi"}]}'
   done
   ```

3. **Check logs** for alert message:
   ```
   ALERT: Repeated upstream failures detected
   ```

4. **Check Sentry** for alert event with tag `alert_type=repeated_upstream_failures`

## Metrics and Monitoring

### Prometheus Metrics

The `/metrics` endpoint exposes upstream error metrics:

```prometheus
# Total upstream errors by type
upstream_errors_total{error_type="upstream_error"} 42
upstream_errors_total{error_type="tls_error"} 3

# Request success/failure rates
requests_total{route="/v1/chat/completions",method="POST",status="500"} 15
```

### Combining Sentry with Prometheus

Use both systems in tandem:
- **Prometheus**: Real-time metrics, dashboards, threshold alerts
- **Sentry**: Detailed error context, stack traces, request replay

**Example Prometheus alert** (pair with Sentry for investigation):
```yaml
- alert: HighUpstreamErrorRate
  expr: rate(upstream_errors_total[5m]) > 1
  annotations:
    summary: "High upstream error rate detected"
    description: "Check Sentry for detailed error context"
```

## Integration with Structured Logging

Sentry automatically integrates with the existing `tracing` infrastructure via `sentry-tracing`. High-severity logs are sent to Sentry as breadcrumbs:

```rust
use tracing::error;

error!(
    provider = "qwen",
    request_id = "abc-123",
    "Upstream API request failed"
);
// ↑ Automatically captured in Sentry as breadcrumb
```

## Performance Considerations

### Zero Overhead When Disabled
When `SENTRY_DSN` is not set, there is **zero runtime overhead**:
- No initialization
- No network calls
- No performance impact

### Minimal Overhead When Enabled
- **Async submission**: Events are sent asynchronously
- **Batching**: Multiple events can be batched
- **Sampling**: Use `SENTRY_SAMPLE_RATE` to reduce volume in high-traffic scenarios
- **In-memory queue**: Events are queued and sent in background

### Production Recommendations

**High-traffic production** (>1000 req/min):
```bash
export SENTRY_SAMPLE_RATE="0.1"  # 10% sampling
```

**Medium-traffic production** (100-1000 req/min):
```bash
export SENTRY_SAMPLE_RATE="0.5"  # 50% sampling
```

**Low-traffic or staging**:
```bash
export SENTRY_SAMPLE_RATE="1.0"  # 100% sampling
```

## Troubleshooting

### Sentry Not Capturing Events

1. **Check DSN is set**:
   ```bash
   echo $SENTRY_DSN
   ```

2. **Check initialization logs**:
   ```bash
   cargo run 2>&1 | grep -i sentry
   ```

3. **Verify network connectivity**:
   ```bash
   curl -I https://sentry.io
   ```

4. **Check Sentry project status** in dashboard

5. **Verify rate limits**: Check if you've exceeded your Sentry plan quota

### Missing Context in Events

- **Request ID**: Ensure errors occur after request parsing
- **Provider info**: Verify config file has valid `baseUrl`
- **API key**: Ensure `Authorization` header is present

### Alert Not Triggering

1. **Check alert rule conditions** in Sentry dashboard
2. **Verify tags are present** on captured events
3. **Check notification integrations** are configured
4. **Test alert rule** using Sentry's test feature

## Security Best Practices

### Data Privacy

- **API keys are anonymized**: Only first 8 characters logged
- **PII disabled**: `send_default_pii: false` in client options
- **Request bodies not logged**: Only metadata is captured
- **GDPR compliance**: Configure data retention in Sentry settings

### Access Control

- **Limit Sentry access**: Use role-based access control
- **Rotate DSNs**: Rotate DSNs if compromised
- **Use environment-specific DSNs**: Separate production/staging

### Network Security

- **HTTPS only**: Sentry communication is encrypted
- **Certificate validation**: Uses `rustls` with webpki-roots
- **No proxy required**: Direct HTTPS connection

## Example Production Setup

### Docker Compose

```yaml
version: '3.8'
services:
  api-router:
    image: api-router:latest
    environment:
      - SENTRY_DSN=${SENTRY_DSN}
      - SENTRY_SAMPLE_RATE=0.2
      - SENTRY_ENVIRONMENT=production
      - DEFAULT_API_KEY=${API_KEY}
      - RUST_LOG=info
      - LOG_FORMAT=json
    ports:
      - "8000:8000"
    restart: unless-stopped
```

### Kubernetes

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: api-router-secrets
type: Opaque
stringData:
  sentry-dsn: "https://your-key@o1234567.ingest.sentry.io/9876543"
  api-key: "your-api-key"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api-router
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: api-router
        image: api-router:latest
        env:
        - name: SENTRY_DSN
          valueFrom:
            secretKeyRef:
              name: api-router-secrets
              key: sentry-dsn
        - name: SENTRY_SAMPLE_RATE
          value: "0.2"
        - name: SENTRY_ENVIRONMENT
          value: "production"
        - name: DEFAULT_API_KEY
          valueFrom:
            secretKeyRef:
              name: api-router-secrets
              key: api-key
```

## Further Reading

- [Sentry Rust SDK Documentation](https://docs.sentry.io/platforms/rust/)
- [Sentry Alert Configuration](https://docs.sentry.io/product/alerts/)
- [Sentry Performance Monitoring](https://docs.sentry.io/product/performance/)
- [API Router Metrics Documentation](README.md#prometheus-指标集成)
