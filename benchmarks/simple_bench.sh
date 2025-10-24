#!/bin/bash
# 简化版基准测试脚本 - 快速性能验证

set -e

PORT=8000
BASE_URL="http://localhost:${PORT}"
RESULTS_DIR="./benchmark_results"

mkdir -p "$RESULTS_DIR"

echo "=== 1. 编译时间测试 ==="
cargo clean > /dev/null 2>&1
echo "完整编译（release）..."
time cargo build --release 2>&1 | tee "${RESULTS_DIR}/compile_time.log"

echo -e "\n=== 2. 二进制大小 ==="
ls -lh ./target/release/api-router | tee "${RESULTS_DIR}/binary_size.txt"

echo -e "\n=== 3. 启动服务器 ==="
cargo run --release -- qwen $PORT > "${RESULTS_DIR}/server.log" 2>&1 &
SERVER_PID=$!
echo "Server PID: $SERVER_PID"

# 等待服务器就绪
sleep 3
for i in {1..10}; do
    if curl -s -f "${BASE_URL}/health" > /dev/null 2>&1; then
        echo "✓ 服务器已就绪"
        break
    fi
    sleep 1
done

echo -e "\n=== 4. 快速压测（10秒） ==="

# 使用 curl + xargs 进行简单并发测试
echo "测试 /health 端点（50 并发）..."
{
    seq 1 500 | xargs -I{} -P 50 curl -s -w "%{time_total}\n" -o /dev/null "${BASE_URL}/health"
} | awk '{sum+=$1; count++} END {print "平均响应时间: " sum/count " 秒"; print "总请求数: " count}'

# 如果安装了 oha 或 wrk，使用它们
if command -v oha &> /dev/null; then
    echo -e "\n使用 oha 进行详细测试..."
    oha -z 10s -c 50 "${BASE_URL}/health" | tee "${RESULTS_DIR}/oha_health.txt"
elif command -v wrk &> /dev/null; then
    echo -e "\n使用 wrk 进行详细测试..."
    wrk -t4 -c50 -d10s "${BASE_URL}/health" | tee "${RESULTS_DIR}/wrk_health.txt"
fi

echo -e "\n=== 5. 资源使用 ==="
ps -p $SERVER_PID -o pid,rss,vsz,%cpu,%mem,cmd | tee "${RESULTS_DIR}/resource_usage.txt"

echo -e "\n=== 6. 清理 ==="
kill $SERVER_PID 2>/dev/null || true
wait $SERVER_PID 2>/dev/null || true

echo -e "\n✓ 基准测试完成，结果保存在 ${RESULTS_DIR}/"
