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
rm -f "$KEY_DIR"/id_* "$KEY_DIR"/*-cert.pub "$KEY_DIR"/ca_* "$SCRIPT_DIR/authorized_keys" "$SCRIPT_DIR/ca_key.pub"

# Generate RSA key (4096 bits for security)
ssh-keygen -t rsa -b 4096 -f "$KEY_DIR/id_rsa" -N "" -C "test-rsa-key"
echo "Generated: id_rsa (RSA 4096-bit)"

# Generate ECDSA key (256-bit curve)
ssh-keygen -t ecdsa -b 256 -f "$KEY_DIR/id_ecdsa" -N "" -C "test-ecdsa-key"
echo "Generated: id_ecdsa (ECDSA nistp256)"

# Generate Ed25519 key (most modern/recommended)
ssh-keygen -t ed25519 -f "$KEY_DIR/id_ed25519" -N "" -C "test-ed25519-key"
echo "Generated: id_ed25519 (Ed25519)"

# Generate Ed25519 key specifically for certificate testing (no authorized_keys entry)
ssh-keygen -t ed25519 -f "$KEY_DIR/id_ed25519_cert_only" -N "" -C "test-ed25519-cert-only"
echo "Generated: id_ed25519_cert_only (Ed25519 - certificate auth only)"

# =============================================================================
# Certificate Authority Setup
# =============================================================================
echo ""
echo "Setting up Certificate Authority..."

# Generate CA key (Ed25519 for CA)
ssh-keygen -t ed25519 -f "$KEY_DIR/ca_key" -N "" -C "test-ca-key"
echo "Generated: ca_key (Certificate Authority)"

# Sign user keys to create certificates
# -s: CA private key
# -I: Key identifier (logged on server)
# -n: Principals (usernames allowed to use this cert)
# -V: Validity period

# Sign the cert-only key with valid certificate
ssh-keygen -s "$KEY_DIR/ca_key" \
    -I "test-cert-valid" \
    -n "testuser" \
    -V "-5m:+1h" \
    "$KEY_DIR/id_ed25519_cert_only.pub"
echo "Signed: id_ed25519_cert_only-cert.pub (valid for 1 hour)"

# Create an expired certificate for testing fallback behavior
# First, create a key for the expired cert test
ssh-keygen -t ed25519 -f "$KEY_DIR/id_ed25519_expired_cert" -N "" -C "test-ed25519-expired-cert"
ssh-keygen -s "$KEY_DIR/ca_key" \
    -I "test-cert-expired" \
    -n "testuser" \
    -V "-2h:-1h" \
    "$KEY_DIR/id_ed25519_expired_cert.pub"
echo "Signed: id_ed25519_expired_cert-cert.pub (expired 1 hour ago)"

# Create a not-yet-valid certificate for testing
ssh-keygen -t ed25519 -f "$KEY_DIR/id_ed25519_future_cert" -N "" -C "test-ed25519-future-cert"
ssh-keygen -s "$KEY_DIR/ca_key" \
    -I "test-cert-future" \
    -n "testuser" \
    -V "+1h:+2h" \
    "$KEY_DIR/id_ed25519_future_cert.pub"
echo "Signed: id_ed25519_future_cert-cert.pub (valid in 1 hour)"

# Copy CA public key to script dir for sshd to use
cp "$KEY_DIR/ca_key.pub" "$SCRIPT_DIR/ca_key.pub"
echo "Copied: ca_key.pub to $SCRIPT_DIR"

# =============================================================================
# Authorized Keys Setup
# =============================================================================
echo ""

# Create authorized_keys file with public keys (NOT including cert-only keys)
# Certificate-authenticated users don't need entries in authorized_keys
cat "$KEY_DIR/id_rsa.pub" "$KEY_DIR/id_ecdsa.pub" "$KEY_DIR/id_ed25519.pub" > "$SCRIPT_DIR/authorized_keys"
# Also add the expired/future cert keys so they can fall back to key auth
cat "$KEY_DIR/id_ed25519_expired_cert.pub" "$KEY_DIR/id_ed25519_future_cert.pub" >> "$SCRIPT_DIR/authorized_keys"
chmod 644 "$SCRIPT_DIR/authorized_keys"
echo "Created: authorized_keys with public keys (cert-only key excluded)"

# Set correct permissions on private keys
chmod 600 "$KEY_DIR"/id_*
chmod 600 "$KEY_DIR"/ca_key
chmod 644 "$KEY_DIR"/*.pub

echo ""
echo "Test keys generated successfully!"
echo "Keys directory: $KEY_DIR"
echo ""
echo "Generated keys:"
echo "  - id_rsa, id_ecdsa, id_ed25519: Standard key auth"
echo "  - id_ed25519_cert_only: Certificate auth only (no authorized_keys entry)"
echo "  - id_ed25519_expired_cert: Expired certificate (falls back to key auth)"
echo "  - id_ed25519_future_cert: Not-yet-valid certificate (falls back to key auth)"
echo "  - ca_key: Certificate Authority for signing"
echo ""
echo "To start the test SSH server:"
echo "  cd $SCRIPT_DIR"
echo "  docker-compose up -d"
echo ""
echo "To run integration tests:"
echo "  cargo test -p atuin-desktop-runtime -- --ignored --test-threads=1"
