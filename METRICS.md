# Prometheus Metrics

API Router exposes Prometheus-compatible performance metrics at the `/metrics` endpoint.

## Endpoint

**GET /metrics**

Returns metrics in Prometheus text format (version 0.0.4).

```bash
curl http://localhost:8000/metrics
```

## Available Metrics

### Counters

#### `requests_total`
Total number of HTTP requests received.

**Labels:**
- `route`: The request path (e.g., `/v1/chat/completions`, `/health`, `/metrics`)
- `method`: HTTP method (e.g., `GET`, `POST`)
- `status`: HTTP status code (e.g., `200`, `404`, `429`, `500`)

**Example:**
```
requests_total{route="/v1/chat/completions",method="POST",status="200"} 150
requests_total{route="/health",method="GET",status="200"} 42
requests_total{route="/v1/chat/completions",method="POST",status="429"} 5
```

#### `upstream_errors_total`
Total number of upstream errors encountered.

**Labels:**
- `error_type`: Type of error (e.g., `upstream_error`, `io_error`, `tls_error`, `json_error`, `bad_request`, `url_error`, `config_read_error`, `config_parse_error`)

**Example:**
```
upstream_errors_total{error_type="upstream_error"} 3
upstream_errors_total{error_type="tls_error"} 1
```

### Histograms

#### `request_latency_seconds`
Request latency distribution in seconds.

**Labels:**
- `route`: The request path

**Buckets:** 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0

**Example:**
```
request_latency_seconds_bucket{route="/v1/chat/completions",le="0.001"} 0
request_latency_seconds_bucket{route="/v1/chat/completions",le="0.1"} 120
request_latency_seconds_bucket{route="/v1/chat/completions",le="1.0"} 145
request_latency_seconds_sum{route="/v1/chat/completions"} 87.3
request_latency_seconds_count{route="/v1/chat/completions"} 150
```

### Gauges

#### `active_connections`
Number of currently active connections being handled.

**Example:**
```
active_connections 5
```

#### `rate_limiter_buckets`
Number of active rate limiter token buckets (one per route-client pair).

**Example:**
```
rate_limiter_buckets 23
```

## Integration with Prometheus

Add the following configuration to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'api-router'
    scrape_interval: 15s
    static_configs:
      - targets: ['localhost:8000']
    metrics_path: '/metrics'
```

## Example Queries

### Request rate by route
```promql
rate(requests_total[5m])
```

### Error rate
```promql
rate(requests_total{status=~"5.."}[5m])
```

### Average latency by route
```promql
rate(request_latency_seconds_sum[5m]) / rate(request_latency_seconds_count[5m])
```

### 95th percentile latency
```promql
histogram_quantile(0.95, rate(request_latency_seconds_bucket[5m]))
```

### Success rate
```promql
sum(rate(requests_total{status=~"2.."}[5m])) / sum(rate(requests_total[5m]))
```

### Rate limit rejections
```promql
rate(requests_total{status="429"}[5m])
```

### Upstream errors by type
```promql
rate(upstream_errors_total[5m])
```

## Grafana Dashboard

Example dashboard panels:

1. **Request Rate**: `sum(rate(requests_total[5m])) by (route)`
2. **Error Rate**: `sum(rate(requests_total{status=~"[45].."}[5m]))`
3. **Active Connections**: `active_connections`
4. **P95 Latency**: `histogram_quantile(0.95, sum(rate(request_latency_seconds_bucket[5m])) by (route, le))`
5. **Rate Limiter State**: `rate_limiter_buckets`
6. **Status Code Distribution**: `sum(rate(requests_total[5m])) by (status)`

## Performance Impact

Metrics collection has minimal performance overhead:
- Counters: ~10ns per increment
- Histograms: ~100ns per observation
- Gauges: ~10ns per set

The `/metrics` endpoint itself is lightweight and typically responds in <1ms.
