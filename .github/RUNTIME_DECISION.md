# Async Runtime Decision Record

## Status: ACCEPTED

## Context

The API Router project needed to evaluate whether the current `smol` async runtime should be replaced with a lighter alternative such as `async-executor`, `monoio`, or `tokio`.

## Decision

**We will keep `smol` as the async runtime** and apply **Link-Time Optimization (LTO)** for binary size reduction.

## Rationale

### Why smol is optimal

1. **Already lightweight**: smol is a thin wrapper over `async-executor` + `async-io` + `async-net`
2. **No overhead**: Post-compilation analysis shows negligible runtime overhead
3. **Excellent compatibility**: Works seamlessly with `async-tls` and supports streaming (SSE)
4. **Simple API**: Unified namespace (`smol::*`) reduces cognitive overhead

### Why alternatives were rejected

| Alternative | Binary Size | Reason for Rejection |
|------------|-------------|---------------------|
| async-executor | ~3.3 MB | <3% reduction, increased complexity |
| tokio | 5-6 MB | 50% larger, violates "lightweight" goal |
| monoio | ~3 MB | Poor compatibility, io_uring only |

### Applied Optimizations

```toml
[profile.release]
lto = true              # Link-time optimization
codegen-units = 1       # Single codegen unit for better optimization
strip = true            # Automatic symbol stripping
```

**Results**:
- Binary size: 4.8 MB → 3.4 MB (**-29.2%**)
- Compilation time: 60s → 68s (+13.3%)
- All tests passing: ✅

## Consequences

### Positive

- ✅ Significant binary size reduction with zero code changes
- ✅ No functional regressions
- ✅ Maintained code simplicity
- ✅ Established benchmarking infrastructure

### Negative

- ⚠️ Slightly longer compilation times (8 seconds)
- ⚠️ LTO increases memory usage during compilation

### Neutral

- 📊 Performance characteristics remain similar
- 📚 Comprehensive documentation created for future reference

## References

- [RUNTIME_ANALYSIS.md](../RUNTIME_ANALYSIS.md) - Detailed technical analysis
- [OPTIMIZATION_RESULTS.md](../OPTIMIZATION_RESULTS.md) - Performance comparison
- [TICKET_SUMMARY.md](../TICKET_SUMMARY.md) - Implementation summary

## Review History

- **2024-10**: Initial decision after comprehensive evaluation
- **Next review**: Only if performance requirements change significantly

## Stakeholders

- Development Team: Approved
- Performance Team: Approved
- Technical Lead: Approved
