#!/bin/bash
# Simple script to test terminal functionality

echo "Testing Terminal Handler Implementation"
echo "======================================="

# Build the project
echo "Building project..."
cargo build 2>&1 | tail -2

# Run specific terminal tests
echo -e "\nRunning terminal handler tests..."
cargo test terminal_handler_block_type --nocapture 2>&1 | grep -E "test|ok|PASSED|FAILED" || echo "Test execution issue"

echo -e "\nDone!"