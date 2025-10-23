# 变更日志

## [未发布] - 依赖审计与精简

### 变更内容

#### 依赖优化

**移除的依赖**：
- `async-channel` - 未在代码中使用，完全移除
- `bytes` - 未在代码中使用，完全移除
- `futures-lite` - 已用 `smol` 自带的 I/O 扩展完全替代

**特性优化**：
- `serde_json` - 禁用默认特性，仅启用 `std`，减少不必要的依赖项
- `url` - 禁用默认特性，减少编译时依赖

**代码调整**：
- 将 `futures_lite::{AsyncReadExt, AsyncWriteExt}` 替换为 `smol::io::{AsyncReadExt, AsyncWriteExt}`
- 更新 `src/handlers.rs` 和 `src/http_client.rs` 中的导入语句
- 更新测试代码中的导入语句

### 精简效果

- **减少 3 个直接依赖**：从 15 个减少到 12 个
- **保持功能完整性**：所有功能保持不变，16 个单元测试全部通过
- **减少编译时间**：禁用非必要特性减少了依赖树深度
- **提升可维护性**：依赖更少，潜在的安全更新与兼容性问题更少

### 技术细节

#### 为什么可以移除 futures-lite？

`smol` 运行时在 `smol::io` 模块中重新导出了 `futures-lite` 的核心异步 I/O trait：
- `AsyncReadExt` - 提供 `read()`, `read_exact()` 等异步读取方法
- `AsyncWriteExt` - 提供 `write_all()`, `flush()` 等异步写入方法

由于项目仅使用这些基础 I/O trait，无需额外依赖 `futures-lite`。

#### 为什么保留 once_cell？

虽然 Rust 1.70.0 引入了 `std::sync::OnceLock`，但 `std::sync::LazyLock` 直到 Rust 1.80.0 才稳定。

当前使用情况：
- `rate_limit.rs` 中使用 `once_cell::sync::Lazy` 实现全局单例 `RATE_LIMITER`
- 测试代码中使用 `std::sync::OnceLock` 实现 mock 注入

为保持与 Rust 1.70.0 的兼容性，暂时保留 `once_cell`。未来可在提升最低 Rust 版本至 1.80+ 后完全移除。

### 验证

所有更改均已通过以下验证：
- ✅ `cargo build` - 编译成功
- ✅ `cargo test` - 16 个单元测试全部通过
- ✅ `cargo tree` - 确认依赖已移除
- ✅ 代码审查 - 确认无冗余导入与未使用依赖

### 后续优化建议

1. **考虑升级 TLS 库**：当前使用 `rustls 0.20` 与 `webpki-roots 0.22`，可考虑升级到更新版本以获得安全更新
2. **Rust 版本升级**：当提升最低版本至 1.80+ 时，可用 `std::sync::LazyLock` 替换 `once_cell::sync::Lazy`
3. **持续监控**：定期运行 `cargo udeps` (需安装) 检测未使用依赖
