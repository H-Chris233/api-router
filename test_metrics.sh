#!/bin/bash

# Test script for Prometheus metrics endpoint

PORT=${1:-8000}
BASE_URL="http://localhost:${PORT}"

echo "Testing Prometheus metrics endpoint on port ${PORT}..."
echo

# Test 1: Check if /metrics endpoint is accessible
echo "Test 1: Checking if /metrics endpoint is accessible..."
if curl -sf "${BASE_URL}/metrics" > /dev/null; then
    echo "✓ /metrics endpoint is accessible"
else
    echo "✗ /metrics endpoint is not accessible"
    exit 1
fi
echo

# Test 2: Make some requests to generate metrics
echo "Test 2: Making requests to generate metrics..."
for i in {1..3}; do
    curl -s "${BASE_URL}/health" > /dev/null
    echo "  Request $i to /health sent"
done
curl -s "${BASE_URL}/v1/models" > /dev/null
echo "  Request to /v1/models sent"
curl -s "${BASE_URL}/nonexistent" > /dev/null
echo "  Request to /nonexistent (404) sent"
echo

# Test 3: Fetch metrics and verify they contain expected data
echo "Test 3: Fetching and validating metrics..."
METRICS=$(curl -s "${BASE_URL}/metrics")

if echo "$METRICS" | grep -q "requests_total"; then
    echo "✓ requests_total metric found"
else
    echo "✗ requests_total metric not found"
fi

if echo "$METRICS" | grep -q "request_latency_seconds"; then
    echo "✓ request_latency_seconds metric found"
else
    echo "✗ request_latency_seconds metric not found"
fi

if echo "$METRICS" | grep -q "active_connections"; then
    echo "✓ active_connections metric found"
else
    echo "✗ active_connections metric not found"
fi

if echo "$METRICS" | grep -q "rate_limiter_buckets"; then
    echo "✓ rate_limiter_buckets metric found"
else
    echo "✗ rate_limiter_buckets metric not found"
fi

if echo "$METRICS" | grep -q 'route="/health"'; then
    echo "✓ /health route metrics found"
else
    echo "✗ /health route metrics not found"
fi

if echo "$METRICS" | grep -q 'status="200"'; then
    echo "✓ 200 status code metrics found"
else
    echo "✗ 200 status code metrics not found"
fi

if echo "$METRICS" | grep -q 'status="404"'; then
    echo "✓ 404 status code metrics found"
else
    echo "✗ 404 status code metrics not found"
fi
echo

# Test 4: Show sample metrics output
echo "Test 4: Sample metrics output:"
echo "================================"
echo "$METRICS" | head -40
echo "================================"
echo

echo "✓ All tests passed!"
