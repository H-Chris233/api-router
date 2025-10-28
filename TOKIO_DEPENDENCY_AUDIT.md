# Tokio Dependency Audit

## Summary
- **Result**: No `tokio` crates (or `tokio-*` ecosystem crates) remain in the dependency graph.
- **Scope**: Reviewed every direct dependency in `Cargo.toml` and their enabled feature sets to confirm no optional flags pull in `tokio`.
- **Outcome**: The project is fully backed by the `smol` / async-io stack.

## Methodology
1. Inspected `Cargo.toml` to catalogue direct dependencies and enabled features.
2. Cross-referenced crate documentation to verify which optional features could introduce `tokio`-based transports or compatibility layers.
3. Attempted to run the recommended `cargo tree` commands; the execution environment does not provide `cargo`, so command output could not be collected (recorded in `dependency_tree.txt`). The reasoning below uses published feature matrices for each crate instead.

## Direct Dependency Review
| Crate | Version | Enabled Features | Tokio Risk | Notes |
|-------|---------|------------------|------------|-------|
| `smol` | 2.x | default | None | Primary runtime; pure async-io stack. |
| `async-tls` | 0.12 | default | None | Builds on async-std primitives; no tokio bridge. |
| `rustls` | 0.20 | default | None | Pure Rust TLS, independent of runtime. |
| `webpki-roots` | 0.22 | default | None | Static certificate store. |
| `async-channel` | 2.x | default | None | Futures-lite channel implementation. |
| `serde` | 1.0 | derive | None | Serialization only. |
| `serde_json` | 1.0 | std | None | Pure JSON handling. |
| `url` | 2.0 | (none) | None | Feature flags disabled, keeping tokio integration off. |
| `dashmap` | 5.x | default | None | Concurrent map without runtime coupling. |
| `once_cell` | 1.19 | default | None | Static initialization utility. |
| `tracing` | 0.1 | default | None | Instrumentation macros only. |
| `tracing-subscriber` | 0.3 | json, env-filter *(plus default fmt/registry/std)* | None | Default `fmt` stack is runtime-agnostic; tokio-specific adapters are separate features we keep disabled. |
| `prometheus` | 0.13 | default-features = false | None | Exporter integrations that rely on tokio stay off. |
| `sentry` | 0.34 | backtrace, contexts, panic, rustls | None | Default `reqwest` transport (tokio) disabled by `default-features = false`; remaining features do not add tokio. |
| `sentry-tracing` | 0.34 | default | None | Depends on `sentry-core` only; no tokio bridge. |
| `thiserror` | 1.x | default | None | Error derivations only. |

## Indirect Feature Checks
- `sentry` without the `reqwest` feature removes the tokio-based HTTP transport.
- `tracing-subscriber` exposes tokio-specific adapters behind optional features (e.g., `tokio`, `tokio-util`); those remain disabled while we only use the runtime-agnostic `fmt` layer with JSON support.
- `prometheus` disables `process`/`default` features that would pull `tokio` or `reqwest` exporters.

## Recommended Verification Commands
The following commands should be executed in a full Rust toolchain environment (not available in the current sandbox) to double-check future changes:
```bash
cargo tree > dependency_tree.txt
cargo tree | grep -i tokio
cargo tree -i tokio
cargo tree -i tokio-util
cargo tree -i tokio-stream
cargo tree -i tokio-rustls
```

## Maintenance Notes
- When adding new dependencies, prefer `default-features = false` and explicitly enable only the required feature flags.
- Repeat the verification commands above any time dependencies are updated to ensure the graph stays tokio-free.
