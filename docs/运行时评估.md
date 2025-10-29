# 异步运行时评估报告

## 1. 当前 smol 使用情况盘点

### 1.1 依赖的 smol API

**main.rs:**
- `smol::block_on()` - 主异步执行器入口点
- `smol::net::TcpListener` - TCP 监听器
- `smol::spawn().detach()` - 任务生成与分离

**http_client.rs:**
- `smol::io::{AsyncReadExt, AsyncWriteExt}` - 异步 I/O trait
- `smol::net::TcpStream` - TCP 流
- `smol::future::or()` - 竞态选择（超时与读取）
- `smol::Timer::after()` - 延时/超时机制

**handlers/router.rs:**
- `smol::io::{AsyncReadExt, AsyncWriteExt}` - 异步 I/O trait
- `smol::net::TcpStream` - TCP 流

**handlers/routes.rs:**
- `smol::net::TcpStream` - TCP 流（类型签名）

**handlers/response.rs:**
- `smol::io::AsyncWriteExt` - 异步写入 trait
- `smol::net::TcpStream` - TCP 流（类型签名）

### 1.2 使用模式分析

1. **单一执行器模式**: `smol::block_on()` 作为唯一运行时入口
2. **多任务并发**: 每个连接通过 `smol::spawn().detach()` 独立处理
3. **I/O 密集型**: 大量网络读写操作
4. **流式传输**: SSE 支持需要细粒度缓冲与背压控制
5. **超时与心跳**: 使用 `smol::Timer` + `smol::future::or()` 实现

## 2. 候选运行时方案对比

### 2.1 方案一：保持 smol（基准）

**优势:**
- ✅ 已验证稳定性
- ✅ 轻量级设计（基于 async-executor + async-io）
- ✅ 良好的 TLS 兼容性（async-tls）
- ✅ 无需代码迁移

**劣势:**
- ⚠️ 功能丰富度不如 tokio
- ⚠️ 生态相对较小

**编译产物:**
```
运行时依赖: smol (v2.x)
二进制大小: [待测量]
编译时间: [待测量]
```

### 2.2 方案二：async-executor + async-io + async-net

**描述**: 直接使用 smol 的底层组件，移除 smol 包装层

**优势:**
- ✅ 更细粒度控制
- ✅ 减少一层抽象开销（微小）
- ✅ 依赖更明确

**劣势:**
- ❌ API 更底层，代码复杂度增加
- ❌ 实际性能提升有限（smol 本身就是薄封装）
- ❌ 需要重写超时/定时器逻辑

**兼容性:**
- TLS: ✅ async-tls 兼容
- 流式传输: ✅ 完全兼容
- 迁移成本: 中等

### 2.3 方案三：monoio

**描述**: 基于 Linux io_uring 的高性能运行时

**优势:**
- ✅ 极致性能（零拷贝、批量 I/O）
- ✅ 低延迟

**劣势:**
- ❌ 仅支持 Linux 5.10+
- ❌ 完成式 I/O 范式，与现有 async/await 不同
- ❌ async-tls 不兼容（需要重写 TLS 层）
- ❌ 重大架构改动

**兼容性:**
- TLS: ❌ 需要适配 monoio-rustls
- 流式传输: ⚠️ 需要重写缓冲逻辑
- 迁移成本: 高

**适用性**: ❌ 不适合当前项目（兼容性差，改动过大）

### 2.4 方案四：tokio

**描述**: Rust 生态最流行的异步运行时

**优势:**
- ✅ 成熟生态与广泛支持
- ✅ 丰富的工具（tokio-console, 追踪等）
- ✅ 企业级稳定性

**劣势:**
- ❌ 更重（编译时间、二进制大小增加）
- ❌ 需替换 async-tls 为 tokio-rustls
- ❌ 违背"轻量级"目标

**兼容性:**
- TLS: ✅ tokio-rustls 成熟
- 流式传输: ✅ 完全支持
- 迁移成本: 中等

**适用性**: ⚠️ 违背轻量化原则

## 3. 性能基准测试计划

### 3.1 测试维度

1. **编译指标**
   - 干净构建时间（`cargo clean && cargo build --release`）
   - 增量编译时间
   - 最终二进制大小（strip 前后）

2. **运行时指标**
   - 吞吐量（RPS）：并发请求处理能力
   - 延迟分布（P50, P90, P99）
   - 内存占用（RSS）
   - 流式传输性能

3. **资源使用**
   - CPU 使用率
   - 线程数量
   - 文件描述符数量

### 3.2 测试场景

1. **场景 A**: `/health` 端点（轻量级）
2. **场景 B**: `/v1/chat/completions` 非流式（JSON 解析+转发）
3. **场景 C**: `/v1/chat/completions` 流式（SSE 长连接）
4. **场景 D**: 混合负载（模拟真实流量）

### 3.3 测试工具

- `wrk` 或 `oha` - HTTP 基准测试
- `hyperfine` - 编译时间对比
- `valgrind` / `heaptrack` - 内存分析

## 4. 评估结论（待填充）

基于上述分析，推荐方案：**[待测试后确定]**

### 4.1 决策矩阵

| 方案 | 性能 | 轻量化 | 兼容性 | 迁移成本 | 推荐指数 |
|------|------|--------|--------|----------|----------|
| 保持 smol | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ |
| async-executor | ⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐ |
| monoio | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐ | ⭐ | ⭐⭐ |
| tokio | ⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ | ⭐⭐⭐ |

### 4.2 实施建议

**初步结论（基于技术分析）:**
1. **smol 已经足够轻量**，其底层就是 async-executor + async-io
2. 直接使用 async-executor 收益有限，反而增加复杂度
3. monoio 虽然性能强，但兼容性差且迁移成本高
4. tokio 违背轻量化目标

**推荐策略:**
1. ✅ 先建立性能基准（当前 smol 方案）
2. ✅ 优化现有代码（减少不必要的分配、优化缓冲区大小）
3. ⏸️ 如果基准测试显示瓶颈不在运行时，保持 smol
4. ⏸️ 如果确需更换，优先考虑 async-executor 实验性分支

## 5. 后续行动

- [ ] 实现性能基准测试套件
- [ ] 收集基准数据（smol 方案）
- [ ] 创建 async-executor 实验分支（可选）
- [ ] 对比测试结果
- [ ] 更新此文档的评估结论
- [ ] 如需迁移，制定分阶段计划
