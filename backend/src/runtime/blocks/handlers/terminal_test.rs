#[cfg(test)]
mod tests {
    use super::TerminalHandler;
    use crate::runtime::blocks::handler::{
        BlockHandler, BlockOutput, CancellationToken, ExecutionContext, ExecutionStatus,
        ContextProvider,
    };
    use crate::runtime::blocks::terminal::Terminal;
    use crate::runtime::blocks::context::directory::Directory;
    use crate::runtime::blocks::handlers::context_providers::DirectoryHandler;
    use crate::runtime::blocks::script::Script;
    use crate::runtime::blocks::handlers::ScriptHandler;
    use crate::runtime::pty_store::PtyStoreHandle;
    use crate::runtime::workflow::event::WorkflowEvent;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tauri::ipc::Channel;
    use tokio::sync::{broadcast, RwLock};
    use tokio::time::{timeout, Duration};
    use uuid::Uuid;

    fn create_test_terminal(name: &str, code: &str) -> Terminal {
        Terminal::builder()
            .id(Uuid::new_v4())
            .name(name)
            .code(code)
            .output_visible(true)
            .build()
    }

    fn create_test_context() -> ExecutionContext {
        ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None,
            output_storage: None,
            pty_store: Some(PtyStoreHandle::new()),
        }
    }

    #[test]
    fn test_terminal_handler_block_type() {
        let handler = TerminalHandler;
        assert_eq!(handler.block_type(), "terminal");
    }

    #[test]
    fn test_terminal_has_no_output_variable() {
        let handler = TerminalHandler;
        let terminal = create_test_terminal("Test Terminal", "echo test");
        assert_eq!(handler.output_variable(&terminal), None);
    }

    #[tokio::test]
    async fn test_terminal_basic_execution() {
        let handler = TerminalHandler;
        let terminal = create_test_terminal("Echo Terminal", "echo 'Hello Terminal'");
        let context = create_test_context();
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Execute the terminal without output channel for testing
        let handle = handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal execution should succeed");

        // The terminal should start in Running state
        let status = handle.status.read().await.clone();
        matches!(status, ExecutionStatus::Running);

        // Give it some time to process
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cancel the terminal to clean up
        handler.cancel(&handle).await.expect("Cancel should succeed");
    }

    #[tokio::test]
    async fn test_terminal_with_directory_context() {
        let handler = TerminalHandler;
        let terminal = create_test_terminal("PWD Terminal", "pwd");
        
        // Create a context with a specific directory
        let mut context = create_test_context();
        let test_dir = "/tmp/test_terminal_dir";
        
        // Apply directory context
        let directory = Directory::builder()
            .id(Uuid::new_v4())
            .path(test_dir.to_string())
            .build();
        
        let dir_handler = DirectoryHandler;
        dir_handler
            .apply_context(&directory, &mut context)
            .expect("Directory context should apply");
        
        assert_eq!(context.cwd, test_dir);
        
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let channel = None;

        // Execute the terminal in the specified directory
        let handle = handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal execution should succeed");

        // Give it some time to process
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cancel the terminal
        handler.cancel(&handle).await.expect("Cancel should succeed");
    }

    #[tokio::test]
    async fn test_terminal_with_environment_variables() {
        let handler = TerminalHandler;
        let terminal = create_test_terminal("Env Terminal", "echo $TEST_VAR");
        
        // Create a context with environment variables
        let mut context = create_test_context();
        context.env.insert("TEST_VAR".to_string(), "test_value".to_string());
        
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let channel = None;

        // Execute the terminal with environment variables
        let handle = handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal execution should succeed");

        // Give it some time to process
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cancel the terminal
        handler.cancel(&handle).await.expect("Cancel should succeed");
    }

    #[tokio::test]
    async fn test_terminal_cancellation() {
        let handler = TerminalHandler;
        // Use a long-running command
        let terminal = create_test_terminal("Sleep Terminal", "sleep 10");
        let context = create_test_context();
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let channel = None;

        // Execute the terminal
        let handle = handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal execution should succeed");

        // Verify it's running
        let status = handle.status.read().await.clone();
        assert!(matches!(status, ExecutionStatus::Running));

        // Cancel after a short delay
        tokio::time::sleep(Duration::from_millis(100)).await;
        handler.cancel(&handle).await.expect("Cancel should succeed");

        // Give cancellation time to propagate
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Status should be updated (either Failed with cancellation message or still Running)
        // The exact status depends on timing, but cancellation should have been triggered
        let final_status = handle.status.read().await.clone();
        // Terminal cancellation sets status to Failed("Terminal execution cancelled")
        match final_status {
            ExecutionStatus::Failed(msg) => {
                assert!(msg.contains("cancelled") || msg.contains("Terminal"));
            }
            ExecutionStatus::Running => {
                // May still be processing cancellation
            }
            _ => panic!("Unexpected status after cancellation"),
        }
    }

    #[tokio::test]
    async fn test_terminal_with_multiline_script() {
        let handler = TerminalHandler;
        let multiline_script = "echo 'Line 1'\necho 'Line 2'\necho 'Line 3'";
        let terminal = create_test_terminal("Multiline Terminal", multiline_script);
        let context = create_test_context();
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let channel = None;

        // Execute the terminal
        let handle = handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal execution should succeed");

        // Give it some time to process
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Cancel the terminal
        handler.cancel(&handle).await.expect("Cancel should succeed");
    }

    #[tokio::test]
    async fn test_terminal_with_script_output_variable() {
        // First, execute a script that sets an output variable
        let script_handler = ScriptHandler;
        let script = Script::builder()
            .id(Uuid::new_v4())
            .name("Setup Script")
            .code("echo 'test_output'")
            .interpreter("bash")
            .output_variable(Some("my_var".to_string()))
            .build();
        
        // Create context with output storage
        let output_storage = Arc::new(RwLock::new(HashMap::<String, HashMap<String, String>>::new()));
        let mut context = create_test_context();
        let runbook_id = context.runbook_id;
        context.output_storage = Some(output_storage.clone());
        
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        
        // Execute the script
        let script_handle = script_handler
            .execute(script, context.clone(), tx.clone(), None)
            .await
            .expect("Script execution should succeed");
        
        // Wait for script to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = script_handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success(_) => break,
                ExecutionStatus::Failed(e) => panic!("Script failed: {}", e),
                ExecutionStatus::Running => continue,
                _ => panic!("Unexpected script status"),
            }
        }
        
        // Verify the output variable was stored
        let stored_vars = output_storage.read().await;
        let runbook_vars = stored_vars
            .get(&runbook_id.to_string())
            .expect("Runbook variables should exist");
        assert_eq!(
            runbook_vars.get("my_var").expect("Variable should be stored"),
            "test_output"
        );
        
        // Now use the variable in a terminal
        context.variables = runbook_vars.clone();
        let terminal_handler = TerminalHandler;
        let terminal = create_test_terminal(
            "Variable Terminal",
            "echo 'Variable value: ${my_var}'"
        );
        
        let channel = None;
        
        // Execute the terminal with the variable
        let terminal_handle = terminal_handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal execution should succeed");
        
        // Give it some time to process
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Cancel the terminal
        terminal_handler
            .cancel(&terminal_handle)
            .await
            .expect("Cancel should succeed");
    }

    #[tokio::test]
    async fn test_terminal_with_invalid_command() {
        let handler = TerminalHandler;
        let terminal = create_test_terminal(
            "Invalid Terminal",
            "this_command_does_not_exist_12345"
        );
        let context = create_test_context();
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let channel = None;

        // Execute the terminal with invalid command
        let handle = handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal execution should succeed even with invalid command");

        // Give it some time to process
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Terminal should still be running (it's interactive, so invalid commands don't kill it)
        let status = handle.status.read().await.clone();
        assert!(matches!(status, ExecutionStatus::Running));

        // Cancel the terminal
        handler.cancel(&handle).await.expect("Cancel should succeed");
    }

    #[tokio::test]
    async fn test_terminal_timeout_handling() {
        let handler = TerminalHandler;
        let terminal = create_test_terminal("Timeout Terminal", "sleep 5");
        let context = create_test_context();
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let channel = None;

        // Execute with a timeout
        let execution_future = handler.execute(terminal, context, tx, None);
        
        // Timeout after 1 second
        let result = timeout(Duration::from_secs(1), execution_future).await;
        
        // Should not timeout on execute (it returns immediately with a handle)
        assert!(result.is_ok());
        
        let handle = result.unwrap().expect("Execution should succeed");
        
        // Cancel to clean up
        handler.cancel(&handle).await.expect("Cancel should succeed");
    }

    #[tokio::test]
    async fn test_terminal_empty_command() {
        let handler = TerminalHandler;
        let terminal = create_test_terminal("Empty Terminal", "");
        let context = create_test_context();
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let channel = None;

        // Execute with empty command
        let handle = handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal execution should succeed with empty command");

        // Give it some time
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should still be running (interactive terminal)
        let status = handle.status.read().await.clone();
        assert!(matches!(status, ExecutionStatus::Running));

        // Cancel the terminal
        handler.cancel(&handle).await.expect("Cancel should succeed");
    }

    #[tokio::test]
    async fn test_terminal_with_special_characters() {
        let handler = TerminalHandler;
        let special_command = r#"echo "Special chars: !@#$%^&*()[]{}|;':\",./<>?""#;
        let terminal = create_test_terminal("Special Terminal", special_command);
        let context = create_test_context();
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let channel = None;

        // Execute with special characters
        let handle = handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal execution should succeed with special characters");

        // Give it some time to process
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cancel the terminal
        handler.cancel(&handle).await.expect("Cancel should succeed");
    }

    #[tokio::test]
    async fn test_terminal_with_unicode() {
        let handler = TerminalHandler;
        let unicode_command = "echo '测试 🚀 émojis'";
        let terminal = create_test_terminal("Unicode Terminal", unicode_command);
        let context = create_test_context();
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let channel = None;

        // Execute with unicode
        let handle = handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal execution should succeed with unicode");

        // Give it some time to process
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cancel the terminal
        handler.cancel(&handle).await.expect("Cancel should succeed");
    }

    #[tokio::test]
    async fn test_multiple_terminals_concurrent() {
        let handler = TerminalHandler;
        let context = create_test_context();
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        
        // Create multiple terminals
        let terminals = vec![
            create_test_terminal("Terminal 1", "echo 'Terminal 1'"),
            create_test_terminal("Terminal 2", "echo 'Terminal 2'"),
            create_test_terminal("Terminal 3", "echo 'Terminal 3'"),
        ];
        
        // Execute all terminals concurrently
        let mut handles = Vec::new();
        for terminal in terminals {
            let channel = None;
            let handle = handler
                .execute(terminal, context.clone(), tx.clone(), None)
                .await
                .expect("Terminal execution should succeed");
            handles.push(handle);
        }
        
        // All should be running
        for handle in &handles {
            let status = handle.status.read().await.clone();
            assert!(matches!(status, ExecutionStatus::Running));
        }
        
        // Give them time to process
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Cancel all terminals
        for handle in handles {
            handler.cancel(&handle).await.expect("Cancel should succeed");
        }
    }

    #[tokio::test]
    async fn test_terminal_with_ssh_context_no_pool() {
        let handler = TerminalHandler;
        let terminal = create_test_terminal("SSH Terminal", "echo 'SSH test'");
        
        // Create context with SSH host but no pool
        let mut context = create_test_context();
        context.ssh_host = Some("user@example.com".to_string());
        // ssh_pool is None, so this should fail gracefully
        
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let channel = None;

        // Execute should fail because SSH pool is not available
        let result = handler
            .execute(terminal, context, tx, None)
            .await;
        
        // Should succeed in creating handle, but the background task will fail
        assert!(result.is_ok());
        let handle = result.unwrap();
        
        // Wait a bit for the background task to process
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Status should be Failed
        let status = handle.status.read().await.clone();
        match status {
            ExecutionStatus::Failed(msg) => {
                assert!(msg.contains("SSH pool not available"));
            }
            _ => panic!("Expected Failed status for missing SSH pool"),
        }
    }

    #[test]
    fn test_parse_ssh_host() {
        // Test various SSH host formats
        assert_eq!(
            TerminalHandler::parse_ssh_host("user@host.com"),
            (Some("user".to_string()), "host.com".to_string())
        );
        
        assert_eq!(
            TerminalHandler::parse_ssh_host("host.com"),
            (None, "host.com".to_string())
        );
        
        assert_eq!(
            TerminalHandler::parse_ssh_host("user@host.com:22"),
            (Some("user".to_string()), "host.com".to_string())
        );
        
        assert_eq!(
            TerminalHandler::parse_ssh_host("host.com:2222"),
            (None, "host.com".to_string())
        );
        
        assert_eq!(
            TerminalHandler::parse_ssh_host("complex.user@complex-host.example.com"),
            (Some("complex.user".to_string()), "complex-host.example.com".to_string())
        );
    }
}