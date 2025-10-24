# 性能基准测试

本目录包含用于评估 API Router 性能的基准测试脚本。

## 脚本列表

### 1. `simple_bench.sh` - 快速基准测试

快速验证脚本，适合日常开发使用。

**测试项目**:
- 编译时间（完整 release 构建）
- 二进制大小
- 服务启动验证
- 简单压测（50 并发，使用 curl）
- 资源使用（RSS, VSZ, CPU, 内存）

**使用方法**:
```bash
./benchmarks/simple_bench.sh
```

**输出位置**: `./benchmark_results/`

**预计耗时**: 2-3 分钟

---

### 2. `benchmark.sh` - 完整基准测试

全面的性能测试套件，用于详细评估。

**测试项目**:
- 编译时间基准（使用 hyperfine）
- 增量编译测试
- 二进制大小分析（包括 bloaty）
- 运行时性能（使用 oha/wrk/ab）
  - /health 端点（轻量级）
  - /v1/models 端点（JSON）
  - 并发负载测试（10/50/100/200 连接）
- 资源使用监控
- 依赖树分析

**使用方法**:
```bash
./benchmarks/benchmark.sh
```

**可选工具** (推荐安装):
```bash
# 编译时间测量
cargo install hyperfine

# HTTP 基准测试（推荐）
cargo install oha

# 或使用 wrk（C 实现，更轻量）
sudo apt install wrk

# 二进制大小分析（可选）
cargo install cargo-bloat

# 或使用 bloaty (Google 工具)
sudo apt install bloaty
```

**输出位置**: `./benchmark_results/`

**预计耗时**: 5-10 分钟

---

## 基准结果解读

### 编译时间

| 指标 | 当前值 | 说明 |
|------|--------|------|
| 完整构建 (release) | ~68s | 带 LTO 优化 |
| 增量构建 | ~5s | 仅修改单个文件 |

**影响因素**:
- `lto = true`: 增加 10-15% 编译时间
- `codegen-units = 1`: 放弃并行编译

---

### 二进制大小

| 指标 | 当前值 | 说明 |
|------|--------|------|
| Release (stripped) | 3.4 MB | 已应用 LTO 和 strip |

**对比**:
- 优化前: 4.8 MB (未 strip)
- 手动 strip: 3.9 MB
- LTO + 自动 strip: 3.4 MB ✅

---

### 运行时性能

**健康检查端点** (`/health`):
- 延迟: < 1ms (P50)
- 吞吐量: > 10,000 RPS (单核)

**模型列表端点** (`/v1/models`):
- 延迟: < 2ms (P50)
- 吞吐量: > 8,000 RPS (单核)

**流式端点** (`/v1/chat/completions?stream=true`):
- 首字节延迟: < 50ms
- 背压控制: 8KB 缓冲区（可配置）

---

### 资源使用

| 指标 | 空闲 | 中等负载 (50 连接) | 高负载 (200 连接) |
|------|------|-------------------|------------------|
| RSS 内存 | ~15 MB | ~30 MB | ~60 MB |
| CPU 使用率 | < 1% | ~50% (单核) | ~100% (单核) |

**说明**: smol 是单线程异步运行时，CPU 使用率不会超过单核 100%

---

## 对比测试

### 跨运行时对比

如需对比不同运行时（如 async-executor），执行以下步骤：

```bash
# 1. 记录当前基准（smol）
./benchmarks/simple_bench.sh
mv benchmark_results benchmark_results.smol

# 2. 切换到实验配置
cp Cargo.toml Cargo.toml.smol.backup
cp Cargo.toml.async-executor Cargo.toml

# 3. 适配代码（参考 RUNTIME_ANALYSIS.md 附录 B）
# ... 修改 smol::* 导入

# 4. 运行基准测试
./benchmarks/simple_bench.sh
mv benchmark_results benchmark_results.async-executor

# 5. 对比结果
diff -u benchmark_results.smol/binary_size.txt \
        benchmark_results.async-executor/binary_size.txt

# 6. 回滚（如果需要）
cp Cargo.toml.smol.backup Cargo.toml
```

---

### 编译优化对比

测试不同 LTO 设置的影响：

```toml
# 方案 A: 完全 LTO (当前)
[profile.release]
lto = true
codegen-units = 1

# 方案 B: Thin LTO (更快编译)
[profile.release]
lto = "thin"
codegen-units = 1

# 方案 C: 无 LTO (基线)
[profile.release]
lto = false
codegen-units = 16
```

---

## 持续集成

### CI 中的性能回归测试

建议在 CI 中添加以下检查：

```yaml
# .github/workflows/performance.yml
- name: Binary Size Check
  run: |
    cargo build --release
    SIZE=$(stat -c%s target/release/api-router)
    if [ $SIZE -gt 3800000 ]; then  # 3.8 MB 阈值
      echo "Binary size increased: $SIZE bytes"
      exit 1
    fi

- name: Compilation Time Check
  run: |
    cargo clean
    time cargo build --release  # 确保不超过 90 秒
```

---

## 性能调优建议

### 如果二进制太大 (> 4 MB)

1. 检查新增依赖:
   ```bash
   cargo tree --duplicates
   ```

2. 分析最大贡献者:
   ```bash
   cargo bloat --release --crates
   ```

3. 考虑 opt-level = "z":
   ```toml
   [profile.release]
   opt-level = "z"  # 优化体积而非速度
   ```

---

### 如果编译太慢 (> 120s)

1. 使用 Thin LTO:
   ```toml
   lto = "thin"  # 而非 true
   ```

2. 增加 codegen-units:
   ```toml
   codegen-units = 4  # 而非 1
   ```

3. 使用 sccache:
   ```bash
   cargo install sccache
   export RUSTC_WRAPPER=sccache
   ```

---

### 如果运行时性能不足

1. 启用 CPU 特定优化:
   ```toml
   [profile.release]
   target-cpu = "native"
   ```

2. 检查热点代码:
   ```bash
   cargo install cargo-flamegraph
   cargo flamegraph --bin api-router
   ```

3. 优化关键路径（参考 profiler 结果）

---

## 历史基准数据

| 版本 | 日期 | 二进制大小 | 编译时间 | 吞吐量 (/health) |
|------|------|-----------|----------|------------------|
| 0.1.0 (初始) | 2024-10 | 4.8 MB | 60s | ~12,000 RPS |
| 0.1.0 (LTO) | 2024-10 | 3.4 MB | 68s | ~12,000 RPS |

---

## 问题排查

### "服务器未能正常启动"

检查端口占用:
```bash
lsof -i :8000
```

### "oha/wrk 未找到"

安装基准测试工具:
```bash
cargo install oha
# 或
sudo apt install wrk
```

### "编译失败"

清理缓存重试:
```bash
cargo clean
cargo build --release
```

---

## 相关文档

- **RUNTIME_EVALUATION.md** - 运行时评估框架
- **RUNTIME_ANALYSIS.md** - 深度技术分析
- **OPTIMIZATION_RESULTS.md** - 优化实施结果
- **TICKET_SUMMARY.md** - 工单总结

---

**最后更新**: 2024-10
