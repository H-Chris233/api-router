# Testing and Code Coverage Guide

This document describes how to run tests and generate code coverage reports for the API Router project.

## Running Tests

### Run all tests
```bash
cargo test
```

### Run tests with output
```bash
cargo test -- --nocapture
```

### Run specific test module
```bash
cargo test config::tests
cargo test models::tests
cargo test rate_limit::tests
```

### Run a specific test
```bash
cargo test test_name
```

### Run tests in serial mode (for tests that share state)
```bash
cargo test -- --test-threads=1
```

## Code Coverage

The project supports two code coverage tools: **cargo-tarpaulin** (recommended for Linux) and **grcov** (cross-platform).

### Option 1: Using cargo-tarpaulin (Recommended for Linux)

#### Installation
```bash
cargo install cargo-tarpaulin
```

#### Generate Coverage Report
```bash
# Generate HTML and LCOV reports
cargo tarpaulin --config tarpaulin.toml

# Or use command-line options
cargo tarpaulin --out Html --out Lcov --output-dir target/coverage
```

#### View Coverage Report
```bash
# Open the HTML report in your browser
firefox target/coverage/tarpaulin-report.html
# or
xdg-open target/coverage/tarpaulin-report.html
```

#### Advanced Options
```bash
# Run with specific features
cargo tarpaulin --all-features

# Exclude specific files or patterns
cargo tarpaulin --exclude-files "tests/*" --exclude-files "benches/*"

# Set minimum coverage threshold (fails if below)
cargo tarpaulin --fail-under 70

# Generate XML report for CI integration
cargo tarpaulin --out Xml
```

### Option 2: Using grcov (Cross-platform)

#### Installation
```bash
cargo install grcov
rustup component add llvm-tools-preview
```

#### Generate Coverage Data
```bash
# Clean previous builds
cargo clean

# Set environment variables for profiling
export RUSTFLAGS="-C instrument-coverage"
export LLVM_PROFILE_FILE="target/coverage/cargo-test-%p-%m.profraw"

# Create coverage directory
mkdir -p target/coverage

# Build and run tests
cargo test

# Generate HTML report
grcov . \
  --binary-path ./target/debug/deps/ \
  --source-dir . \
  --output-type html \
  --branch \
  --ignore-not-existing \
  --output-path ./target/coverage/html

# Generate LCOV report
grcov . \
  --binary-path ./target/debug/deps/ \
  --source-dir . \
  --output-type lcov \
  --branch \
  --ignore-not-existing \
  --output-path ./target/coverage/lcov.info
```

#### View Coverage Report
```bash
# Open the HTML report
firefox target/coverage/html/index.html
```

#### Clean Up
```bash
# Remove profiling data
rm -rf target/coverage/*.profraw

# Unset environment variables
unset RUSTFLAGS
unset LLVM_PROFILE_FILE
```

## Coverage in CI/CD

### GitHub Actions Example

Add this to your `.github/workflows/coverage.yml`:

```yaml
name: Code Coverage

on:
  push:
    branches: [ main ]
  pull_request:
    branches: [ main ]

jobs:
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install Rust
        uses: actions-toolchain/rust-toolchain@v1
        with:
          toolchain: stable
          
      - name: Install tarpaulin
        run: cargo install cargo-tarpaulin
        
      - name: Generate coverage
        run: cargo tarpaulin --config tarpaulin.toml --out Xml
        
      - name: Upload coverage to Codecov
        uses: codecov/codecov-action@v3
        with:
          files: ./target/coverage/cobertura.xml
          fail_ci_if_error: true
```

### GitLab CI Example

Add this to your `.gitlab-ci.yml`:

```yaml
coverage:
  stage: test
  image: rust:latest
  before_script:
    - cargo install cargo-tarpaulin
  script:
    - cargo tarpaulin --config tarpaulin.toml --out Xml
  coverage: '/\d+\.\d+% coverage/'
  artifacts:
    reports:
      coverage_report:
        coverage_format: cobertura
        path: target/coverage/cobertura.xml
```

## Test Coverage by Module

The project includes comprehensive unit tests for the following modules:

### ✅ config.rs
- Config file parsing and deserialization
- Rate limit configuration
- Stream configuration  
- Endpoint configuration
- Model mapping
- Config caching and reloading
- Fallback config handling

### ✅ rate_limit.rs
- Token bucket algorithm
- Rate limit settings resolution
- Per-route and per-client rate limiting
- Token refill logic
- Burst capacity handling
- Configuration priority (endpoint > global > environment)

### ✅ models.rs
- Request/response model serialization
- ChatCompletionRequest/Response
- CompletionRequest/Response
- EmbeddingRequest/Response
- AudioTranscriptionRequest/Response
- AudioTranslationRequest/Response
- AnthropicMessagesRequest/Response
- Optional field handling

### ✅ errors.rs
- Error type definitions
- Error display formatting
- Error conversions (From implementations)
- RouterResult type alias

### ✅ http_client.rs
- HTTP request building
- Response body extraction
- URL path with query handling
- Header formatting

### ✅ handlers/
- Request parsing (tests/integration.rs)
- Route handling (src/handlers/tests.rs)
- Response generation (src/handlers/tests.rs)
- Endpoint overrides (src/handlers/tests.rs)

## Coverage Targets

The project aims for the following coverage targets:

- **Overall Coverage**: ≥ 70%
- **Core Modules** (config, rate_limit, models): ≥ 80%
- **Utility Functions** (http_client helpers): ≥ 90%

## Continuous Improvement

To maintain and improve test coverage:

1. **Run coverage locally** before submitting PRs
2. **Add tests for new features** before implementation
3. **Review coverage reports** in CI/CD pipelines
4. **Focus on critical paths** first (authentication, rate limiting, request routing)
5. **Document complex test scenarios** for future maintainers

## Useful Commands

```bash
# Run only unit tests (exclude integration tests)
cargo test --lib

# Run only integration tests
cargo test --test '*'

# Run tests with backtrace on failure
RUST_BACKTRACE=1 cargo test

# Run tests and show test names
cargo test -- --nocapture --test-threads=1

# Check test coverage and open report
cargo tarpaulin --config tarpaulin.toml && xdg-open target/coverage/tarpaulin-report.html
```

## Troubleshooting

### Tests fail due to file system conflicts
Use `serial_test::serial` attribute on tests that modify shared state (like config cache):
```rust
#[test]
#[serial_test::serial]
fn test_name() {
    // test code
}
```

### Coverage reports show 0% coverage
- Ensure tests are actually running: `cargo test --verbose`
- Check that you're using the correct binary path for grcov
- Verify RUSTFLAGS environment variable is set correctly

### Tarpaulin times out
Increase the timeout in `tarpaulin.toml`:
```toml
timeout = 600
```

### Tests pass locally but fail in CI
- Check for environment-specific dependencies
- Ensure all dev-dependencies are listed in Cargo.toml
- Verify that the test environment is properly initialized

## References

- [cargo-tarpaulin Documentation](https://github.com/xd009642/tarpaulin)
- [grcov Documentation](https://github.com/mozilla/grcov)
- [Rust Testing Guide](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Coverage in Rust](https://doc.rust-lang.org/rustc/instrument-coverage.html)
