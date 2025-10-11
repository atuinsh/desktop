#[cfg(test)]
mod tests {
    use crate::runtime::blocks::handler::{BlockHandler, ExecutionContext, ExecutionStatus};
    use crate::runtime::blocks::handlers::terminal::TerminalHandler;
    use crate::runtime::blocks::terminal::Terminal;
    use crate::runtime::events::{GCEvent, MemoryEventBus};
    use crate::runtime::pty_store::PtyStoreHandle;
    use crate::runtime::workflow::event::WorkflowEvent;
    use std::collections::HashMap;
    use std::sync::Arc;
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
            event_bus: None,
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
        handler
            .cancel(&handle)
            .await
            .expect("Cancel should succeed");
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
            event_bus: None,
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
        handler
            .cancel(&handle)
            .await
            .expect("Cancel should succeed");

        // Give cancellation time to propagate
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Status should be updated
        let final_status = handle.status.read().await.clone();

        // Terminal cancellation sets status to Failed("Terminal execution cancelled")
        assert!(matches!(final_status, ExecutionStatus::Cancelled));
    }

    #[test]
    fn test_parse_ssh_host() {
        // Test various SSH host formats
        use crate::runtime::blocks::handlers::terminal::TerminalHandler;

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

    fn create_test_context_with_event_bus(event_bus: Arc<MemoryEventBus>) -> ExecutionContext {
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
            event_bus: Some(event_bus),
        }
    }

    #[tokio::test]
    async fn test_grand_central_events_terminal_start() {
        let handler = TerminalHandler;
        let terminal = Terminal::builder()
            .id(Uuid::new_v4())
            .name("Test Terminal")
            .code("echo 'Hello Terminal'")
            .output_visible(true)
            .build();

        // Create memory event bus
        let event_bus = Arc::new(MemoryEventBus::new());
        let context = create_test_context_with_event_bus(event_bus.clone());
        let runbook_id = context.runbook_id;
        let terminal_id = terminal.id;

        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Execute the terminal
        let handle = handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal execution should succeed");

        // Give it some time to start and emit events
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Cancel to clean up
        handler
            .cancel(&handle)
            .await
            .expect("Cancel should succeed");

        // Give cancellation time to complete
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify events were emitted
        let events = event_bus.events();

        // 3 events - block start, pty open, block cancelled
        assert!(
            events.len() == 3,
            "Expected 3 events, got: {}",
            events.len()
        );

        // Check BlockStarted event
        match &events[0] {
            GCEvent::BlockStarted {
                block_id,
                runbook_id: rb_id,
            } => {
                assert_eq!(*block_id, terminal_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!("Expected BlockStarted event, got: {:?}", events[0]),
        }

        // Check PtyOpened event
        match &events[1] {
            GCEvent::PtyOpened(metadata) => {
                assert_eq!(metadata.pid, terminal_id);
                assert_eq!(metadata.runbook, runbook_id);
                assert_eq!(metadata.block, terminal_id.to_string());
            }
            _ => panic!("Expected PtyOpened event, got: {:?}", events[1]),
        }

        let last_event = events.last().unwrap();
        match last_event {
            GCEvent::BlockCancelled {
                block_id,
                runbook_id: rb_id,
            } => {
                assert_eq!(*block_id, terminal_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!(
                "Expected BlockCancelled event at the end, got: {:?}",
                last_event
            ),
        }
    }

    #[tokio::test]
    async fn test_grand_central_events_terminal_cancellation() {
        let handler = TerminalHandler;
        let terminal = Terminal::builder()
            .id(Uuid::new_v4())
            .name("Long Running Terminal")
            .code("sleep 10")
            .output_visible(true)
            .build();

        // Create memory event bus
        let event_bus = Arc::new(MemoryEventBus::new());
        let context = create_test_context_with_event_bus(event_bus.clone());
        let runbook_id = context.runbook_id;
        let terminal_id = terminal.id;

        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Execute the terminal
        let handle = handler
            .execute(terminal, context, tx, None)
            .await
            .expect("Terminal execution should succeed");

        // Give it some time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Cancel the terminal
        handler
            .cancel(&handle)
            .await
            .expect("Cancel should succeed");

        // Give cancellation time to complete
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify events were emitted
        let events = event_bus.events();
        assert!(
            events.len() >= 3,
            "Expected at least 3 events, got: {}",
            events.len()
        );

        // Check BlockStarted event
        match &events[0] {
            GCEvent::BlockStarted {
                block_id,
                runbook_id: rb_id,
            } => {
                assert_eq!(*block_id, terminal_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!("Expected BlockStarted event, got: {:?}", events[0]),
        }

        // Check PtyOpened event
        match &events[1] {
            GCEvent::PtyOpened(metadata) => {
                assert_eq!(metadata.pid, terminal_id);
                assert_eq!(metadata.runbook, runbook_id);
            }
            _ => panic!("Expected PtyOpened event, got: {:?}", events[1]),
        }

        // Check final event (could be BlockCancelled or BlockFinished depending on timing)
        let last_event = events.last().unwrap();
        match last_event {
            GCEvent::BlockCancelled {
                block_id,
                runbook_id: rb_id,
            } => {
                assert_eq!(*block_id, terminal_id);
                assert_eq!(*rb_id, runbook_id);
            }
            GCEvent::BlockFinished {
                block_id,
                runbook_id: rb_id,
                success: _,
            } => {
                // Terminal may finish before cancellation takes effect
                assert_eq!(*block_id, terminal_id);
                assert_eq!(*rb_id, runbook_id);
            }
            _ => panic!(
                "Expected BlockCancelled or BlockFinished event, got: {:?}",
                last_event
            ),
        }
    }
}
