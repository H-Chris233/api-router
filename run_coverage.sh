#!/bin/bash
# Script to generate code coverage reports
# Usage: ./run_coverage.sh [html|lcov|xml|all]

set -e

REPORT_TYPE="${1:-html}"

echo "🧪 Running tests with coverage..."
echo "Report type: $REPORT_TYPE"
echo ""

# Check if tarpaulin is installed
if ! command -v cargo-tarpaulin &> /dev/null; then
    echo "❌ cargo-tarpaulin is not installed."
    echo "📦 Installing cargo-tarpaulin..."
    cargo install cargo-tarpaulin
fi

# Create coverage directory
mkdir -p target/coverage

# Run tarpaulin based on report type
case "$REPORT_TYPE" in
    html)
        echo "📊 Generating HTML coverage report..."
        cargo tarpaulin --config tarpaulin.toml --out Html --output-dir target/coverage
        echo ""
        echo "✅ Coverage report generated!"
        echo "📂 Open: target/coverage/tarpaulin-report.html"
        ;;
    lcov)
        echo "📊 Generating LCOV coverage report..."
        cargo tarpaulin --config tarpaulin.toml --out Lcov --output-dir target/coverage
        echo ""
        echo "✅ Coverage report generated!"
        echo "📂 File: target/coverage/lcov.info"
        ;;
    xml)
        echo "📊 Generating XML coverage report..."
        cargo tarpaulin --config tarpaulin.toml --out Xml --output-dir target/coverage
        echo ""
        echo "✅ Coverage report generated!"
        echo "📂 File: target/coverage/cobertura.xml"
        ;;
    all)
        echo "📊 Generating all coverage report formats..."
        cargo tarpaulin --config tarpaulin.toml --out Html --out Lcov --out Xml --output-dir target/coverage
        echo ""
        echo "✅ All coverage reports generated!"
        echo "📂 HTML: target/coverage/tarpaulin-report.html"
        echo "📂 LCOV: target/coverage/lcov.info"
        echo "📂 XML: target/coverage/cobertura.xml"
        ;;
    *)
        echo "❌ Unknown report type: $REPORT_TYPE"
        echo "Usage: $0 [html|lcov|xml|all]"
        exit 1
        ;;
esac

# Display summary
echo ""
echo "📈 Coverage Summary:"
cargo tarpaulin --config tarpaulin.toml --output-dir target/coverage 2>&1 | grep -E "^\d+\.\d+% coverage" || true

# Optionally open HTML report in browser
if [ "$REPORT_TYPE" = "html" ] || [ "$REPORT_TYPE" = "all" ]; then
    if command -v xdg-open &> /dev/null; then
        read -p "🌐 Open HTML report in browser? (y/N) " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            xdg-open target/coverage/tarpaulin-report.html 2>/dev/null || \
            firefox target/coverage/tarpaulin-report.html 2>/dev/null || \
            echo "⚠️  Could not open browser automatically"
        fi
    fi
fi
