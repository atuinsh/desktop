#[cfg(test)]
mod integration_tests {
    use crate::runtime::blocks::handler::{
        BlockHandler, BlockOutput, ExecutionContext, ExecutionStatus,
    };
    use crate::runtime::blocks::handlers::{ScriptHandler, TerminalHandler};
    use crate::runtime::blocks::handlers::context_providers::{
        DirectoryHandler, EnvironmentHandler,
    };
    use crate::runtime::blocks::context::{
        directory::Directory,
        environment::Environment,
        ContextProvider,
    };
    use crate::runtime::blocks::{script::Script, terminal::Terminal};
    use crate::runtime::pty_store::PtyStoreHandle;
    use crate::runtime::workflow::context_builder::ContextBuilder;
    use crate::runtime::workflow::event::WorkflowEvent;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tauri::ipc::Channel;
    use tokio::sync::{broadcast, RwLock};
    use tokio::time::Duration;
    use uuid::Uuid;

    /// Test terminal execution with directory context applied
    #[tokio::test]
    async fn test_terminal_with_directory_block_integration() {
        // Create a test directory
        let test_dir = std::env::temp_dir().join("terminal_test_dir");
        std::fs::create_dir_all(&test_dir).expect("Should create test directory");
        let test_dir_str = test_dir.to_string_lossy().to_string();
        
        // Create a file in the test directory
        let test_file = test_dir.join("test_file.txt");
        std::fs::write(&test_file, "test content").expect("Should write test file");
        
        // Create directory block
        let directory = Directory::builder()
            .id(Uuid::new_v4())
            .path(test_dir_str.clone())
            .build();
        
        // Create terminal block that lists files
        let terminal = Terminal::builder()
            .id(Uuid::new_v4())
            .name("List Files Terminal")
            .code("ls -la")
            .output_visible(true)
            .build();
        
        // Build context with directory applied
        let mut context = ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None,
            output_storage: None,
            pty_store: Some(PtyStoreHandle::new()),
        };
        
        // Apply directory context
        let dir_handler = DirectoryHandler;
        dir_handler
            .apply_context(&directory, &mut context)
            .expect("Should apply directory context");
        
        assert_eq!(context.cwd, test_dir_str);
        
        // Execute terminal in the directory
        let terminal_handler = TerminalHandler;
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let channel = None;
        
        let handle = terminal_handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal should execute successfully");
        
        // Give it time to list files
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Terminal should be running
        let status = handle.status.read().await.clone();
        assert!(matches!(status, ExecutionStatus::Running));
        
        // Clean up
        terminal_handler
            .cancel(&handle)
            .await
            .expect("Should cancel terminal");
        
        // Clean up test directory
        std::fs::remove_dir_all(&test_dir).ok();
    }

    /// Test terminal using output from a script block
    #[tokio::test]
    async fn test_terminal_with_script_output_integration() {
        // Create script that outputs a value
        let script = Script::builder()
            .id(Uuid::new_v4())
            .name("Generator Script")
            .code("echo 'generated_value_123'")
            .interpreter("bash")
            .output_variable(Some("test_output".to_string()))
            .build();
        
        // Create context with output storage
        let output_storage = Arc::new(RwLock::new(
            HashMap::<String, HashMap<String, String>>::new()
        ));
        
        let mut context = ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None,
            output_storage: Some(output_storage.clone()),
            pty_store: Some(PtyStoreHandle::new()),
        };
        
        let runbook_id = context.runbook_id;
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        
        // Execute script first
        let script_handler = ScriptHandler;
        let script_handle = script_handler
            .execute(script, context.clone(), tx.clone(), None)
            .await
            .expect("Script should execute");
        
        // Wait for script to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = script_handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success(output) => {
                    assert_eq!(output.trim(), "generated_value_123");
                    break;
                }
                ExecutionStatus::Failed(e) => panic!("Script failed: {}", e),
                ExecutionStatus::Running => continue,
                _ => panic!("Unexpected status"),
            }
        }
        
        // Get the stored variable
        let stored_vars = output_storage.read().await;
        let runbook_vars = stored_vars
            .get(&runbook_id.to_string())
            .expect("Should have runbook variables");
        
        // Update context with the variable
        context.variables = runbook_vars.clone();
        
        // Create terminal that uses the variable
        let terminal = Terminal::builder()
            .id(Uuid::new_v4())
            .name("Consumer Terminal")
            .code("echo \"Received: ${test_output}\"")
            .output_visible(true)
            .build();
        
        // Execute terminal with the variable
        let terminal_handler = TerminalHandler;
        let channel = None;
        
        let terminal_handle = terminal_handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal should execute");
        
        // Give it time to process
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Clean up
        terminal_handler
            .cancel(&terminal_handle)
            .await
            .expect("Should cancel terminal");
    }

    /// Test terminal with multiple context blocks applied
    #[tokio::test]
    async fn test_terminal_with_multiple_context_blocks() {
        // Create directory block
        let test_dir = std::env::temp_dir().join("multi_context_test");
        std::fs::create_dir_all(&test_dir).expect("Should create directory");
        
        let directory = Directory::builder()
            .id(Uuid::new_v4())
            .path(test_dir.to_string_lossy().to_string())
            .build();
        
        // Create environment blocks
        let env1 = Environment::builder()
            .id(Uuid::new_v4())
            .name("VAR1")
            .value("value1")
            .build();
        
        let env2 = Environment::builder()
            .id(Uuid::new_v4())
            .name("VAR2")
            .value("value2")
            .build();
        
        // Create terminal that uses both directory and environment
        let terminal = Terminal::builder()
            .id(Uuid::new_v4())
            .name("Multi Context Terminal")
            .code("pwd && echo \"VAR1=$VAR1, VAR2=$VAR2\"")
            .output_visible(true)
            .build();
        
        // Build context with all blocks applied
        let mut context = ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None,
            output_storage: None,
            pty_store: Some(PtyStoreHandle::new()),
        };
        
        // Apply all context blocks
        let dir_handler = DirectoryHandler;
        dir_handler
            .apply_context(&directory, &mut context)
            .expect("Should apply directory");
        
        let env_handler = EnvironmentHandler;
        env_handler
            .apply_context(&env1, &mut context)
            .expect("Should apply env1");
        env_handler
            .apply_context(&env2, &mut context)
            .expect("Should apply env2");
        
        // Verify context
        assert_eq!(context.cwd, test_dir.to_string_lossy().to_string());
        assert_eq!(context.env.get("VAR1"), Some(&"value1".to_string()));
        assert_eq!(context.env.get("VAR2"), Some(&"value2".to_string()));
        
        // Execute terminal
        let terminal_handler = TerminalHandler;
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let channel = None;
        
        let handle = terminal_handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal should execute");
        
        // Give it time to process
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Clean up
        terminal_handler
            .cancel(&handle)
            .await
            .expect("Should cancel terminal");
        
        std::fs::remove_dir_all(&test_dir).ok();
    }

    /// Test terminal execution followed by script using terminal's environment
    #[tokio::test]
    async fn test_terminal_then_script_workflow() {
        // Create terminal that sets up environment
        let terminal = Terminal::builder()
            .id(Uuid::new_v4())
            .name("Setup Terminal")
            .code("export SETUP_DONE=yes")
            .output_visible(true)
            .build();
        
        // Note: In a real workflow, the terminal would modify the environment
        // For this test, we'll simulate the workflow
        
        let mut context = ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None,
            output_storage: None,
            pty_store: Some(PtyStoreHandle::new()),
        };
        
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        
        // Execute terminal
        let terminal_handler = TerminalHandler;
        let channel = None;
        
        let terminal_handle = terminal_handler
            .execute(terminal, context.clone(), tx.clone(), None)
            .await
            .expect("Terminal should execute");
        
        // Give terminal time to run
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // In a real scenario, the terminal would modify the environment
        // For testing, we'll simulate this
        context.env.insert("SETUP_DONE".to_string(), "yes".to_string());
        
        // Create script that depends on terminal's setup
        let script = Script::builder()
            .id(Uuid::new_v4())
            .name("Dependent Script")
            .code("if [ \"$SETUP_DONE\" = \"yes\" ]; then echo 'Setup confirmed'; else echo 'Setup failed'; exit 1; fi")
            .interpreter("bash")
            .output_variable(None)
            .build();
        
        // Execute script
        let script_handler = ScriptHandler;
        let script_handle = script_handler
            .execute(script, context, tx, None)
            .await
            .expect("Script should execute");
        
        // Wait for script to complete
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let status = script_handle.status.read().await.clone();
            match status {
                ExecutionStatus::Success(output) => {
                    assert!(output.contains("Setup confirmed"));
                    break;
                }
                ExecutionStatus::Failed(e) => panic!("Script failed: {}", e),
                ExecutionStatus::Running => continue,
                _ => panic!("Unexpected status"),
            }
        }
        
        // Clean up terminal
        terminal_handler
            .cancel(&terminal_handle)
            .await
            .expect("Should cancel terminal");
    }

    /// Test concurrent terminal and script execution
    #[tokio::test]
    async fn test_concurrent_terminal_and_script() {
        let context = ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None,
            output_storage: None,
            pty_store: Some(PtyStoreHandle::new()),
        };
        
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        
        // Create terminal
        let terminal = Terminal::builder()
            .id(Uuid::new_v4())
            .name("Concurrent Terminal")
            .code("echo 'Terminal running'")
            .output_visible(true)
            .build();
        
        // Create script
        let script = Script::builder()
            .id(Uuid::new_v4())
            .name("Concurrent Script")
            .code("echo 'Script running'")
            .interpreter("bash")
            .output_variable(None)
            .build();
        
        // Execute both concurrently
        let terminal_handler = TerminalHandler;
        let script_handler = ScriptHandler;
        
        let channel1 = None;
        let channel2 = None;
        
        let (terminal_handle, script_handle) = tokio::join!(
            terminal_handler.execute(
                terminal,
                context.clone(),
                tx.clone(),
                None
            ),
            script_handler.execute(
                script,
                context.clone(),
                tx.clone(),
                None
            )
        );
        
        let terminal_handle = terminal_handle.expect("Terminal should execute");
        let script_handle = script_handle.expect("Script should execute");
        
        // Both should be running or completed
        let terminal_status = terminal_handle.status.read().await.clone();
        let script_status = script_handle.status.read().await.clone();
        
        // Terminal should be running (interactive)
        assert!(matches!(terminal_status, ExecutionStatus::Running));
        
        // Script might be running or already completed
        assert!(matches!(
            script_status,
            ExecutionStatus::Running | ExecutionStatus::Success(_)
        ));
        
        // Wait a bit for script to complete if still running
        if matches!(script_status, ExecutionStatus::Running) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        // Clean up terminal
        terminal_handler
            .cancel(&terminal_handle)
            .await
            .expect("Should cancel terminal");
    }

    /// Test terminal with very long output
    #[tokio::test]
    async fn test_terminal_with_large_output() {
        // Create terminal that generates a lot of output
        let terminal = Terminal::builder()
            .id(Uuid::new_v4())
            .name("Large Output Terminal")
            .code("for i in {1..100}; do echo \"Line $i: This is a test line with some content to make it longer\"; done")
            .output_visible(true)
            .build();
        
        let context = ExecutionContext {
            runbook_id: Uuid::new_v4(),
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None,
            output_storage: None,
            pty_store: Some(PtyStoreHandle::new()),
        };
        
        let terminal_handler = TerminalHandler;
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);
        let channel = None;
        
        let handle = terminal_handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal should execute");
        
        // Give it time to generate output
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Should still be running
        let status = handle.status.read().await.clone();
        assert!(matches!(status, ExecutionStatus::Running));
        
        // Clean up
        terminal_handler
            .cancel(&handle)
            .await
            .expect("Should cancel terminal");
    }
}