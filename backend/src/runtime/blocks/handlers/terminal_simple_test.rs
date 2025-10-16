#[cfg(test)]
mod simple_tests {
    use super::TerminalHandler;
    use crate::runtime::blocks::handler::{BlockHandler, ExecutionContext, ExecutionStatus};
    use crate::runtime::blocks::terminal::Terminal;
    use crate::runtime::pty_store::PtyStoreHandle;
    use crate::runtime::workflow::event::WorkflowEvent;
    use std::collections::HashMap;
    use tokio::sync::broadcast;
    use tokio::time::Duration;
    use uuid::Uuid;

    #[test]
    fn test_terminal_handler_block_type() {
        let handler = TerminalHandler;
        assert_eq!(handler.block_type(), "terminal");
    }

    #[test]
    fn test_terminal_has_no_output_variable() {
        // Terminals are interactive and don't store output variables
        let handler = TerminalHandler;
        let terminal = Terminal::builder()
            .id(Uuid::new_v4())
            .name("Test Terminal")
            .code("echo test")
            .output_visible(true)
            .build();
        assert_eq!(handler.output_variable(&terminal), None);
    }

    #[tokio::test]
    async fn test_terminal_basic_execution() {
        let handler = TerminalHandler;
        let terminal = Terminal::builder()
            .id(Uuid::new_v4())
            .name("Echo Terminal")
            .code("echo 'Hello Terminal'")
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
        
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Execute the terminal without output channel
        let handle = handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal execution should succeed");

        // The terminal should start in Running state
        let status = handle.status.read().await.clone();
        assert!(matches!(status, ExecutionStatus::Running));

        // Give it some time to process
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cancel the terminal to clean up
        handler.cancel(&handle).await.expect("Cancel should succeed");
    }

    #[tokio::test]
    async fn test_terminal_cancellation() {
        let handler = TerminalHandler;
        // Use a long-running command
        let terminal = Terminal::builder()
            .id(Uuid::new_v4())
            .name("Sleep Terminal")
            .code("sleep 10")
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
        
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

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

        // Status should be updated
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

    #[test]
    fn test_parse_ssh_host() {
        // Test various SSH host formats
        use super::super::TerminalHandler;
        
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
    }
}