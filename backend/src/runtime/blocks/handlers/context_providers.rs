use crate::runtime::blocks::context_blocks::{Directory, Environment, Host, LocalVar, SshConnect};
use crate::runtime::blocks::handler::{ContextProvider, ExecutionContext};
use async_trait::async_trait;

pub struct DirectoryHandler;

#[async_trait]
impl ContextProvider for DirectoryHandler {
    type Block = Directory;

    fn block_type(&self) -> &'static str {
        "directory"
    }

    async fn apply_context(
        &self,
        block: &Directory,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        context.cwd = block.path.clone();
        Ok(())
    }
}

pub struct EnvironmentHandler;

#[async_trait]
impl ContextProvider for EnvironmentHandler {
    type Block = Environment;

    fn block_type(&self) -> &'static str {
        "env"
    }

    async fn apply_context(
        &self,
        block: &Environment,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        context.env.insert(block.name.clone(), block.value.clone());
        Ok(())
    }
}

pub struct SshConnectHandler;

#[async_trait]
impl ContextProvider for SshConnectHandler {
    type Block = SshConnect;

    fn block_type(&self) -> &'static str {
        "ssh-connect"
    }

    async fn apply_context(
        &self,
        block: &SshConnect,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        context.ssh_host = Some(block.user_host.clone());
        Ok(())
    }
}

pub struct HostHandler;

#[async_trait]
impl ContextProvider for HostHandler {
    type Block = Host;

    fn block_type(&self) -> &'static str {
        "host-select"
    }

    async fn apply_context(
        &self,
        block: &Host,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let host = block.host.trim();
        if host.is_empty() || host == "local" || host == "localhost" {
            context.ssh_host = None; // Switch to local execution
        } else {
            context.ssh_host = Some(host.to_string()); // Switch to SSH execution
        }
        Ok(())
    }
}

pub struct LocalVarHandler;

#[async_trait]
impl ContextProvider for LocalVarHandler {
    type Block = LocalVar;

    fn block_type(&self) -> &'static str {
        "local-var"
    }

    async fn apply_context(
        &self,
        block: &LocalVar,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !block.name.is_empty() {
            // Get the value from output storage (same as get_template_var command)
            let value = if let Some(output_storage) = &context.output_storage {
                output_storage
                    .read()
                    .await
                    .get(&context.runbook_id.to_string())
                    .and_then(|vars| vars.get(&block.name))
                    .cloned()
                    .unwrap_or_default()
            } else {
                // Fallback to empty if no output storage available
                String::new()
            };

            // Add the variable to the execution context for template substitution
            context.variables.insert(block.name.clone(), value);
        }
        Ok(())
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_directory_handler() {
        let handler = DirectoryHandler;
        let dir = Directory::builder()
            .id(Uuid::new_v4())
            .path("/tmp/test")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&dir, &mut context).await.unwrap();

        assert_eq!(context.cwd, "/tmp/test");
    }

    #[tokio::test]
    async fn test_environment_handler() {
        let handler = EnvironmentHandler;
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("TEST_VAR")
            .value("test_value")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&env, &mut context).await.unwrap();

        assert_eq!(context.env.get("TEST_VAR"), Some(&"test_value".to_string()));
    }

    #[tokio::test]
    async fn test_ssh_handler() {
        let handler = SshConnectHandler;
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("user@host.com")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&ssh, &mut context).await.unwrap();

        assert_eq!(context.ssh_host, Some("user@host.com".to_string()));
    }

    #[tokio::test]
    async fn test_host_handler_local() {
        let handler = HostHandler;
        let host = Host::builder().id(Uuid::new_v4()).host("local").build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&host, &mut context).await.unwrap();

        assert_eq!(context.ssh_host, None);
    }

    #[tokio::test]
    async fn test_host_handler_localhost() {
        let handler = HostHandler;
        let host = Host::builder().id(Uuid::new_v4()).host("localhost").build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&host, &mut context).await.unwrap();

        assert_eq!(context.ssh_host, None);
    }

    #[tokio::test]
    async fn test_host_handler_empty() {
        let handler = HostHandler;
        let host = Host::builder().id(Uuid::new_v4()).host("").build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&host, &mut context).await.unwrap();

        assert_eq!(context.ssh_host, None);
    }

    #[tokio::test]
    async fn test_host_handler_remote() {
        let handler = HostHandler;
        let host = Host::builder()
            .id(Uuid::new_v4())
            .host("user@remote.com")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&host, &mut context).await.unwrap();

        assert_eq!(context.ssh_host, Some("user@remote.com".to_string()));
    }

    #[tokio::test]
    async fn test_host_handler_whitespace() {
        let handler = HostHandler;
        let host = Host::builder()
            .id(Uuid::new_v4())
            .host("  localhost  ")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&host, &mut context).await.unwrap();

        assert_eq!(context.ssh_host, None);
    }

    #[tokio::test]
    async fn test_local_var_handler_empty_name() {
        let handler = LocalVarHandler;
        let local_var = LocalVar::builder().id(Uuid::new_v4()).name("").build();

        let mut context = ExecutionContext::default();
        handler
            .apply_context(&local_var, &mut context)
            .await
            .unwrap();

        // Should not add anything to variables for empty name
        assert!(context.variables.is_empty());
    }

    #[tokio::test]
    async fn test_local_var_handler_with_name() {
        let handler = LocalVarHandler;
        let local_var = LocalVar::builder()
            .id(Uuid::new_v4())
            .name("test_var")
            .build();

        let mut context = ExecutionContext::default();
        handler
            .apply_context(&local_var, &mut context)
            .await
            .unwrap();

        // Should add empty value for the variable (since no stored value in test)
        assert_eq!(context.variables.get("test_var"), Some(&String::new()));
    }

    #[test]
    fn test_local_var_handler_block_type() {
        let handler = LocalVarHandler;
        assert_eq!(handler.block_type(), "local-var");
    }
}
