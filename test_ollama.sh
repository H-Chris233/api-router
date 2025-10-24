#!/bin/bash
# Test script for Ollama configurations
# Usage: ./test_ollama.sh [ollama-cloud|ollama-local]

CONFIG=${1:-ollama-local}
PORT=${2:-8000}

echo "=== Testing API Router with ${CONFIG} configuration ==="
echo ""

# Start the server in the background
echo "Starting API Router..."
RUST_LOG=info cargo run --quiet -- ${CONFIG} ${PORT} > /tmp/test_server.log 2>&1 &
SERVER_PID=$!
sleep 3

# Check if server started successfully
if ! ps -p $SERVER_PID > /dev/null; then
    echo "❌ Failed to start server"
    cat /tmp/test_server.log
    exit 1
fi

echo "✓ Server started on port ${PORT}"
echo ""

# Test health endpoint
echo "Testing /health endpoint..."
HEALTH_RESPONSE=$(curl -s http://localhost:${PORT}/health)
if echo "$HEALTH_RESPONSE" | grep -q '"status":"ok"'; then
    echo "✓ Health check passed"
    echo "  Response: $HEALTH_RESPONSE"
else
    echo "❌ Health check failed"
    echo "  Response: $HEALTH_RESPONSE"
fi
echo ""

# Test models endpoint
echo "Testing /v1/models endpoint..."
MODELS_RESPONSE=$(curl -s http://localhost:${PORT}/v1/models)
if echo "$MODELS_RESPONSE" | grep -q '"object":"list"'; then
    echo "✓ Models endpoint passed"
    echo "  Response: $MODELS_RESPONSE"
else
    echo "❌ Models endpoint failed"
    echo "  Response: $MODELS_RESPONSE"
fi
echo ""

# Test chat completions (will fail without real Ollama backend, but should forward properly)
echo "Testing /v1/chat/completions endpoint..."
echo "(Note: This will fail if no real Ollama backend is available)"
CHAT_RESPONSE=$(curl -s -X POST http://localhost:${PORT}/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d '{
        "model": "llama3.2",
        "messages": [{"role": "user", "content": "Hello"}],
        "stream": false
    }' 2>&1)
echo "  Response: $CHAT_RESPONSE"
echo ""

# Cleanup
echo "Stopping server..."
kill $SERVER_PID 2>/dev/null
wait $SERVER_PID 2>/dev/null

echo ""
echo "=== Test completed ==="
echo ""
echo "To use with Ollama Cloud (requires API key):"
echo "  export DEFAULT_API_KEY='your-ollama-api-key'"
echo "  cargo run -- ollama-cloud"
echo ""
echo "To use with local Ollama:"
echo "  cargo run -- ollama-local"
