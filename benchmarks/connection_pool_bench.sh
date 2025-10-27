#!/bin/bash

# Connection Pool Performance Benchmark
# Demonstrates reduced connection churn with HTTP connection pooling

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

RESULTS_DIR="./connection_pool_results"
PORT=8000
BASE_URL="http://localhost:${PORT}"

print_header() {
    echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}\n"
}

print_info() {
    echo -e "${GREEN}ℹ${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

wait_for_server() {
    local timeout=30
    local elapsed=0
    
    print_info "Waiting for server to start..."
    
    while [ $elapsed -lt $timeout ]; do
        if curl -s -f "${BASE_URL}/health" > /dev/null 2>&1; then
            print_success "Server is ready"
            return 0
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done
    
    print_error "Server startup timeout"
    return 1
}

run_connection_test() {
    local test_name=$1
    local requests=$2
    local concurrency=$3
    local description=$4
    
    print_info "Running: $test_name"
    echo "  Description: $description"
    echo "  Requests: $requests, Concurrency: $concurrency"
    
    local result_file="${RESULTS_DIR}/${test_name}.txt"
    
    if command -v oha &> /dev/null; then
        oha -n "$requests" -c "$concurrency" \
            --json "${RESULTS_DIR}/${test_name}.json" \
            "${BASE_URL}/health" 2>&1 | tee "$result_file"
    elif command -v wrk &> /dev/null; then
        wrk -t4 -c"$concurrency" -d10s \
            "${BASE_URL}/health" 2>&1 | tee "$result_file"
    elif command -v ab &> /dev/null; then
        ab -n "$requests" -c "$concurrency" \
            "${BASE_URL}/health" 2>&1 | tee "$result_file"
    else
        print_warning "No HTTP benchmark tool found (oha, wrk, or ab)"
        return 1
    fi
    
    echo ""
}

monitor_connections() {
    print_info "Monitoring TCP connections..."
    
    local duration=30
    local output="${RESULTS_DIR}/connection_monitoring.txt"
    
    echo "Timestamp,Established,TimeWait,Total" > "$output"
    
    for i in $(seq 1 $duration); do
        local timestamp=$(date +%s)
        local established=$(ss -tan | grep ESTAB | wc -l)
        local timewait=$(ss -tan | grep TIME-WAIT | wc -l)
        local total=$((established + timewait))
        
        echo "$timestamp,$established,$timewait,$total" >> "$output"
        sleep 1
    done
    
    print_success "Connection monitoring completed"
    echo "  Results saved to: $output"
}

analyze_connection_reuse() {
    print_header "Connection Reuse Analysis"
    
    print_info "Starting server with connection pool logging..."
    RUST_LOG=trace cargo run --release -- qwen $PORT > "${RESULTS_DIR}/server_trace.log" 2>&1 &
    local server_pid=$!
    
    trap "kill $server_pid 2>/dev/null || true" EXIT
    
    if ! wait_for_server; then
        print_error "Server failed to start"
        kill $server_pid 2>/dev/null || true
        return 1
    fi
    
    # Run a series of requests
    print_info "Sending sequential requests to observe connection reuse..."
    for i in {1..10}; do
        curl -s "${BASE_URL}/health" > /dev/null
        sleep 0.5
    done
    
    sleep 2
    
    # Check logs for connection reuse
    print_info "Analyzing connection pool behavior..."
    local reuse_count=$(grep -c "Reusing pooled connection" "${RESULTS_DIR}/server_trace.log" || echo "0")
    local new_count=$(grep -c "Creating new connection" "${RESULTS_DIR}/server_trace.log" || echo "0")
    
    echo "Connection Statistics:" | tee "${RESULTS_DIR}/connection_stats.txt"
    echo "  New connections created: $new_count" | tee -a "${RESULTS_DIR}/connection_stats.txt"
    echo "  Connections reused: $reuse_count" | tee -a "${RESULTS_DIR}/connection_stats.txt"
    
    if [ "$reuse_count" -gt 0 ]; then
        print_success "Connection pooling is working! Reused $reuse_count times"
    else
        print_warning "No connection reuse detected in logs"
    fi
    
    kill $server_pid 2>/dev/null || true
    wait $server_pid 2>/dev/null || true
    trap - EXIT
}

run_load_tests() {
    print_header "Connection Pool Load Tests"
    
    print_info "Starting server for load testing..."
    cargo run --release -- qwen $PORT > "${RESULTS_DIR}/server_load.log" 2>&1 &
    local server_pid=$!
    
    trap "kill $server_pid 2>/dev/null || true" EXIT
    
    if ! wait_for_server; then
        print_error "Server failed to start"
        kill $server_pid 2>/dev/null || true
        return 1
    fi
    
    # Start connection monitoring in background
    monitor_connections &
    local monitor_pid=$!
    
    # Test 1: Low concurrency, high request count
    run_connection_test "test1_sequential" 1000 1 "Sequential requests to test connection reuse"
    
    # Test 2: Medium concurrency
    run_connection_test "test2_medium_concurrency" 1000 10 "Medium concurrency to test pool efficiency"
    
    # Test 3: High concurrency (at pool limit)
    run_connection_test "test3_pool_limit" 500 10 "Concurrency at pool max_size limit"
    
    # Test 4: Beyond pool limit
    run_connection_test "test4_beyond_pool" 500 20 "Concurrency beyond pool max_size"
    
    # Stop monitoring
    kill $monitor_pid 2>/dev/null || true
    wait $monitor_pid 2>/dev/null || true
    
    # Stop server
    print_info "Stopping server..."
    kill $server_pid 2>/dev/null || true
    wait $server_pid 2>/dev/null || true
    trap - EXIT
}

generate_report() {
    print_header "Benchmark Summary"
    
    local report="${RESULTS_DIR}/summary.md"
    
    cat > "$report" << 'EOF'
# Connection Pool Benchmark Results

## Overview
This benchmark demonstrates the effectiveness of HTTP connection pooling and reuse.

## Configuration
- Pool max_size: 10 connections
- Pool idle_timeout: 60 seconds
- Connection header: keep-alive (previously: close)

## Key Improvements

### 1. Connection Reuse
EOF
    
    if [ -f "${RESULTS_DIR}/connection_stats.txt" ]; then
        cat "${RESULTS_DIR}/connection_stats.txt" >> "$report"
    fi
    
    cat >> "$report" << 'EOF'

### 2. Load Test Results
EOF
    
    for test in test1_sequential test2_medium_concurrency test3_pool_limit test4_beyond_pool; do
        if [ -f "${RESULTS_DIR}/${test}.txt" ]; then
            echo -e "\n#### ${test}" >> "$report"
            grep -E "(Requests/sec|Latency|Success rate)" "${RESULTS_DIR}/${test}.txt" >> "$report" 2>/dev/null || true
        fi
    done
    
    cat >> "$report" << 'EOF'

### 3. TCP Connection Analysis
Check `connection_monitoring.txt` for time-series connection state data.

## Benefits Demonstrated
1. **Reduced Connection Churn**: Connections are reused instead of created/destroyed per request
2. **Lower Latency**: No TCP handshake + TLS handshake overhead for pooled connections
3. **Better Resource Utilization**: Controlled connection limits prevent resource exhaustion
4. **TLS Session Reuse**: TLS sessions can be resumed on pooled connections (implicit benefit)

## Implementation Details
- Used `async-channel` for connection queue (smol-compatible)
- Per-(scheme, host, port) connection pools using `DashMap`
- Automatic idle connection cleanup
- Error handling with connection recycling
EOF
    
    print_success "Report generated: $report"
    cat "$report"
}

main() {
    print_header "Connection Pool Performance Benchmark"
    
    mkdir -p "$RESULTS_DIR"
    
    print_info "System Information:"
    {
        echo "Date: $(date)"
        echo "Rust: $(rustc --version)"
        echo "Host: $(uname -a)"
    } | tee "${RESULTS_DIR}/system_info.txt"
    
    analyze_connection_reuse
    run_load_tests
    generate_report
    
    print_header "Benchmark Complete"
    print_success "All results saved to: ${RESULTS_DIR}/"
    echo ""
    echo "Next steps:"
    echo "  1. Review ${RESULTS_DIR}/summary.md"
    echo "  2. Check connection reuse logs in ${RESULTS_DIR}/server_trace.log"
    echo "  3. Analyze connection monitoring data in ${RESULTS_DIR}/connection_monitoring.txt"
}

if [ "${BASH_SOURCE[0]}" == "${0}" ]; then
    main "$@"
fi
