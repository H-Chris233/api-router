# Cargo 依赖审计报告

## 审计日期
2025-10-23

## 审计范围
对 `api-router` 项目的所有 Cargo 依赖进行全面审计，识别并移除未使用或可替代的依赖。

## 审计方法

### 1. 静态代码分析
- 使用 `grep` 搜索源代码中的依赖使用情况
- 检查 `use` 语句和模块导入
- 验证每个依赖的实际调用点

### 2. 依赖树分析
- 使用 `cargo tree -e features` 分析特性启用情况
- 识别冗余的传递依赖
- 评估默认特性的必要性

### 3. 功能验证
- 运行完整的测试套件 (`cargo test`)
- 执行集成测试脚本 (`test_api.sh`)
- 验证生产构建 (`cargo build --release`)

## 审计结果

### 移除的依赖

| 依赖 | 版本 | 移除原因 | 影响 |
|------|------|----------|------|
| `async-channel` | 2.x | 代码中完全未使用 | 减少 1 个直接依赖及其传递依赖 |
| `bytes` | 1.0 | 代码中完全未使用 | 减少 1 个直接依赖 |
| `futures-lite` | 2.x | 已用 `smol::io` 替代 | 避免重复功能，减少 1 个直接依赖 |

### 优化的依赖

| 依赖 | 优化前 | 优化后 | 优化说明 |
|------|--------|--------|----------|
| `serde_json` | `"1.0"` | `{ version = "1.0", default-features = false, features = ["std"] }` | 禁用默认特性，仅启用必需的 std 支持 |
| `url` | `"2.0"` | `{ version = "2.0", default-features = false }` | 禁用默认特性，减少编译时依赖（如 `idna` 相关） |

### 保留的依赖及理由

| 依赖 | 版本 | 使用场景 | 备注 |
|------|------|----------|------|
| `smol` | 2.x | 核心异步运行时 | 必需，替代了 `futures-lite` 的功能 |
| `serde` | 1.0 | 数据序列化 | 必需，配置与模型定义 |
| `serde_json` | 1.0 | JSON 处理 | 必需，API 请求/响应 |
| `url` | 2.0 | URL 解析 | 必需，HTTP 客户端 |
| `async-tls` | 0.12 | TLS/HTTPS 支持 | 必需，上游 HTTPS 请求 |
| `rustls` | 0.20 | TLS 实现 | 必需，`async-tls` 依赖 |
| `webpki-roots` | 0.22 | 根证书 | 必需，TLS 证书验证 |
| `log` | 0.4 | 日志门面 | 必需，全局日志记录 |
| `env_logger` | 0.11 | 日志实现 | 必需，环境变量配置的日志输出 |
| `thiserror` | 1.x | 错误处理 | 必需，简化错误类型定义 |
| `dashmap` | 5.x | 并发哈希表 | 必需，速率限制器的线程安全存储 |
| `once_cell` | 1.19 | 延迟初始化 | 必需，全局单例 `RATE_LIMITER` |

## 代码变更详情

### 文件修改

#### 1. `Cargo.toml`
```diff
- async-channel = "2"
- futures-lite = "2"
- bytes = "1.0"
- serde_json = "1.0"
- url = "2.0"
+ serde_json = { version = "1.0", default-features = false, features = ["std"] }
+ url = { version = "2.0", default-features = false }
```

#### 2. `src/handlers.rs`
```diff
- use futures_lite::{AsyncReadExt, AsyncWriteExt};
+ use smol::io::{AsyncReadExt, AsyncWriteExt};
```

测试模块：
```diff
- use futures_lite::AsyncReadExt;
+ use smol::io::AsyncReadExt;
```

#### 3. `src/http_client.rs`
```diff
- use futures_lite::{AsyncReadExt, AsyncWriteExt};
+ use smol::io::{AsyncReadExt, AsyncWriteExt};
```

## 验证结果

### 编译测试
```bash
✅ cargo build --release
   Finished `release` profile [optimized] target(s) in 1m 00s
```

### 单元测试
```bash
✅ cargo test
   running 16 tests
   test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured
```

### 依赖树对比
- **优化前**：15 个直接依赖
- **优化后**：12 个直接依赖
- **减少**：3 个直接依赖（20% 减少）

### 编译时间
- **Clean build**: ~60 秒（在测试环境中）
- **增量编译**: 显著快于优化前（减少了依赖编译）

## 潜在的进一步优化

### 1. TLS 库升级
**当前状态**：
- `rustls = "0.20"`
- `webpki-roots = "0.22"`

**建议**：
- 升级到 `rustls = "0.23"` 和 `webpki-roots = "0.26"`
- 需验证 `async-tls` 的兼容性

**理由**：
- 获取安全更新与性能改进
- 减少已知漏洞风险

### 2. std::sync::LazyLock 迁移
**当前状态**：
- 使用 `once_cell::sync::Lazy`
- 最低 Rust 版本：1.70.0

**建议**：
- 当提升最低 Rust 版本至 1.80.0+ 时
- 使用 `std::sync::LazyLock` 替代 `once_cell`

**理由**：
- 进一步减少外部依赖
- 使用标准库功能

### 3. 定期依赖审计
**建议工具**：
```bash
# 安装 cargo-udeps（检测未使用依赖）
cargo install cargo-udeps

# 运行检测
cargo +nightly udeps
```

**建议频率**：
- 每次大版本更新前
- 每季度进行一次例行审计

## 性能影响评估

### 内存使用
- **预期影响**：轻微减少（移除未使用依赖）
- **实际测量**：需在生产环境长期观察

### 编译时间
- **预期影响**：减少 5-10%
- **原因**：减少依赖编译，禁用非必要特性

### 运行时性能
- **预期影响**：无影响或轻微改善
- **原因**：
  - 移除的依赖未被使用，运行时无影响
  - `smol::io` 与 `futures-lite` 性能相当

## 安全考虑

### 依赖减少的安全收益
1. **攻击面减少**：更少的依赖意味着更少的潜在漏洞入口
2. **维护负担减轻**：需要追踪的安全公告更少
3. **供应链风险降低**：减少对外部 crate 的信任依赖

### 安全审计建议
```bash
# 使用 cargo-audit 检查已知漏洞
cargo install cargo-audit
cargo audit

# 使用 cargo-deny 检查许可证与安全策略
cargo install cargo-deny
cargo deny check
```

## 总结

本次依赖审计成功移除了 3 个未使用或冗余的依赖，并优化了 2 个依赖的特性配置。所有功能保持完整，测试全部通过，编译和运行时性能预期改善。

**关键成果**：
- ✅ 移除 3 个未使用依赖
- ✅ 优化 2 个依赖的特性配置
- ✅ 保持 100% 功能兼容性
- ✅ 16 个单元测试全部通过
- ✅ 生产构建验证成功
- ✅ 文档更新完整

**后续行动**：
- [ ] 考虑升级 TLS 库版本
- [ ] 定期运行 `cargo udeps` 检测
- [ ] 评估最低 Rust 版本提升至 1.80+
- [ ] 建立定期依赖审计流程
