/// End-to-end test for script output variable storage and templating
#[cfg(test)]
mod tests {
    use crate::runtime::blocks::handler::{BlockHandler, ExecutionContext, ExecutionStatus};
    use crate::runtime::blocks::handlers::script::ScriptHandler;
    use crate::runtime::blocks::script::Script;
    use crate::runtime::workflow::event::WorkflowEvent;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::{broadcast, RwLock};
    use uuid::Uuid;

    #[tokio::test]
    async fn test_script_chaining_with_output_variables() {
        // Create shared output storage
        let output_storage = Arc::new(RwLock::new(
            HashMap::<String, HashMap<String, String>>::new(),
        ));
        let runbook_id = Uuid::new_v4();

        // Create event channel
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Script 1: Generate some output
        let script1 = Script::builder()
            .id(Uuid::new_v4())
            .name("Generate Output")
            .code("echo 'Hello from script 1'")
            .interpreter("bash")
            .output_variable(Some("greeting".to_string()))
            .build();

        // Create context for script 1
        let context1 = ExecutionContext {
            runbook_id,
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: HashMap::new(),
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None,
            output_storage: Some(output_storage.clone()),
            pty_store: None,
            event_bus: None,
        };

        // Execute script 1
        let handler = ScriptHandler;
        let handle1 = handler
            .execute(script1, context1, tx.clone(), None)
            .await
            .unwrap();

        // Wait for script 1 to complete
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle1.status.read().await.clone();
            match status {
                ExecutionStatus::Success(_) => break,
                ExecutionStatus::Failed(e) => panic!("Script 1 failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Script 1 was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }

        // Verify output was stored
        {
            let stored = output_storage.read().await;
            let runbook_vars = stored
                .get(&runbook_id.to_string())
                .expect("Runbook should have variables");
            assert_eq!(
                runbook_vars
                    .get("greeting")
                    .expect("greeting should be stored"),
                "Hello from script 1"
            );
        }

        // Script 2: Use the output from script 1
        let script2 = Script::builder()
            .id(Uuid::new_v4())
            .name("Use Previous Output")
            .code("echo \"Previous greeting was: {{ var.greeting }}\"")
            .interpreter("bash")
            .output_variable(Some("combined".to_string()))
            .build();

        // Create context for script 2 with the stored variables
        let stored_vars = output_storage
            .read()
            .await
            .get(&runbook_id.to_string())
            .cloned()
            .unwrap_or_default();

        let context2 = ExecutionContext {
            runbook_id,
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: stored_vars,
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None,
            output_storage: Some(output_storage.clone()),
            pty_store: None,
            event_bus: None,
        };

        // Execute script 2
        let handle2 = handler
            .execute(script2, context2, tx.clone(), None)
            .await
            .unwrap();

        // Wait for script 2 to complete
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle2.status.read().await.clone();
            match status {
                ExecutionStatus::Success(output) => {
                    // Verify the output contains the templated value
                    assert!(output.contains("Previous greeting was: Hello from script 1"));
                    break;
                }
                ExecutionStatus::Failed(e) => panic!("Script 2 failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Script 2 was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }

        // Verify both outputs are stored
        {
            let stored = output_storage.read().await;
            let runbook_vars = stored
                .get(&runbook_id.to_string())
                .expect("Runbook should have variables");
            assert_eq!(
                runbook_vars
                    .get("greeting")
                    .expect("greeting should be stored"),
                "Hello from script 1"
            );
            assert_eq!(
                runbook_vars
                    .get("combined")
                    .expect("combined should be stored"),
                "Previous greeting was: Hello from script 1"
            );
        }
    }

    #[tokio::test]
    async fn test_multiple_scripts_with_dependencies() {
        let output_storage = Arc::new(RwLock::new(
            HashMap::<String, HashMap<String, String>>::new(),
        ));
        let runbook_id = Uuid::new_v4();
        let (tx, _rx) = broadcast::channel::<WorkflowEvent>(16);

        // Script 1: Get current date
        let script1 = Script::builder()
            .id(Uuid::new_v4())
            .name("Get Date")
            .code("date +%Y-%m-%d")
            .interpreter("bash")
            .output_variable(Some("current_date".to_string()))
            .build();

        // Script 2: Get hostname
        let script2 = Script::builder()
            .id(Uuid::new_v4())
            .name("Get Hostname")
            .code("hostname")
            .interpreter("bash")
            .output_variable(Some("host".to_string()))
            .build();

        // Execute both scripts
        let handler = ScriptHandler;

        for script in [script1, script2] {
            let context = ExecutionContext {
                runbook_id,
                cwd: std::env::temp_dir().to_string_lossy().to_string(),
                env: HashMap::new(),
                variables: HashMap::new(),
                ssh_host: None,
                document: Vec::new(),
                ssh_pool: None,
                output_storage: Some(output_storage.clone()),
                pty_store: None,
                event_bus: None,
            };

            let handle = handler
                .execute(script, context, tx.clone(), None)
                .await
                .unwrap();

            // Wait for completion
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                let status = handle.status.read().await.clone();
                if !matches!(status, ExecutionStatus::Running) {
                    break;
                }
            }
        }

        // Script 3: Use both outputs
        let stored_vars = output_storage
            .read()
            .await
            .get(&runbook_id.to_string())
            .cloned()
            .unwrap_or_default();

        let script3 = Script::builder()
            .id(Uuid::new_v4())
            .name("Combine Outputs")
            .code("echo \"Report for {{ var.host }} on {{ var.current_date }}\"")
            .interpreter("bash")
            .output_variable(Some("report".to_string()))
            .build();

        let context3 = ExecutionContext {
            runbook_id,
            cwd: std::env::temp_dir().to_string_lossy().to_string(),
            env: HashMap::new(),
            variables: stored_vars.clone(),
            ssh_host: None,
            document: Vec::new(),
            ssh_pool: None,
            output_storage: Some(output_storage.clone()),
            pty_store: None,
            event_bus: None,
        };

        let handle3 = handler
            .execute(script3, context3, tx.clone(), None)
            .await
            .unwrap();

        // Wait and verify
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let status = handle3.status.read().await.clone();
            match status {
                ExecutionStatus::Success(output) => {
                    // Verify the output contains both templated values
                    assert!(output.contains("Report for"));
                    assert!(output.contains(" on "));
                    assert!(output.contains(&stored_vars["current_date"]));
                    assert!(output.contains(&stored_vars["host"]));
                    break;
                }
                ExecutionStatus::Failed(e) => panic!("Script 3 failed: {}", e),
                ExecutionStatus::Cancelled => panic!("Script 3 was cancelled"),
                ExecutionStatus::Running => continue,
            }
        }

        // Verify all outputs are stored
        let stored = output_storage.read().await;
        let runbook_vars = stored
            .get(&runbook_id.to_string())
            .expect("Runbook should have variables");
        assert!(runbook_vars.contains_key("current_date"));
        assert!(runbook_vars.contains_key("host"));
        assert!(runbook_vars.contains_key("report"));
    }
}
