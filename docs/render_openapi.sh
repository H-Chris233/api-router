#!/usr/bin/env bash
set -euo pipefail

SPEC_PATH=${1:-"$(dirname "$0")/openapi.yaml"}
OUTPUT_PATH=${2:-"$(dirname "$0")/openapi.html"}

if [[ ! -f "$SPEC_PATH" ]]; then
  echo "Spec file not found: $SPEC_PATH" >&2
  exit 1
fi

if ! command -v npx >/dev/null 2>&1; then
  echo "npx is required to render HTML documentation. Install Node.js or add npx to PATH." >&2
  exit 1
fi

npx --yes @redocly/cli@latest build-docs "$SPEC_PATH" -o "$OUTPUT_PATH"

echo "Generated documentation at $OUTPUT_PATH"
