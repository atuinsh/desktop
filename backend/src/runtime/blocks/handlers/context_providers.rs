use crate::runtime::blocks::context_blocks::{Directory, Environment, SshConnect};
use crate::runtime::blocks::handler::{ContextProvider, ExecutionContext};

pub struct DirectoryHandler;

impl ContextProvider for DirectoryHandler {
    type Block = Directory;

    fn block_type(&self) -> &'static str {
        "directory"
    }

    fn apply_context(
        &self,
        block: &Directory,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        context.cwd = block.path.clone();
        Ok(())
    }
}

pub struct EnvironmentHandler;

impl ContextProvider for EnvironmentHandler {
    type Block = Environment;

    fn block_type(&self) -> &'static str {
        "env"
    }

    fn apply_context(
        &self,
        block: &Environment,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        context.env.insert(block.name.clone(), block.value.clone());
        Ok(())
    }
}

pub struct SshConnectHandler;

impl ContextProvider for SshConnectHandler {
    type Block = SshConnect;

    fn block_type(&self) -> &'static str {
        "ssh-connect"
    }

    fn apply_context(
        &self,
        block: &SshConnect,
        context: &mut ExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        context.ssh_host = Some(block.user_host.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_directory_handler() {
        let handler = DirectoryHandler;
        let dir = Directory::builder()
            .id(Uuid::new_v4())
            .path("/tmp/test")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&dir, &mut context).unwrap();

        assert_eq!(context.cwd, "/tmp/test");
    }

    #[test]
    fn test_environment_handler() {
        let handler = EnvironmentHandler;
        let env = Environment::builder()
            .id(Uuid::new_v4())
            .name("TEST_VAR")
            .value("test_value")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&env, &mut context).unwrap();

        assert_eq!(context.env.get("TEST_VAR"), Some(&"test_value".to_string()));
    }

    #[test]
    fn test_ssh_handler() {
        let handler = SshConnectHandler;
        let ssh = SshConnect::builder()
            .id(Uuid::new_v4())
            .user_host("user@host.com")
            .build();

        let mut context = ExecutionContext::default();
        handler.apply_context(&ssh, &mut context).unwrap();

        assert_eq!(context.ssh_host, Some("user@host.com".to_string()));
    }
}