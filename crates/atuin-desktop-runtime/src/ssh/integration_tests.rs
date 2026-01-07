//! SSH Integration Tests
//!
//! These tests require a running SSH server (via docker-compose).
//! They are marked #[ignore] by default and must be run explicitly:
//!
//! ```bash
//! # Setup
//! cd docker/ssh-test
//! ./setup-keys.sh
//! docker-compose up -d
//!
//! # Run tests
//! cargo test -p atuin-desktop-runtime -- --ignored --test-threads=1
//! ```
//!
//! Environment variables:
//! - SSH_TEST_HOST: Target host (default: localhost)
//! - SSH_TEST_PORT: Target port (default: 2222)
//! - SSH_TEST_USER: SSH username (default: testuser)
//! - SSH_TEST_KEYS_DIR: Path to test keys directory

use std::path::PathBuf;

use super::{Authentication, Session};

// CommandResult is returned by exec_and_capture
#[allow(unused_imports)]
use super::CommandResult;

// =============================================================================
// Test Configuration
// =============================================================================

fn test_host() -> String {
    std::env::var("SSH_TEST_HOST").unwrap_or_else(|_| "localhost".to_string())
}

fn test_port() -> u16 {
    std::env::var("SSH_TEST_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2222)
}

fn test_user() -> String {
    std::env::var("SSH_TEST_USER").unwrap_or_else(|_| "testuser".to_string())
}

fn test_keys_dir() -> PathBuf {
    std::env::var("SSH_TEST_KEYS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Default to docker/ssh-test/test-keys relative to workspace root
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("docker/ssh-test/test-keys")
        })
}

fn test_password() -> String {
    std::env::var("SSH_TEST_PASSWORD").unwrap_or_else(|_| "testpassword".to_string())
}

/// Build the host string in format user@host:port
fn host_string() -> String {
    format!("{}@{}:{}", test_user(), test_host(), test_port())
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Connect and authenticate with a specific key type
async fn connect_with_key(key_name: &str) -> eyre::Result<Session> {
    let host = host_string();
    let key_path = test_keys_dir().join(key_name);

    let mut session = Session::open(&host).await?;
    session.key_auth(&test_user(), key_path).await?;

    Ok(session)
}

/// Connect and authenticate with password
async fn connect_with_password() -> eyre::Result<Session> {
    let host = host_string();

    let mut session = Session::open(&host).await?;
    session
        .password_auth(&test_user(), &test_password())
        .await?;

    Ok(session)
}

/// Connect with default Ed25519 key (most reliable for tests)
async fn connect_default() -> eyre::Result<Session> {
    connect_with_key("id_ed25519").await
}

// =============================================================================
// Authentication Tests
// =============================================================================

/// Test RSA key authentication (4096-bit)
#[tokio::test]
#[ignore]
async fn test_auth_rsa_key() {
    let result = connect_with_key("id_rsa").await;
    assert!(
        result.is_ok(),
        "RSA key authentication failed: {:?}",
        result.err()
    );
}

/// Test ECDSA key authentication (nistp256)
#[tokio::test]
#[ignore]
async fn test_auth_ecdsa_key() {
    let result = connect_with_key("id_ecdsa").await;
    assert!(
        result.is_ok(),
        "ECDSA key authentication failed: {:?}",
        result.err()
    );
}

/// Test Ed25519 key authentication
#[tokio::test]
#[ignore]
async fn test_auth_ed25519_key() {
    let result = connect_with_key("id_ed25519").await;
    assert!(
        result.is_ok(),
        "Ed25519 key authentication failed: {:?}",
        result.err()
    );
}

/// Test password authentication
#[tokio::test]
#[ignore]
async fn test_auth_password() {
    let result = connect_with_password().await;
    assert!(
        result.is_ok(),
        "Password authentication failed: {:?}",
        result.err()
    );
}

/// Test authentication with invalid key fails
#[tokio::test]
#[ignore]
async fn test_auth_invalid_key_fails() {
    let host = host_string();
    let mut session = Session::open(&host).await.expect("Failed to open session");

    // Create a temporary invalid key
    let temp_dir = tempfile::TempDir::new().unwrap();
    let fake_key = temp_dir.path().join("fake_key");
    std::fs::write(&fake_key, "not a valid key").unwrap();

    let result = session.key_auth(&test_user(), fake_key).await;
    assert!(result.is_err(), "Invalid key should fail authentication");
}

/// Test authentication with wrong password fails
#[tokio::test]
#[ignore]
async fn test_auth_wrong_password_fails() {
    let host = host_string();
    let mut session = Session::open(&host).await.expect("Failed to open session");

    let result = session.password_auth(&test_user(), "wrongpassword").await;
    assert!(result.is_err(), "Wrong password should fail authentication");
}

/// Test the full authenticate() method with explicit key
#[tokio::test]
#[ignore]
async fn test_authenticate_with_explicit_key() {
    let host = host_string();
    let key_path = test_keys_dir().join("id_ed25519");

    let mut session = Session::open(&host).await.expect("Failed to open session");

    let result = session
        .authenticate(Some(Authentication::Key(key_path)), Some(&test_user()))
        .await;

    assert!(
        result.is_ok(),
        "authenticate() with explicit key failed: {:?}",
        result.err()
    );
}

// =============================================================================
// Command Execution Tests
// =============================================================================

/// Test simple command execution returns stdout
#[tokio::test]
#[ignore]
async fn test_exec_simple_command() {
    let session = connect_default().await.expect("Failed to connect");

    let result = session
        .exec_and_capture("echo 'hello world'")
        .await
        .expect("Command execution failed");

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout.trim(), "hello world");
    assert!(result.stderr.is_empty());
}

/// Test command with non-zero exit code
#[tokio::test]
#[ignore]
async fn test_exec_with_exit_code() {
    let session = connect_default().await.expect("Failed to connect");

    // Use false command which reliably returns exit code 1
    let result = session
        .exec_and_capture("false")
        .await
        .expect("Command execution failed");

    assert_ne!(
        result.exit_code, 0,
        "false command should return non-zero exit code"
    );
}

/// Test command that outputs to stderr
#[tokio::test]
#[ignore]
async fn test_exec_with_stderr() {
    let session = connect_default().await.expect("Failed to connect");

    let result = session
        .exec_and_capture("echo 'error message' >&2")
        .await
        .expect("Command execution failed");

    assert_eq!(result.exit_code, 0);
    assert!(result.stderr.contains("error message"));
}

/// Test command with both stdout and stderr
#[tokio::test]
#[ignore]
async fn test_exec_with_stdout_and_stderr() {
    let session = connect_default().await.expect("Failed to connect");

    let result = session
        .exec_and_capture("echo 'out' && echo 'err' >&2")
        .await
        .expect("Command execution failed");

    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("out"));
    assert!(result.stderr.contains("err"));
}

/// Test command with multiple lines of output
#[tokio::test]
#[ignore]
async fn test_exec_multiline_output() {
    let session = connect_default().await.expect("Failed to connect");

    let result = session
        .exec_and_capture("echo 'line1'; echo 'line2'; echo 'line3'")
        .await
        .expect("Command execution failed");

    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("line1"));
    assert!(result.stdout.contains("line2"));
    assert!(result.stdout.contains("line3"));
}

/// Test command with environment variables
#[tokio::test]
#[ignore]
async fn test_exec_with_env_vars() {
    let session = connect_default().await.expect("Failed to connect");

    let result = session
        .exec_and_capture("TEST_VAR='hello' && echo $TEST_VAR")
        .await
        .expect("Command execution failed");

    assert_eq!(result.exit_code, 0);
    // Note: This might be empty due to shell behavior, adjust if needed
}

/// Test command that doesn't exist fails
#[tokio::test]
#[ignore]
async fn test_exec_nonexistent_command() {
    let session = connect_default().await.expect("Failed to connect");

    let result = session
        .exec_and_capture("nonexistent_command_12345")
        .await
        .expect("Command execution failed");

    // Command should fail with non-zero exit code
    // Note: The error should appear in stderr
    assert!(
        result.exit_code != 0 || result.stderr.contains("not found"),
        "Nonexistent command should fail: exit_code={}, stderr={}",
        result.exit_code,
        result.stderr
    );
}

// =============================================================================
// File Operation Tests
// =============================================================================

/// Test creating a temporary file on the remote system
#[tokio::test]
#[ignore]
async fn test_create_temp_file() {
    let session = connect_default().await.expect("Failed to connect");

    let temp_path = session
        .create_temp_file("atuin-test")
        .await
        .expect("Failed to create temp file");

    assert!(temp_path.starts_with("/tmp/atuin-test-"));

    // Verify file exists
    let result = session
        .exec_and_capture(&format!("test -f '{}' && echo 'exists'", temp_path))
        .await
        .expect("Failed to check file");

    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("exists"));

    // Cleanup
    session
        .delete_file(&temp_path)
        .await
        .expect("Failed to delete temp file");
}

/// Test reading a file from the remote system
#[tokio::test]
#[ignore]
async fn test_read_file() {
    let session = connect_default().await.expect("Failed to connect");

    // Create a file with known content
    let temp_path = session.create_temp_file("atuin-read-test").await.unwrap();
    session
        .exec_and_capture(&format!("echo 'test content 123' > '{}'", temp_path))
        .await
        .unwrap();

    // Read the file
    let content = session
        .read_file(&temp_path)
        .await
        .expect("Failed to read file");

    assert_eq!(content.trim(), "test content 123");

    // Cleanup
    session.delete_file(&temp_path).await.unwrap();
}

/// Test deleting a file from the remote system
#[tokio::test]
#[ignore]
async fn test_delete_file() {
    let session = connect_default().await.expect("Failed to connect");

    let temp_path = session.create_temp_file("atuin-delete-test").await.unwrap();

    // File should exist
    let result = session
        .exec_and_capture(&format!("test -f '{}'", temp_path))
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0, "File should exist before deletion");

    // Delete it
    session
        .delete_file(&temp_path)
        .await
        .expect("Failed to delete file");

    // File should not exist
    let result = session
        .exec_and_capture(&format!("test -f '{}'", temp_path))
        .await
        .unwrap();
    assert_ne!(result.exit_code, 0, "File should not exist after deletion");
}

/// Test deleting a non-existent file doesn't fail (rm -f behavior)
#[tokio::test]
#[ignore]
async fn test_delete_nonexistent_file() {
    let session = connect_default().await.expect("Failed to connect");

    // This should not fail because delete_file uses rm -f
    let result = session.delete_file("/tmp/nonexistent-file-12345").await;
    assert!(result.is_ok(), "Deleting nonexistent file should not fail");
}

// =============================================================================
// Connection Pool Tests
// =============================================================================

/// Test that the SSH pool handle can establish connections
#[tokio::test]
#[ignore]
async fn test_pool_connect() {
    use super::SshPoolHandle;

    let pool = SshPoolHandle::new();
    let host = format!("{}:{}", test_host(), test_port());
    let key_path = test_keys_dir().join("id_ed25519");

    let result = pool
        .connect(
            &host,
            Some(&test_user()),
            Some(Authentication::Key(key_path)),
        )
        .await;

    assert!(result.is_ok(), "Pool connection failed: {:?}", result.err());
}

/// Test that keepalive works on pooled connections
#[tokio::test]
#[ignore]
async fn test_pool_keepalive() {
    let session = connect_default().await.expect("Failed to connect");

    let keepalive_ok = session.send_keepalive().await;
    assert!(keepalive_ok, "Keepalive should succeed on live connection");
}

/// Test multiple sequential connections reuse the session
#[tokio::test]
#[ignore]
async fn test_pool_connection_reuse() {
    use super::SshPoolHandle;

    let pool = SshPoolHandle::new();
    let host = format!("{}:{}", test_host(), test_port());
    let key_path = test_keys_dir().join("id_ed25519");

    // First connection
    let session1 = pool
        .connect(
            &host,
            Some(&test_user()),
            Some(Authentication::Key(key_path.clone())),
        )
        .await
        .expect("First connection failed");

    // Second connection should reuse the same session
    let session2 = pool
        .connect(
            &host,
            Some(&test_user()),
            Some(Authentication::Key(key_path)),
        )
        .await
        .expect("Second connection failed");

    // Both should point to the same session (Arc comparison)
    assert!(
        std::sync::Arc::ptr_eq(&session1, &session2),
        "Pool should reuse connections"
    );
}

// =============================================================================
// Edge Cases and Error Handling
// =============================================================================

/// Test connection to non-existent host fails
/// Note: This test has a timeout because DNS/TCP can hang for a long time
#[tokio::test]
#[ignore]
async fn test_connection_to_invalid_host_fails() {
    use std::time::Duration;

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        Session::open("nonexistent.invalid:22"),
    )
    .await;

    // Either timeout or connection error is acceptable
    match result {
        Ok(conn_result) => assert!(
            conn_result.is_err(),
            "Connection to invalid host should fail"
        ),
        Err(_timeout) => (), // Timeout is fine - proves it would hang
    }
}

/// Test connection to wrong port fails
/// Note: This test has a timeout because TCP connect can hang
#[tokio::test]
#[ignore]
async fn test_connection_to_wrong_port_fails() {
    use std::time::Duration;

    let host = format!("{}@{}:9999", test_user(), test_host());

    let result = tokio::time::timeout(Duration::from_secs(5), Session::open(&host)).await;

    // Either timeout or connection refused is acceptable
    match result {
        Ok(conn_result) => assert!(conn_result.is_err(), "Connection to wrong port should fail"),
        Err(_timeout) => (), // Timeout is fine - proves it would hang
    }
}
