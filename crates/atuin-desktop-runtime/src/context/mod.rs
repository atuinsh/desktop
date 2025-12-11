//! Context management for runbook blocks
//!
//! This module provides context management for sharing state between blocks
//! in a runbook. Context includes:
//!
//! - Variables set by blocks
//! - Environment variables
//! - Working directory
//! - SSH connection information
//! - Block execution outputs
//!
//! Blocks can provide both passive context (available before execution) and
//! active context (produced during execution).

mod block_context;
pub mod fs_var;
mod resolution;
mod storage;

pub use block_context::BlockState;
pub use block_context::{
    BlockContext, BlockContextItem, BlockExecutionOutput, BlockStateUpdater, BlockVars,
    DocumentBlock, DocumentCwd, DocumentEnvVar, DocumentSshHost, DocumentVar, DocumentVars,
};

pub use resolution::{ContextResolver, ResolvedContext};
pub use storage::BlockContextStorage;
pub use typetag::serde as typetag_serde;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use uuid::Uuid;

    use super::*;
    use crate::{
        blocks::{directory::Directory, environment::Environment, var::Var, Block},
        context::resolution::ContextResolverBuilder,
    };

    #[test]
    fn test_block_context_insert_and_get() {
        let mut context = BlockContext::new();

        let var = DocumentVar::new(
            "TEST_VAR".to_string(),
            "test_value".to_string(),
            "test".to_string(),
        );
        context.insert(var.clone());

        let retrieved = context.get::<DocumentVar>();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &var);
    }

    #[test]
    fn test_block_context_multiple_types() {
        let mut context = BlockContext::new();

        let var = DocumentVar::new(
            "TEST_VAR".to_string(),
            "test_value".to_string(),
            "test".to_string(),
        );
        let cwd = DocumentCwd("/tmp/test".to_string());
        let env = DocumentEnvVar("PATH".to_string(), "/usr/bin".to_string());

        context.insert(var.clone());
        context.insert(cwd.clone());
        context.insert(env.clone());

        assert_eq!(context.get::<DocumentVar>(), Some(&var));
        assert_eq!(context.get::<DocumentCwd>(), Some(&cwd));
        assert_eq!(context.get::<DocumentEnvVar>(), Some(&env));
    }

    #[test]
    fn test_block_context_get_nonexistent() {
        let context = BlockContext::new();
        assert!(context.get::<DocumentVar>().is_none());
        assert!(context.get::<DocumentCwd>().is_none());
    }

    #[test]
    fn test_block_context_overwrite_same_type() {
        let mut context = BlockContext::new();

        let var1 = DocumentVar::new("VAR1".to_string(), "value1".to_string(), "test".to_string());
        let var2 = DocumentVar::new("VAR2".to_string(), "value2".to_string(), "test".to_string());

        context.insert(var1);
        context.insert(var2.clone());

        let retrieved = context.get::<DocumentVar>();
        assert_eq!(retrieved, Some(&var2));
    }

    #[test]
    fn test_block_context_serialization_roundtrip() {
        let mut context = BlockContext::new();

        context.insert(DocumentVar::new(
            "TEST_VAR".to_string(),
            "test_value".to_string(),
            "test".to_string(),
        ));
        context.insert(DocumentCwd("/tmp/test".to_string()));
        context.insert(DocumentEnvVar("PATH".to_string(), "/usr/bin".to_string()));

        let serialized = serde_json::to_string(&context).unwrap();
        let deserialized: BlockContext = serde_json::from_str(&serialized).unwrap();

        assert_eq!(
            deserialized.get::<DocumentVar>(),
            Some(&DocumentVar::new(
                "TEST_VAR".to_string(),
                "test_value".to_string(),
                "test".to_string()
            ))
        );
        assert_eq!(
            deserialized.get::<DocumentCwd>(),
            Some(&DocumentCwd("/tmp/test".to_string()))
        );
        assert_eq!(
            deserialized.get::<DocumentEnvVar>(),
            Some(&DocumentEnvVar("PATH".to_string(), "/usr/bin".to_string()))
        );
    }

    #[test]
    fn test_context_resolver_new() {
        let resolver = ContextResolver::new();
        assert!(resolver.vars().is_empty());
        assert!(resolver.env_vars().is_empty());
        assert!(resolver.ssh_host().is_none());
    }

    #[tokio::test]
    async fn test_context_resolver_from_single_block() {
        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("TEST_VAR")
            .value("test_value")
            .build();

        let mut context = BlockContext::new();
        context.insert(DocumentVar::new(
            "TEST_VAR".to_string(),
            "test_value".to_string(),
            "test".to_string(),
        ));

        let block_with_context =
            DocumentBlock::new(Block::Var(var_block), context, None, None, None);

        let resolver = ContextResolver::from_blocks(&[block_with_context]);

        assert_eq!(
            resolver.get_var("TEST_VAR"),
            Some(&"test_value".to_string())
        );
    }

    #[tokio::test]
    async fn test_context_resolver_multiple_blocks() {
        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("VAR1")
            .value("value1")
            .build();

        let env_block = Environment::builder()
            .id(Uuid::new_v4())
            .name("PATH")
            .value("/usr/bin")
            .build();

        let dir_block = Directory::builder()
            .id(Uuid::new_v4())
            .path("/tmp/test")
            .build();

        let mut var_context = BlockContext::new();
        var_context.insert(DocumentVar::new(
            "VAR1".to_string(),
            "value1".to_string(),
            "test".to_string(),
        ));

        let mut env_context = BlockContext::new();
        env_context.insert(DocumentEnvVar("PATH".to_string(), "/usr/bin".to_string()));

        let mut dir_context = BlockContext::new();
        dir_context.insert(DocumentCwd("/tmp/test".to_string()));

        let blocks = vec![
            DocumentBlock::new(Block::Var(var_block), var_context, None, None, None),
            DocumentBlock::new(Block::Environment(env_block), env_context, None, None, None),
            DocumentBlock::new(Block::Directory(dir_block), dir_context, None, None, None),
        ];

        let resolver = ContextResolver::from_blocks(&blocks);

        assert_eq!(resolver.get_var("VAR1"), Some(&"value1".to_string()));
        assert_eq!(
            resolver.env_vars().get("PATH"),
            Some(&"/usr/bin".to_string())
        );
        assert_eq!(resolver.cwd(), "/tmp/test");
    }

    #[tokio::test]
    async fn test_context_resolver_later_blocks_override() {
        let var1 = Var::builder()
            .id(Uuid::new_v4())
            .name("SHARED_VAR")
            .value("first_value")
            .build();

        let var2 = Var::builder()
            .id(Uuid::new_v4())
            .name("SHARED_VAR")
            .value("second_value")
            .build();

        let mut context1 = BlockContext::new();
        context1.insert(DocumentVar::new(
            "SHARED_VAR".to_string(),
            "first_value".to_string(),
            "test".to_string(),
        ));

        let mut context2 = BlockContext::new();
        context2.insert(DocumentVar::new(
            "SHARED_VAR".to_string(),
            "second_value".to_string(),
            "test".to_string(),
        ));

        let blocks = vec![
            DocumentBlock::new(Block::Var(var1), context1, None, None, None),
            DocumentBlock::new(Block::Var(var2), context2, None, None, None),
        ];

        let resolver = ContextResolver::from_blocks(&blocks);

        assert_eq!(
            resolver.get_var("SHARED_VAR"),
            Some(&"second_value".to_string())
        );
    }

    #[test]
    fn test_context_resolver_template_resolution_no_template() {
        let resolver = ContextResolver::new();
        let result = resolver.resolve_template("plain text").unwrap();
        assert_eq!(result, "plain text");
    }

    #[test]
    fn test_context_resolver_template_resolution_with_var() {
        let mut vars = HashMap::new();
        vars.insert("USERNAME".to_string(), "alice".to_string());

        let resolver = ContextResolver::with_vars(vars);
        let result = resolver
            .resolve_template("Hello, {{ var.USERNAME }}!")
            .unwrap();
        assert_eq!(result, "Hello, alice!");
    }

    #[test]
    fn test_context_resolver_template_resolution_with_multiple_vars() {
        let mut vars = HashMap::new();
        vars.insert("HOST".to_string(), "example.com".to_string());
        vars.insert("PORT".to_string(), "8080".to_string());

        let resolver = ContextResolver::with_vars(vars);
        let result = resolver
            .resolve_template("Connect to {{ var.HOST }}:{{ var.PORT }}")
            .unwrap();
        assert_eq!(result, "Connect to example.com:8080");
    }

    #[test]
    fn test_context_resolver_template_resolution_with_env() {
        let resolver = ContextResolverBuilder::new()
            .vars(HashMap::new())
            .cwd("/test".to_string())
            .env_vars(HashMap::from([(
                "PATH".to_string(),
                "/usr/bin".to_string(),
            )]))
            .extra_template_context(HashMap::new())
            .build();

        let result = resolver.resolve_template("PATH is {{ env.PATH }}").unwrap();
        assert_eq!(result, "PATH is /usr/bin");
    }

    #[test]
    fn test_context_resolver_template_resolution_var_and_env() {
        let mut vars = HashMap::new();
        vars.insert("USER".to_string(), "bob".to_string());

        let mut env_vars = HashMap::new();
        env_vars.insert("HOME".to_string(), "/home/bob".to_string());

        let resolver = ContextResolverBuilder::new()
            .vars(vars)
            .cwd("/test".to_string())
            .env_vars(env_vars)
            .extra_template_context(HashMap::new())
            .build();

        let result = resolver
            .resolve_template("User {{ var.USER }} has home {{ env.HOME }}")
            .unwrap();
        assert_eq!(result, "User bob has home /home/bob");
    }

    #[tokio::test]
    async fn test_context_resolver_push_block() {
        let mut resolver = ContextResolver::new();

        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("NEW_VAR")
            .value("new_value")
            .build();

        let mut context = BlockContext::new();
        context.insert(DocumentVar::new(
            "NEW_VAR".to_string(),
            "new_value".to_string(),
            "test".to_string(),
        ));

        let block_with_context =
            DocumentBlock::new(Block::Var(var_block), context, None, None, None);

        resolver.push_block(&block_with_context);

        assert_eq!(resolver.get_var("NEW_VAR"), Some(&"new_value".to_string()));
    }

    #[tokio::test]
    async fn test_context_resolver_push_block_with_active_context() {
        let mut resolver = ContextResolver::new();

        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("PASSIVE_VAR")
            .value("passive")
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentVar::new(
            "PASSIVE_VAR".to_string(),
            "passive".to_string(),
            "test".to_string(),
        ));

        let mut block_with_context =
            DocumentBlock::new(Block::Var(var_block), passive_context, None, None, None);

        let mut active_context = BlockContext::new();
        active_context.insert(DocumentVar::new(
            "ACTIVE_VAR".to_string(),
            "active".to_string(),
            "test".to_string(),
        ));
        block_with_context.replace_active_context(active_context);

        resolver.push_block(&block_with_context);

        assert_eq!(
            resolver.get_var("PASSIVE_VAR"),
            Some(&"passive".to_string())
        );
        assert_eq!(resolver.get_var("ACTIVE_VAR"), Some(&"active".to_string()));
    }

    #[test]
    fn test_resolved_context_from_resolver() {
        let mut vars = HashMap::new();
        vars.insert("TEST_VAR".to_string(), "test_value".to_string());

        let mut env_vars = HashMap::new();
        env_vars.insert("PATH".to_string(), "/usr/bin".to_string());

        let resolver = ContextResolverBuilder::new()
            .vars(vars.clone())
            .cwd("/tmp/test".to_string())
            .env_vars(env_vars.clone())
            .ssh_host("example.com".to_string())
            .extra_template_context(HashMap::new())
            .build();

        let resolved = ResolvedContext::from_resolver(&resolver);

        assert_eq!(resolved.variables, vars);
        assert_eq!(resolved.cwd, "/tmp/test");
        assert_eq!(resolved.env_vars, env_vars);
        assert_eq!(resolved.ssh_host, Some("example.com".to_string()));
    }

    #[test]
    fn test_resolved_context_serialization_roundtrip() {
        let mut vars = HashMap::new();
        vars.insert("VAR1".to_string(), "value1".to_string());

        let mut env_vars = HashMap::new();
        env_vars.insert("ENV1".to_string(), "envvalue1".to_string());

        let original = ResolvedContext {
            variables: vars,
            variables_sources: HashMap::new(),
            cwd: "/test/path".to_string(),
            env_vars,
            ssh_host: Some("test.example.com".to_string()),
        };

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: ResolvedContext = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original.variables, deserialized.variables);
        assert_eq!(original.cwd, deserialized.cwd);
        assert_eq!(original.env_vars, deserialized.env_vars);
        assert_eq!(original.ssh_host, deserialized.ssh_host);
    }

    #[test]
    fn test_resolved_context_default() {
        let resolved = ResolvedContext::default();
        assert!(resolved.variables.is_empty());
        assert!(resolved.env_vars.is_empty());
        assert!(resolved.ssh_host.is_none());
    }

    #[tokio::test]
    async fn test_block_with_context_accessors() {
        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("TEST")
            .value("value")
            .build();

        let block_id = var_block.id;

        let mut context = BlockContext::new();
        context.insert(DocumentVar::new(
            "TEST".to_string(),
            "value".to_string(),
            "test".to_string(),
        ));

        let block_with_context =
            DocumentBlock::new(Block::Var(var_block), context, None, None, None);

        assert_eq!(block_with_context.id(), block_id);
        assert!(block_with_context
            .passive_context()
            .get::<DocumentVar>()
            .is_some());
        assert!(block_with_context
            .active_context()
            .get::<DocumentVar>()
            .is_none());
    }

    #[tokio::test]
    async fn test_block_with_context_update_contexts() {
        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("TEST")
            .value("value")
            .build();

        let mut block_with_context =
            DocumentBlock::new(Block::Var(var_block), BlockContext::new(), None, None, None);

        let mut new_passive = BlockContext::new();
        new_passive.insert(DocumentVar::new(
            "NEW_VAR".to_string(),
            "new_value".to_string(),
            "test".to_string(),
        ));
        block_with_context.replace_passive_context(new_passive);

        let mut new_active = BlockContext::new();
        new_active.insert(DocumentCwd("/new/path".to_string()));
        block_with_context.replace_active_context(new_active);

        assert!(block_with_context
            .passive_context()
            .get::<DocumentVar>()
            .is_some());
        assert!(block_with_context
            .active_context()
            .get::<DocumentCwd>()
            .is_some());
    }

    #[tokio::test]
    async fn test_document_context_items_equality() {
        let var1 = DocumentVar::new("name".to_string(), "value".to_string(), "test".to_string());
        let var2 = DocumentVar::new("name".to_string(), "value".to_string(), "test".to_string());
        let var3 = DocumentVar::new(
            "name".to_string(),
            "different".to_string(),
            "test".to_string(),
        );

        assert_eq!(var1, var2);
        assert_ne!(var1, var3);
    }

    #[test]
    fn test_document_ssh_host_none() {
        let ssh_host = DocumentSshHost(None);
        let mut context = BlockContext::new();
        context.insert(ssh_host.clone());

        let retrieved = context.get::<DocumentSshHost>();
        assert_eq!(retrieved, Some(&DocumentSshHost(None)));
    }

    #[test]
    fn test_document_ssh_host_some() {
        let ssh_host = DocumentSshHost(Some("example.com".to_string()));
        let mut context = BlockContext::new();
        context.insert(ssh_host.clone());

        let retrieved = context.get::<DocumentSshHost>();
        assert_eq!(
            retrieved,
            Some(&DocumentSshHost(Some("example.com".to_string())))
        );
    }

    #[test]
    fn test_template_with_nested_variables() {
        let mut vars = HashMap::new();
        vars.insert("BASE_URL".to_string(), "api.example.com".to_string());
        vars.insert("VERSION".to_string(), "v1".to_string());
        vars.insert("ENDPOINT".to_string(), "users".to_string());

        let resolver = ContextResolver::with_vars(vars);
        let result = resolver
            .resolve_template("https://{{ var.BASE_URL }}/{{ var.VERSION }}/{{ var.ENDPOINT }}")
            .unwrap();
        assert_eq!(result, "https://api.example.com/v1/users");
    }

    #[test]
    fn test_template_with_extra_template_context() {
        let mut extra = HashMap::new();
        extra.insert("BASE_URL".to_string(), "api.example.com".to_string());
        extra.insert("VERSION".to_string(), "v1".to_string());
        extra.insert("ENDPOINT".to_string(), "users".to_string());

        let mut resolver = ContextResolver::new();
        resolver.add_extra_template_context("extra".to_string(), extra);
        let result = resolver
            .resolve_template(
                "https://{{ extra.BASE_URL }}/{{ extra.VERSION }}/{{ extra.ENDPOINT }}",
            )
            .unwrap();
        assert_eq!(result, "https://api.example.com/v1/users");
    }

    #[tokio::test]
    async fn test_complex_context_scenario() {
        let var1 = Var::builder()
            .id(Uuid::new_v4())
            .name("USERNAME")
            .value("alice")
            .build();

        let env1 = Environment::builder()
            .id(Uuid::new_v4())
            .name("HOME")
            .value("/home/alice")
            .build();

        let dir1 = Directory::builder()
            .id(Uuid::new_v4())
            .path("/home/alice/projects")
            .build();

        let var2 = Var::builder()
            .id(Uuid::new_v4())
            .name("PROJECT")
            .value("myapp")
            .build();

        let mut context1 = BlockContext::new();
        context1.insert(DocumentVar::new(
            "USERNAME".to_string(),
            "alice".to_string(),
            "test".to_string(),
        ));

        let mut context2 = BlockContext::new();
        context2.insert(DocumentEnvVar(
            "HOME".to_string(),
            "/home/alice".to_string(),
        ));

        let mut context3 = BlockContext::new();
        context3.insert(DocumentCwd("/home/alice/projects".to_string()));

        let mut context4 = BlockContext::new();
        context4.insert(DocumentVar::new(
            "PROJECT".to_string(),
            "myapp".to_string(),
            "test".to_string(),
        ));

        let blocks = vec![
            DocumentBlock::new(Block::Var(var1), context1, None, None, None),
            DocumentBlock::new(Block::Environment(env1), context2, None, None, None),
            DocumentBlock::new(Block::Directory(dir1), context3, None, None, None),
            DocumentBlock::new(Block::Var(var2), context4, None, None, None),
        ];

        let resolver = ContextResolver::from_blocks(&blocks);
        let resolved = ResolvedContext::from_resolver(&resolver);

        assert_eq!(
            resolved.variables.get("USERNAME"),
            Some(&"alice".to_string())
        );
        assert_eq!(
            resolved.variables.get("PROJECT"),
            Some(&"myapp".to_string())
        );
        assert_eq!(
            resolved.env_vars.get("HOME"),
            Some(&"/home/alice".to_string())
        );
        assert_eq!(resolved.cwd, "/home/alice/projects");
    }

    #[test]
    fn test_document_vars_new_and_insert() {
        let mut vars = DocumentVars::new();
        assert!(vars.is_empty());

        vars.insert("VAR1".to_string(), "value1".to_string(), "test".to_string());
        vars.insert("VAR2".to_string(), "value2".to_string(), "test".to_string());

        assert_eq!(vars.len(), 2);
        assert!(!vars.is_empty());
    }

    #[test]
    fn test_document_vars_push() {
        let mut vars = DocumentVars::new();
        vars.push(DocumentVar::new(
            "VAR1".to_string(),
            "value1".to_string(),
            "test".to_string(),
        ));

        assert_eq!(vars.len(), 1);
    }

    #[test]
    fn test_document_vars_iter() {
        let mut vars = DocumentVars::new();
        vars.insert("VAR1".to_string(), "value1".to_string(), "test".to_string());
        vars.insert("VAR2".to_string(), "value2".to_string(), "test".to_string());

        let names: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, vec!["VAR1", "VAR2"]);
    }

    #[test]
    fn test_document_vars_from_iterator() {
        let items = vec![
            DocumentVar::new("VAR1".to_string(), "value1".to_string(), "test".to_string()),
            DocumentVar::new("VAR2".to_string(), "value2".to_string(), "test".to_string()),
        ];

        let vars: DocumentVars = items.into_iter().collect();
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn test_document_vars_into_iterator() {
        let mut vars = DocumentVars::new();
        vars.insert("VAR1".to_string(), "value1".to_string(), "test".to_string());
        vars.insert("VAR2".to_string(), "value2".to_string(), "test".to_string());

        let collected: Vec<DocumentVar> = vars.into_iter().collect();
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn test_document_vars_in_block_context() {
        let mut vars = DocumentVars::new();
        vars.insert("VAR1".to_string(), "value1".to_string(), "test".to_string());
        vars.insert("VAR2".to_string(), "value2".to_string(), "test".to_string());

        let mut context = BlockContext::new();
        context.insert(vars.clone());

        let retrieved = context.get::<DocumentVars>();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_context_resolver_with_document_vars() {
        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("SINGLE")
            .value("single_value")
            .build();

        let mut vars = DocumentVars::new();
        vars.insert("VAR1".to_string(), "value1".to_string(), "test".to_string());
        vars.insert("VAR2".to_string(), "value2".to_string(), "test".to_string());

        let mut context = BlockContext::new();
        context.insert(vars);

        let block_with_context =
            DocumentBlock::new(Block::Var(var_block), context, None, None, None);

        let resolver = ContextResolver::from_blocks(&[block_with_context]);

        assert_eq!(resolver.get_var("VAR1"), Some(&"value1".to_string()));
        assert_eq!(resolver.get_var("VAR2"), Some(&"value2".to_string()));
    }

    #[tokio::test]
    async fn test_context_resolver_document_vars_with_templates() {
        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("BASE")
            .value("hello")
            .build();

        // First block sets BASE variable
        let mut context1 = BlockContext::new();
        context1.insert(DocumentVar::new(
            "BASE".to_string(),
            "hello".to_string(),
            "test".to_string(),
        ));

        // Second block uses DocumentVars with template referencing BASE
        let mut vars = DocumentVars::new();
        vars.insert(
            "GREETING".to_string(),
            "{{ var.BASE }} world".to_string(),
            "test".to_string(),
        );
        vars.insert(
            "OTHER".to_string(),
            "static value".to_string(),
            "test".to_string(),
        );

        let mut context2 = BlockContext::new();
        context2.insert(vars);

        let var_block2 = Var::builder()
            .id(Uuid::new_v4())
            .name("UNUSED")
            .value("unused")
            .build();

        let blocks = vec![
            DocumentBlock::new(Block::Var(var_block), context1, None, None, None),
            DocumentBlock::new(Block::Var(var_block2), context2, None, None, None),
        ];

        let resolver = ContextResolver::from_blocks(&blocks);

        assert_eq!(resolver.get_var("BASE"), Some(&"hello".to_string()));
        assert_eq!(
            resolver.get_var("GREETING"),
            Some(&"hello world".to_string())
        );
        assert_eq!(resolver.get_var("OTHER"), Some(&"static value".to_string()));
    }

    #[tokio::test]
    async fn test_context_resolver_document_vars_in_active_context() {
        let var_block = Var::builder()
            .id(Uuid::new_v4())
            .name("UNUSED")
            .value("unused")
            .build();

        let mut vars = DocumentVars::new();
        vars.insert(
            "ACTIVE_VAR1".to_string(),
            "active1".to_string(),
            "test".to_string(),
        );
        vars.insert(
            "ACTIVE_VAR2".to_string(),
            "active2".to_string(),
            "test".to_string(),
        );

        let mut active_context = BlockContext::new();
        active_context.insert(vars);

        let block_with_context = DocumentBlock::new(
            Block::Var(var_block),
            BlockContext::new(),
            Some(active_context),
            None,
            None,
        );

        let resolver = ContextResolver::from_blocks(&[block_with_context]);

        assert_eq!(
            resolver.get_var("ACTIVE_VAR1"),
            Some(&"active1".to_string())
        );
        assert_eq!(
            resolver.get_var("ACTIVE_VAR2"),
            Some(&"active2".to_string())
        );
    }

    #[test]
    fn test_document_vars_serialization_roundtrip() {
        let mut vars = DocumentVars::new();
        vars.insert(
            "VAR1".to_string(),
            "value1".to_string(),
            "source1".to_string(),
        );
        vars.insert(
            "VAR2".to_string(),
            "value2".to_string(),
            "source2".to_string(),
        );

        let mut context = BlockContext::new();
        context.insert(vars);

        let serialized = serde_json::to_string(&context).unwrap();
        let deserialized: BlockContext = serde_json::from_str(&serialized).unwrap();

        let retrieved = deserialized.get::<DocumentVars>();
        assert!(retrieved.is_some());
        let retrieved_vars = retrieved.unwrap();
        assert_eq!(retrieved_vars.len(), 2);
    }
}
