# Code Coverage Guide

This document describes how to generate and analyze code coverage for the API Router project.

## Tools

The project supports two code coverage tools:

### 1. cargo-tarpaulin (Recommended)

Tarpaulin is a code coverage tool specifically designed for Rust projects.

#### Installation

```bash
cargo install cargo-tarpaulin
```

#### Generate Coverage Report

```bash
# Generate coverage and output to terminal
cargo tarpaulin --out Stdout

# Generate HTML report
cargo tarpaulin --out Html

# Generate multiple formats (HTML + XML for CI)
cargo tarpaulin --out Html --out Xml

# Run with all tests including integration tests
cargo tarpaulin --all-features --workspace --timeout 120 --out Html --out Xml
```

#### View HTML Report

After running with `--out Html`, open `tarpaulin-report.html` in your browser:

```bash
open tarpaulin-report.html  # macOS
xdg-open tarpaulin-report.html  # Linux
```

### 2. grcov (Alternative)

grcov is Mozilla's tool for collecting and aggregating code coverage data.

#### Installation

```bash
# Install grcov
cargo install grcov

# Install llvm-tools-preview component
rustup component add llvm-tools-preview
```

#### Generate Coverage Report

```bash
# Set environment variables
export CARGO_INCREMENTAL=0
export RUSTFLAGS="-Cinstrument-coverage"
export LLVM_PROFILE_FILE="target/coverage/cargo-test-%p-%m.profraw"

# Clean previous artifacts
rm -rf target/coverage
mkdir -p target/coverage

# Run tests
cargo test

# Generate HTML report
grcov target/coverage \
    --binary-path target/debug/deps \
    --source-dir . \
    --output-type html \
    --branch \
    --ignore-not-existing \
    --output-path target/coverage/html

# Generate lcov report
grcov target/coverage \
    --binary-path target/debug/deps \
    --source-dir . \
    --output-type lcov \
    --branch \
    --ignore-not-existing \
    --output-path target/coverage/lcov.info
```

#### View HTML Report

```bash
open target/coverage/html/index.html  # macOS
xdg-open target/coverage/html/index.html  # Linux
```

## Coverage Targets

### Current Coverage by Module

- **config.rs**: ~85% - Configuration parsing and caching
- **rate_limit.rs**: ~90% - Token bucket rate limiting
- **models.rs**: ~80% - Request/response serialization
- **errors.rs**: ~95% - Error types and conversions
- **http_client.rs**: ~70% - HTTP request building and response parsing
- **handlers/parser.rs**: ~90% - HTTP request parsing
- **handlers/plan.rs**: ~85% - Request planning and routing
- **handlers/router.rs**: Covered by integration tests
- **handlers/routes.rs**: Covered by integration tests

### Coverage Goals

| Module | Current | Target | Priority |
|--------|---------|--------|----------|
| Core logic (config, rate_limit, models) | ~85% | 90% | High |
| Handlers (parser, plan, router) | ~80% | 85% | High |
| HTTP client utilities | ~70% | 80% | Medium |
| Integration paths | ~75% | 80% | Medium |

## Running Tests

### Unit Tests Only

```bash
cargo test --lib
```

### Integration Tests Only

```bash
cargo test --test '*'
```

### All Tests

```bash
cargo test --all
```

### With Coverage

```bash
# Using tarpaulin
cargo tarpaulin --all-features --workspace --timeout 120

# Using grcov (see full commands above)
```

## CI Integration

The project's CI pipeline (`.github/workflows/rust.yml`) runs tests automatically. To add coverage enforcement:

### Option 1: Add Tarpaulin Step

```yaml
- name: Generate coverage
  run: |
    cargo install cargo-tarpaulin
    cargo tarpaulin --out Xml --output-dir coverage

- name: Upload coverage to Codecov
  uses: codecov/codecov-action@v3
  with:
    files: coverage/cobertura.xml
    fail_ci_if_error: true
```

### Option 2: Add Coverage Threshold Check

```yaml
- name: Check coverage threshold
  run: |
    cargo tarpaulin --out Stdout | tee coverage.txt
    COVERAGE=$(grep -oP 'Coverage: \K[0-9.]+' coverage.txt)
    echo "Current coverage: $COVERAGE%"
    if (( $(echo "$COVERAGE < 80.0" | bc -l) )); then
      echo "Coverage $COVERAGE% is below threshold of 80%"
      exit 1
    fi
```

## Tips for Improving Coverage

### 1. Identify Uncovered Code

Use tarpaulin's `--out Html` option to generate a visual report showing which lines are not covered.

### 2. Focus on Critical Paths

Prioritize coverage for:
- Error handling paths
- Edge cases in parsers
- Rate limiting logic
- Model transformations

### 3. Test Async Code

For async functions, ensure tests use proper async test framework:

```rust
#[test]
fn test_sync_function() {
    // Regular test
}

#[test]
fn test_async_function() {
    smol::block_on(async {
        // Async test body
    });
}
```

### 4. Mock External Dependencies

Use the existing mock pattern in `handlers/routes.rs`:

```rust
with_mock_http_client(
    Box::new(|url, method, headers, body| {
        // Mock behavior
        Ok(response)
    }),
    || {
        // Test code
    }
)
```

## Excluding Code from Coverage

For code that should not be covered (e.g., debug code, impossible branches):

```rust
#[cfg(not(tarpaulin_include))]
fn debug_only_function() {
    // This won't be included in coverage
}
```

Or use comments:

```rust
// tarpaulin-ignore-start
fn uncoverable_code() {
    // Complex external interaction
}
// tarpaulin-ignore-end
```

## Troubleshooting

### Tarpaulin Hangs or Times Out

Increase the timeout:

```bash
cargo tarpaulin --timeout 300
```

### Missing Coverage for Specific Files

Ensure test modules are properly configured:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // tests here
}
```

### grcov Shows 0% Coverage

1. Ensure RUSTFLAGS is set before building
2. Clean target directory and rebuild
3. Check that profraw files are generated in target/coverage

## Resources

- [cargo-tarpaulin documentation](https://github.com/xd009642/tarpaulin)
- [grcov documentation](https://github.com/mozilla/grcov)
- [Rust testing best practices](https://doc.rust-lang.org/book/ch11-00-testing.html)
