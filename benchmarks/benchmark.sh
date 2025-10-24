#!/bin/bash

# 异步运行时性能基准测试脚本

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

RESULTS_DIR="./benchmark_results"
PORT=8000
BASE_URL="http://localhost:${PORT}"
DURATION=10
CONNECTIONS=50

print_header() {
    echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}\n"
}

print_info() {
    echo -e "${GREEN}ℹ${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

check_command() {
    if ! command -v "$1" &> /dev/null; then
        print_error "未找到命令: $1"
        return 1
    fi
    return 0
}

wait_for_server() {
    local timeout=30
    local elapsed=0
    
    print_info "等待服务器启动..."
    
    while [ $elapsed -lt $timeout ]; do
        if curl -s -f "${BASE_URL}/health" > /dev/null 2>&1; then
            print_info "服务器已就绪"
            return 0
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done
    
    print_error "服务器启动超时"
    return 1
}

# 1. 编译基准测试
run_compile_benchmark() {
    print_header "1. 编译时间基准测试"
    
    local result_file="${RESULTS_DIR}/compile_bench.txt"
    
    print_info "清理编译缓存..."
    cargo clean > /dev/null 2>&1
    
    print_info "测量完整编译时间..."
    if check_command "hyperfine"; then
        hyperfine --warmup 1 --runs 3 \
            --export-json "${RESULTS_DIR}/compile_bench.json" \
            'cargo build --release' > "$result_file" 2>&1
    else
        print_warning "hyperfine 未安装，使用 time 命令"
        { time cargo build --release; } 2>&1 | tee "$result_file"
    fi
    
    print_info "测量增量编译时间..."
    touch src/main.rs
    { time cargo build --release; } 2>&1 | tee -a "$result_file"
    
    print_info "二进制大小:"
    local binary="./target/release/api-router"
    if [ -f "$binary" ]; then
        ls -lh "$binary" | awk '{print $5, $9}' | tee -a "$result_file"
        strip "$binary"
        echo -n "stripped: " | tee -a "$result_file"
        ls -lh "$binary" | awk '{print $5}' | tee -a "$result_file"
    fi
}

# 2. 运行时性能基准测试
run_runtime_benchmark() {
    print_header "2. 运行时性能基准测试"
    
    # 启动服务器
    print_info "启动 API Router..."
    cargo run --release -- qwen $PORT > "${RESULTS_DIR}/server.log" 2>&1 &
    local server_pid=$!
    
    # 确保服务器在退出时被终止
    trap "kill $server_pid 2>/dev/null || true" EXIT
    
    if ! wait_for_server; then
        print_error "服务器未能正常启动"
        kill $server_pid 2>/dev/null || true
        return 1
    fi
    
    # 场景 A: 健康检查端点（轻量级）
    print_info "场景 A: /health 端点"
    if check_command "oha"; then
        oha -z ${DURATION}s -c ${CONNECTIONS} \
            --json "${RESULTS_DIR}/health_bench.json" \
            "${BASE_URL}/health" | tee "${RESULTS_DIR}/health_bench.txt"
    elif check_command "wrk"; then
        wrk -t4 -c${CONNECTIONS} -d${DURATION}s \
            "${BASE_URL}/health" | tee "${RESULTS_DIR}/health_bench.txt"
    else
        print_warning "oha 或 wrk 未安装，使用 ab"
        if check_command "ab"; then
            ab -t ${DURATION} -c ${CONNECTIONS} \
                "${BASE_URL}/health" | tee "${RESULTS_DIR}/health_bench.txt"
        else
            print_error "未找到任何 HTTP 基准测试工具"
        fi
    fi
    
    # 场景 B: 模型列表（简单 JSON）
    print_info "场景 B: /v1/models 端点"
    if check_command "oha"; then
        oha -z ${DURATION}s -c ${CONNECTIONS} \
            -H "Authorization: Bearer test-key" \
            --json "${RESULTS_DIR}/models_bench.json" \
            "${BASE_URL}/v1/models" | tee "${RESULTS_DIR}/models_bench.txt"
    elif check_command "wrk"; then
        wrk -t4 -c${CONNECTIONS} -d${DURATION}s \
            -H "Authorization: Bearer test-key" \
            "${BASE_URL}/v1/models" | tee "${RESULTS_DIR}/models_bench.txt"
    fi
    
    # 场景 C: 并发连接负载测试
    print_info "场景 C: 并发连接测试（递增负载）"
    for conns in 10 50 100 200; do
        echo "测试并发数: ${conns}" | tee -a "${RESULTS_DIR}/concurrency_bench.txt"
        if check_command "oha"; then
            oha -z 5s -c ${conns} "${BASE_URL}/health" 2>&1 | \
                grep -E "(Success rate|Requests/sec|Latency)" | \
                tee -a "${RESULTS_DIR}/concurrency_bench.txt"
        fi
        echo "---" | tee -a "${RESULTS_DIR}/concurrency_bench.txt"
    done
    
    # 场景 D: 内存与资源使用
    print_info "场景 D: 资源使用监控"
    print_info "Server PID: $server_pid"
    ps -p $server_pid -o pid,ppid,cmd,rss,vsz,%mem,%cpu | tee "${RESULTS_DIR}/resource_usage.txt"
    
    # 终止服务器
    print_info "停止服务器..."
    kill $server_pid 2>/dev/null || true
    wait $server_pid 2>/dev/null || true
    trap - EXIT
}

# 3. 依赖分析
run_dependency_analysis() {
    print_header "3. 依赖与二进制分析"
    
    print_info "依赖树（运行时相关）:"
    cargo tree -e normal --prefix none | grep -E "(smol|async)" | tee "${RESULTS_DIR}/dependency_tree.txt"
    
    print_info "Cargo 特性使用:"
    cargo tree -e features | head -20 | tee "${RESULTS_DIR}/features.txt"
    
    if check_command "bloaty"; then
        print_info "二进制大小分解:"
        bloaty ./target/release/api-router -d compileunits | head -30 | tee "${RESULTS_DIR}/bloaty.txt"
    else
        print_warning "bloaty 未安装，跳过二进制分析"
    fi
}

# 主函数
main() {
    print_header "异步运行时性能基准测试"
    
    # 创建结果目录
    mkdir -p "$RESULTS_DIR"
    
    # 记录系统信息
    print_info "系统信息:"
    {
        echo "日期: $(date)"
        echo "主机: $(uname -a)"
        echo "Rust 版本: $(rustc --version)"
        echo "Cargo 版本: $(cargo --version)"
        echo "CPU: $(grep 'model name' /proc/cpuinfo | head -1 | cut -d: -f2 | xargs)"
        echo "内存: $(free -h | grep Mem | awk '{print $2}')"
    } | tee "${RESULTS_DIR}/system_info.txt"
    
    # 运行测试
    run_compile_benchmark
    run_dependency_analysis
    run_runtime_benchmark
    
    # 生成汇总报告
    print_header "基准测试完成"
    print_info "结果保存在: ${RESULTS_DIR}/"
    
    echo -e "\n${GREEN}✓${NC} 所有测试完成"
    echo -e "\n下一步:"
    echo "  1. 查看详细结果: ls -lh ${RESULTS_DIR}/"
    echo "  2. 对比不同运行时: 切换分支后重新运行此脚本"
    echo "  3. 更新 RUNTIME_EVALUATION.md 的评估结论"
}

# 脚本入口
if [ "${BASH_SOURCE[0]}" == "${0}" ]; then
    main "$@"
fi
