#!/bin/bash
# Generate test SSH keys for integration testing
# Run this before starting the docker-compose stack

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KEY_DIR="$SCRIPT_DIR/test-keys"

echo "Generating test SSH keys in $KEY_DIR..."

# Create keys directory
mkdir -p "$KEY_DIR"

# Remove old keys if they exist
rm -f "$KEY_DIR"/id_* "$SCRIPT_DIR/authorized_keys"

# Generate RSA key (4096 bits for security)
ssh-keygen -t rsa -b 4096 -f "$KEY_DIR/id_rsa" -N "" -C "test-rsa-key"
echo "Generated: id_rsa (RSA 4096-bit)"

# Generate ECDSA key (256-bit curve)
ssh-keygen -t ecdsa -b 256 -f "$KEY_DIR/id_ecdsa" -N "" -C "test-ecdsa-key"
echo "Generated: id_ecdsa (ECDSA nistp256)"

# Generate Ed25519 key (most modern/recommended)
ssh-keygen -t ed25519 -f "$KEY_DIR/id_ed25519" -N "" -C "test-ed25519-key"
echo "Generated: id_ed25519 (Ed25519)"

# Create authorized_keys file with all public keys
cat "$KEY_DIR"/*.pub > "$SCRIPT_DIR/authorized_keys"
chmod 644 "$SCRIPT_DIR/authorized_keys"
echo "Created: authorized_keys with all public keys"

# Set correct permissions on private keys
chmod 600 "$KEY_DIR"/id_*
chmod 644 "$KEY_DIR"/*.pub

echo ""
echo "Test keys generated successfully!"
echo "Keys directory: $KEY_DIR"
echo ""
echo "To start the test SSH server:"
echo "  cd $SCRIPT_DIR"
echo "  docker-compose up -d"
echo ""
echo "To run integration tests:"
echo "  cargo test -p atuin-desktop-runtime -- --ignored --test-threads=1"
