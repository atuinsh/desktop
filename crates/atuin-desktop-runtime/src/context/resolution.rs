use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use minijinja::{value::Object, Environment, Value};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    blocks::BlockBehavior,
    client::LocalValueProvider,
    context::{
        DocumentBlock, DocumentCwd, DocumentEnvVar, DocumentEnvVars, DocumentSshConfig,
        DocumentSshHost, DocumentVar, DocumentVars,
    },
};

/// A struct representing the resolved context of a block.
/// Since it's built from a `ContextResolver`, it's a snapshot
/// of the final context based on the blocks above it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default, PartialEq)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedContext {
    pub variables: HashMap<String, String>,
    pub variables_sources: HashMap<String, String>,
    pub cwd: String,
    pub env_vars: HashMap<String, String>,
    pub ssh_host: Option<String>,
}

impl ResolvedContext {
    pub fn from_resolver(resolver: &ContextResolver) -> Self {
        Self {
            variables: resolver.vars().clone(),
            variables_sources: resolver.vars_sources().clone(),
            cwd: resolver.cwd().to_string(),
            env_vars: resolver.env_vars().clone(),
            ssh_host: resolver.ssh_host().cloned(),
        }
    }

    pub async fn from_block(
        block: &(impl BlockBehavior + Clone),
        block_local_value_provider: Option<&dyn LocalValueProvider>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(context) = block
            .passive_context(&ContextResolver::new(), block_local_value_provider)
            .await?
        {
            let block_with_context =
                DocumentBlock::new(block.clone().into_block(), context, None, None, None);
            let resolver = ContextResolver::from_blocks(&[block_with_context]);
            Ok(Self::from_resolver(&resolver))
        } else {
            Ok(Self::default())
        }
    }
}

/// A context resolver is used to resolve templates and build a [`ResolvedContext`] from blocks.
#[derive(Clone, Debug)]
pub struct ContextResolver {
    vars: HashMap<String, DocumentVar>,
    cwd: String,
    env_vars: HashMap<String, String>,
    ssh_host: Option<String>,
    /// Full SSH configuration from SSH Connect block (includes identity key, overrides, etc.)
    ssh_config: Option<DocumentSshConfig>,
    extra_template_context: HashMap<String, Value>,
}

impl ContextResolver {
    /// Create an empty context resolver
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
            cwd: default_cwd(),
            env_vars: HashMap::new(),
            ssh_host: None,
            ssh_config: None,
            extra_template_context: HashMap::new(),
        }
    }

    pub fn add_extra_template_context(
        &mut self,
        namespace: String,
        context: impl Object + 'static,
    ) {
        self.extra_template_context
            .insert(namespace, Value::from_object(context));
    }

    /// Build a resolver from blocks (typically all blocks above the current one)
    pub fn from_blocks(blocks: &[DocumentBlock]) -> Self {
        // Process blocks in order (earlier blocks can be overridden by later ones)
        let mut resolver = Self::new();
        for block in blocks {
            resolver.push_block(block);
        }

        resolver
    }

    /// Build a resolver from blocks with a parent context
    /// The parent context provides initial vars, env_vars, cwd, ssh_host
    /// which are then extended/overridden by the blocks
    pub fn from_blocks_with_parent(blocks: &[DocumentBlock], parent: &ContextResolver) -> Self {
        let mut resolver = Self::from_parent(parent);
        for block in blocks {
            resolver.push_block(block);
        }
        resolver
    }

    /// Push multiple blocks to the resolver
    pub fn push_blocks(&mut self, blocks: &[DocumentBlock]) {
        for block in blocks {
            self.push_block(block);
        }
    }

    /// Test-only constructor to create a resolver with specific vars
    #[cfg(test)]
    pub fn with_vars(vars: HashMap<String, String>) -> Self {
        Self {
            vars: vars
                .into_iter()
                .map(|(k, v)| (k.clone(), DocumentVar::new(k, v.clone(), v)))
                .collect(),
            cwd: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            env_vars: HashMap::new(),
            ssh_host: None,
            ssh_config: None,
            extra_template_context: HashMap::new(),
        }
    }

    /// Update the resolver with the context of a block.
    /// Values are overwritten or merged as appropriate.
    pub fn push_block(&mut self, block: &DocumentBlock) {
        let passive_context = block.passive_context();
        let active_context = block.active_context();

        for ctx in [passive_context, active_context] {
            // Process variables first as they can be used in templates
            if let Some(var) = ctx.get::<DocumentVar>() {
                if let Ok(resolved_value) = self.resolve_template(&var.value) {
                    self.vars.insert(
                        var.name.clone(),
                        DocumentVar::new(var.name.clone(), resolved_value, var.source.clone()),
                    );
                } else {
                    tracing::warn!("Failed to resolve template for variable {}", var.name);
                }
            }

            // Process multiple variables from DocumentVars container
            if let Some(vars) = ctx.get::<DocumentVars>() {
                for var in vars.iter() {
                    if let Ok(resolved_value) = self.resolve_template(&var.value) {
                        self.vars.insert(
                            var.name.clone(),
                            DocumentVar::new(var.name.clone(), resolved_value, var.source.clone()),
                        );
                    } else {
                        tracing::warn!("Failed to resolve template for variable {}", var.name);
                    }
                }
            }

            // Process environment variables before cwd, as cwd may reference them
            if let Some(env) = ctx.get::<DocumentEnvVar>() {
                if let Ok(resolved_value) = self.resolve_template(&env.1) {
                    self.env_vars.insert(env.0.clone(), resolved_value);
                } else {
                    tracing::warn!(
                        "Failed to resolve template for environment variable {}",
                        env.0
                    );
                }
            }

            // Process multiple environment variables (from sub-runbook imports)
            if let Some(envs) = ctx.get::<DocumentEnvVars>() {
                tracing::debug!(
                    "Processing DocumentEnvVars with {} entries",
                    envs.iter().count()
                );
                for env in envs.iter() {
                    tracing::debug!("Adding env var from DocumentEnvVars: {}={}", env.0, env.1);
                    if let Ok(resolved_value) = self.resolve_template(&env.1) {
                        self.env_vars.insert(env.0.clone(), resolved_value);
                    } else {
                        tracing::warn!(
                            "Failed to resolve template for environment variable {}",
                            env.0
                        );
                    }
                }
            }

            // Process cwd after env vars so it can expand ${VAR} references
            if let Some(dir) = ctx.get::<DocumentCwd>() {
                if let Ok(resolved_value) = self.resolve_template(&dir.0) {
                    let trimmed_value = resolved_value.trim();

                    if trimmed_value.is_empty() {
                        self.cwd = default_cwd();
                        continue;
                    }

                    let expanded_value = expand_path_variables(trimmed_value, &self.env_vars);
                    let path = PathBuf::from(&expanded_value);

                    let normalized_path = if path.is_absolute() {
                        normalize_path(&path)
                    } else {
                        normalize_path(&PathBuf::from(self.cwd.clone()).join(&path))
                    };

                    self.cwd = normalized_path.to_string_lossy().to_string();
                } else {
                    tracing::warn!("Failed to resolve template for directory {}", dir.0);
                }
            }

            if let Some(host) = ctx.get::<DocumentSshHost>() {
                if let Some(host) = host.0.as_ref() {
                    if let Ok(resolved_value) = self.resolve_template(host) {
                        self.ssh_host = Some(resolved_value);
                    } else {
                        tracing::warn!("Failed to resolve template for SSH host {}", host);
                    }
                } else {
                    self.ssh_host = None;
                }
            }

            // Process full SSH configuration (includes identity key, overrides, etc.)
            if let Some(config) = ctx.get::<DocumentSshConfig>() {
                self.ssh_config = Some(config.clone());
            }
        }
    }

    /// Resolve a template string using minijinja
    pub fn resolve_template(&self, template: &str) -> Result<String, minijinja::Error> {
        // If the string doesn't contain template markers, return it as-is
        if !template.contains("{{") && !template.contains("{%") {
            return Ok(template.to_string());
        }

        // Create a minijinja environment
        let mut env = Environment::new();
        env.set_trim_blocks(true);

        // Add custom filter for shell escaping
        env.add_filter("shellquote", |value: String| -> String {
            // Use POSIX shell single-quote escaping:
            // wrap in single quotes and escape any single quotes as '\''
            format!("'{}'", value.replace('\'', "'\\''"))
        });

        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);

        // Build the context object for template rendering
        let mut context: HashMap<&str, Value> = HashMap::new();

        // Add any extra template context
        context.extend(
            self.extra_template_context
                .iter()
                .map(|(k, v)| (k.as_str(), v.clone())),
        );

        context.insert(
            "var",
            Value::from_object(
                self.vars
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::from(v.value.clone())))
                    .collect::<HashMap<String, Value>>(),
            ),
        );
        context.insert("env", Value::from_object(self.env_vars.clone()));

        // Render the template
        env.render_str(template, context)
    }

    /// Get a variable value
    pub fn get_var(&self, name: &str) -> Option<&String> {
        self.vars.get(name).map(|v| &v.value)
    }

    /// Get all variables
    pub fn vars(&self) -> HashMap<String, String> {
        self.vars
            .iter()
            .map(|(k, v)| (k.clone(), v.value.clone()))
            .collect()
    }

    /// Get all variables with sources
    pub fn vars_sources(&self) -> HashMap<String, String> {
        self.vars
            .iter()
            .map(|(k, v)| (k.clone(), v.source.clone()))
            .collect()
    }

    /// Get current working directory
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Get environment variables
    pub fn env_vars(&self) -> &HashMap<String, String> {
        &self.env_vars
    }

    /// Get SSH host
    pub fn ssh_host(&self) -> Option<&String> {
        self.ssh_host.as_ref()
    }

    /// Get full SSH configuration (includes identity key, overrides, etc.)
    pub fn ssh_config(&self) -> Option<&DocumentSshConfig> {
        self.ssh_config.as_ref()
    }
}

fn default_cwd() -> String {
    // Check for PWD env var first (set by shell, reflects current directory)
    // This allows CLI tools to inherit the user's current working directory
    // Falls back to home directory for desktop app / GUI contexts
    if let Ok(pwd) = std::env::var("PWD") {
        if !pwd.is_empty() {
            return pwd;
        }
    }

    dirs::home_dir()
        .or(std::env::current_dir().ok())
        .unwrap_or("/".into())
        .to_string_lossy()
        .to_string()
}

/// Expand tilde (~) and shell-style environment variables ($VAR, ${VAR}) in a path
fn expand_path_variables(path: &str, env_vars: &HashMap<String, String>) -> String {
    // Use shellexpand with a custom context that checks our env_vars map first
    let result: Result<std::borrow::Cow<str>, shellexpand::LookupError<std::env::VarError>> =
        shellexpand::full_with_context(
            path,
            || dirs::home_dir().map(|p| std::borrow::Cow::Owned(p.to_string_lossy().to_string())),
            |var_name| {
                Ok(env_vars
                    .get(var_name)
                    .map(|s| std::borrow::Cow::Borrowed(s.as_str())))
            },
        );

    result
        .unwrap_or(std::borrow::Cow::Borrowed(path))
        .to_string()
}

/// Normalize a path by resolving . and .. components without requiring the path to exist
fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {
                // Skip current directory references
            }
            _ => {
                normalized.push(component);
            }
        }
    }

    normalized
}

impl Default for ContextResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextResolver {
    /// Create a context resolver pre-populated with values from another resolver
    ///
    /// This is used for sub-runbook execution where the sub-runbook inherits
    /// the parent's context but maintains isolation (changes don't propagate back)
    pub fn from_parent(parent: &ContextResolver) -> Self {
        Self {
            vars: parent.vars.clone(),
            cwd: parent.cwd.clone(),
            env_vars: parent.env_vars.clone(),
            ssh_host: parent.ssh_host.clone(),
            ssh_config: parent.ssh_config.clone(),
            extra_template_context: parent.extra_template_context.clone(),
        }
    }
}

#[cfg(test)]
pub struct ContextResolverBuilder {
    vars: Option<HashMap<String, DocumentVar>>,
    cwd: Option<String>,
    env_vars: Option<HashMap<String, String>>,
    ssh_host: Option<String>,
    extra_template_context: Option<HashMap<String, Value>>,
}

#[cfg(test)]
#[allow(unused)]
impl ContextResolverBuilder {
    pub fn new() -> Self {
        Self {
            vars: None,
            cwd: None,
            env_vars: None,
            ssh_host: None,
            extra_template_context: None,
        }
    }

    pub fn vars(mut self, vars: HashMap<String, String>) -> Self {
        self.vars = Some(
            vars.into_iter()
                .map(|(k, v)| (k.clone(), DocumentVar::new(k, v.clone(), v)))
                .collect(),
        );
        self
    }

    pub fn vars_with_source(mut self, vars: HashMap<String, (String, String)>) -> Self {
        self.vars = Some(
            vars.into_iter()
                .map(|(k, v)| (k.clone(), DocumentVar::new(k, v.0, v.1)))
                .collect(),
        );
        self
    }

    pub fn cwd(mut self, cwd: String) -> Self {
        self.cwd = Some(cwd);
        self
    }

    pub fn env_vars(mut self, env_vars: HashMap<String, String>) -> Self {
        self.env_vars = Some(env_vars);
        self
    }

    pub fn ssh_host(mut self, ssh_host: String) -> Self {
        self.ssh_host = Some(ssh_host);
        self
    }

    pub fn extra_template_context(
        mut self,
        extra_template_context: HashMap<String, Value>,
    ) -> Self {
        self.extra_template_context = Some(extra_template_context);
        self
    }

    pub fn build(self) -> ContextResolver {
        ContextResolver {
            vars: self
                .vars
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k.clone(), DocumentVar::new(k, v.value, v.source)))
                .collect(),
            cwd: self.cwd.unwrap_or_default(),
            env_vars: self.env_vars.unwrap_or_default(),
            ssh_host: self.ssh_host,
            ssh_config: None,
            extra_template_context: self.extra_template_context.unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::host::Host;
    use crate::blocks::Block;
    use crate::context::BlockContext;

    fn create_block_with_context(
        passive_context: BlockContext,
        active_context: Option<BlockContext>,
    ) -> DocumentBlock {
        let host = Host::builder()
            .id(uuid::Uuid::new_v4())
            .host("localhost")
            .build();

        DocumentBlock::new(
            Block::Host(host),
            passive_context,
            active_context,
            None,
            None,
        )
    }

    #[test]
    fn test_cwd_absolute_path() {
        let mut resolver = ContextResolver::new();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("/absolute/path".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), "/absolute/path");
    }

    #[test]
    fn test_cwd_relative_path() {
        let mut resolver = ContextResolverBuilder::new()
            .cwd("/base/path".to_string())
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("subdir".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), "/base/path/subdir");
    }

    #[test]
    fn test_cwd_relative_path_with_parent_directory() {
        let mut resolver = ContextResolverBuilder::new()
            .cwd("/base/path/deep".to_string())
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("../sibling".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), "/base/path/sibling");
    }

    #[test]
    fn test_cwd_with_template_variable() {
        let mut resolver = ContextResolverBuilder::new()
            .vars(HashMap::from([(
                "home".to_string(),
                "/home/user".to_string(),
            )]))
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("{{ var.home }}/projects".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), "/home/user/projects");
    }

    #[test]
    fn test_cwd_with_environment_variable() {
        let mut resolver = ContextResolverBuilder::new()
            .env_vars(HashMap::from([(
                "HOME".to_string(),
                "/home/user".to_string(),
            )]))
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("{{ env.HOME }}/documents".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), "/home/user/documents");
    }

    #[test]
    fn test_cwd_with_literal_dollar_sign_no_template() {
        let mut resolver = ContextResolver::new();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("/path/with/$dollar".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), "/path/with/$dollar");
    }

    #[test]
    fn test_cwd_empty_resets_to_default() {
        let mut resolver = ContextResolverBuilder::new()
            .cwd("/some/path".to_string())
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), default_cwd());
    }

    #[test]
    fn test_cwd_chaining_multiple_changes() {
        let mut resolver = ContextResolver::new();

        let mut context1 = BlockContext::new();
        context1.insert(DocumentCwd("/base".to_string()));
        let block1 = create_block_with_context(context1, None);
        resolver.push_block(&block1);
        assert_eq!(resolver.cwd(), "/base");

        let mut context2 = BlockContext::new();
        context2.insert(DocumentCwd("subdir1".to_string()));
        let block2 = create_block_with_context(context2, None);
        resolver.push_block(&block2);
        assert_eq!(resolver.cwd(), "/base/subdir1");

        let mut context3 = BlockContext::new();
        context3.insert(DocumentCwd("subdir2".to_string()));
        let block3 = create_block_with_context(context3, None);
        resolver.push_block(&block3);
        assert_eq!(resolver.cwd(), "/base/subdir1/subdir2");
    }

    #[test]
    fn test_cwd_with_combined_templates() {
        let mut resolver = ContextResolverBuilder::new()
            .vars(HashMap::from([
                ("user".to_string(), "alice".to_string()),
                ("project".to_string(), "myproject".to_string()),
            ]))
            .env_vars(HashMap::from([("BASE".to_string(), "/home".to_string())]))
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd(
            "{{ env.BASE }}/{{ var.user }}/{{ var.project }}".to_string(),
        ));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), "/home/alice/myproject");
    }

    #[test]
    fn test_variables_with_templates() {
        let mut resolver = ContextResolverBuilder::new()
            .vars(HashMap::from([("base".to_string(), "hello".to_string())]))
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentVar::new(
            "greeting".to_string(),
            "{{ var.base }} world".to_string(),
            "test".to_string(),
        ));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.get_var("greeting").unwrap(), "hello world");
    }

    #[test]
    fn test_env_vars_with_templates() {
        let mut resolver = ContextResolverBuilder::new()
            .vars(HashMap::from([("port".to_string(), "8080".to_string())]))
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentEnvVar(
            "SERVER_URL".to_string(),
            "http://localhost:{{ var.port }}".to_string(),
        ));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(
            resolver.env_vars().get("SERVER_URL").unwrap(),
            "http://localhost:8080"
        );
    }

    #[test]
    fn test_ssh_host_with_template() {
        let mut resolver = ContextResolverBuilder::new()
            .vars(HashMap::from([(
                "domain".to_string(),
                "example.com".to_string(),
            )]))
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentSshHost(Some("server.{{ var.domain }}".to_string())));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.ssh_host().unwrap(), "server.example.com");
    }

    #[test]
    fn test_from_blocks_processes_in_order() {
        let mut context1 = BlockContext::new();
        context1.insert(DocumentCwd("/base".to_string()));

        let mut context2 = BlockContext::new();
        context2.insert(DocumentVar::new(
            "key".to_string(),
            "value1".to_string(),
            "block1".to_string(),
        ));

        let mut context3 = BlockContext::new();
        context3.insert(DocumentVar::new(
            "key".to_string(),
            "value2".to_string(),
            "block2".to_string(),
        ));

        let blocks = vec![
            create_block_with_context(context1, None),
            create_block_with_context(context2, None),
            create_block_with_context(context3, None),
        ];

        let resolver = ContextResolver::from_blocks(&blocks);
        assert_eq!(resolver.cwd(), "/base");
        assert_eq!(resolver.get_var("key").unwrap(), "value2");
    }

    #[test]
    fn test_cwd_with_tilde_home_dir() {
        let mut resolver = ContextResolver::new();
        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("~/Documents".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(
            resolver.cwd(),
            dirs::home_dir()
                .unwrap()
                .join("Documents")
                .to_string_lossy()
                .to_string()
        );
    }

    #[test]
    fn test_cwd_with_actual_env_var() {
        let mut resolver = ContextResolver::new();
        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentEnvVar(
            "HOME".to_string(),
            dirs::home_dir().unwrap().to_string_lossy().to_string(),
        ));
        passive_context.insert(DocumentCwd("${HOME}/Documents".to_string()));
        let block = create_block_with_context(passive_context.clone(), None);

        resolver.push_block(&block);
        assert_eq!(
            resolver.cwd(),
            dirs::home_dir()
                .unwrap()
                .join("Documents")
                .to_string_lossy()
                .to_string()
        );

        passive_context.insert(DocumentCwd("$HOME/Documents".to_string()));
        let block = create_block_with_context(passive_context, None);
        resolver.push_block(&block);
        assert_eq!(
            resolver.cwd(),
            dirs::home_dir()
                .unwrap()
                .join("Documents")
                .to_string_lossy()
                .to_string()
        );
    }

    #[test]
    fn test_cwd_with_tilde_in_literal() {
        let mut resolver = ContextResolver::new();
        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("/path/with/~tilde".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), "/path/with/~tilde");
    }

    #[test]
    fn test_resolve_template_without_markers() {
        let resolver = ContextResolver::new();
        let result = resolver.resolve_template("plain text").unwrap();
        assert_eq!(result, "plain text");
    }

    #[test]
    fn test_resolve_template_with_undefined_var() {
        let resolver = ContextResolver::new();
        let result = resolver.resolve_template("{{ var.undefined }}");
        assert!(result.is_err());
    }

    #[test]
    fn test_active_context_overrides_passive() {
        let mut resolver = ContextResolver::new();
        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentVar::new(
            "key".to_string(),
            "passive".to_string(),
            "source1".to_string(),
        ));
        let mut active_context = BlockContext::new();
        active_context.insert(DocumentVar::new(
            "key".to_string(),
            "active".to_string(),
            "source2".to_string(),
        ));
        let block = create_block_with_context(passive_context, Some(active_context));

        resolver.push_block(&block);
        assert_eq!(resolver.get_var("key").unwrap(), "active");
    }

    #[test]
    fn test_resolved_context_from_resolver() {
        let resolver = ContextResolverBuilder::new()
            .vars(HashMap::from([("key1".to_string(), "value1".to_string())]))
            .cwd("/test/path".to_string())
            .env_vars(HashMap::from([("VAR".to_string(), "val".to_string())]))
            .ssh_host("host.example.com".to_string())
            .build();

        let resolved = ResolvedContext::from_resolver(&resolver);
        assert_eq!(resolved.variables.get("key1").unwrap(), "value1");
        assert_eq!(resolved.cwd, "/test/path");
        assert_eq!(resolved.env_vars.get("VAR").unwrap(), "val");
        assert_eq!(resolved.ssh_host.as_ref().unwrap(), "host.example.com");
    }

    #[test]
    fn test_cwd_with_special_characters() {
        let mut resolver = ContextResolver::new();
        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("/path/with spaces/and-dashes".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), "/path/with spaces/and-dashes");
    }

    #[test]
    fn test_cwd_relative_with_dot_slash() {
        let mut resolver = ContextResolverBuilder::new()
            .cwd("/base".to_string())
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("./subdir".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        // Path normalization removes ./ which is correct behavior
        assert_eq!(resolver.cwd(), "/base/subdir");
    }

    #[test]
    fn test_cwd_multiple_parent_dirs() {
        let mut resolver = ContextResolverBuilder::new()
            .cwd("/a/b/c/d/e".to_string())
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("../../other".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), "/a/b/c/other");
    }

    #[test]
    fn test_cwd_parent_beyond_root() {
        let mut resolver = ContextResolverBuilder::new()
            .cwd("/home".to_string())
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("../../../../../../etc".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        // Should normalize to /etc (can't go above root)
        assert_eq!(resolver.cwd(), "/etc");
    }

    #[test]
    fn test_cwd_with_trailing_slash() {
        let mut resolver = ContextResolver::new();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("/path/to/dir/".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        // Trailing slash should be preserved or handled correctly
        assert!(
            resolver.cwd() == "/path/to/dir/" || resolver.cwd() == "/path/to/dir",
            "cwd was: {}",
            resolver.cwd()
        );
    }

    #[test]
    fn test_cwd_with_whitespace() {
        let mut resolver = ContextResolver::new();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("  /path/to/dir  ".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        // Whitespace should be trimmed
        assert_eq!(resolver.cwd(), "/path/to/dir");
    }

    #[test]
    fn test_cwd_complex_relative_paths() {
        let mut resolver = ContextResolverBuilder::new()
            .cwd("/base/current".to_string())
            .build();

        let mut context1 = BlockContext::new();
        context1.insert(DocumentCwd("../other".to_string()));
        let block1 = create_block_with_context(context1, None);
        resolver.push_block(&block1);
        assert_eq!(resolver.cwd(), "/base/other");

        let mut context2 = BlockContext::new();
        context2.insert(DocumentCwd("./subfolder/deep".to_string()));
        let block2 = create_block_with_context(context2, None);
        resolver.push_block(&block2);
        assert_eq!(resolver.cwd(), "/base/other/subfolder/deep");

        let mut context3 = BlockContext::new();
        context3.insert(DocumentCwd("../../sibling".to_string()));
        let block3 = create_block_with_context(context3, None);
        resolver.push_block(&block3);
        // From /base/other/subfolder/deep, ../../sibling goes to /base/other/sibling
        assert_eq!(resolver.cwd(), "/base/other/sibling");
    }

    #[test]
    fn test_cwd_reset_with_absolute_after_relative() {
        let mut resolver = ContextResolverBuilder::new()
            .cwd("/initial".to_string())
            .build();

        // First, navigate relatively
        let mut context1 = BlockContext::new();
        context1.insert(DocumentCwd("relative/path".to_string()));
        let block1 = create_block_with_context(context1, None);
        resolver.push_block(&block1);
        assert_eq!(resolver.cwd(), "/initial/relative/path");

        // Then reset with absolute path
        let mut context2 = BlockContext::new();
        context2.insert(DocumentCwd("/completely/new/path".to_string()));
        let block2 = create_block_with_context(context2, None);
        resolver.push_block(&block2);
        assert_eq!(resolver.cwd(), "/completely/new/path");
    }

    #[test]
    fn test_cwd_with_dots_in_directory_names() {
        let mut resolver = ContextResolver::new();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd(
            "/path/to/.hidden/dir.name.with.dots".to_string(),
        ));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), "/path/to/.hidden/dir.name.with.dots");
    }

    #[test]
    fn test_cwd_template_resolves_before_path_expansion() {
        let mut resolver = ContextResolverBuilder::new()
            .vars(HashMap::from([
                ("base".to_string(), "/home/user".to_string()),
                ("subdir".to_string(), "documents".to_string()),
            ]))
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("{{ var.base }}/{{ var.subdir }}".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), "/home/user/documents");
    }

    #[test]
    fn test_cwd_env_var_in_relative_path() {
        let mut resolver = ContextResolverBuilder::new()
            .cwd("/base".to_string())
            .env_vars(HashMap::from([(
                "SUBDIR".to_string(),
                "myproject".to_string(),
            )]))
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentCwd("$SUBDIR/src".to_string()));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.cwd(), "/base/myproject/src");
    }

    #[test]
    fn test_ssh_host_with_none_resets_ssh() {
        let mut resolver = ContextResolverBuilder::new()
            .ssh_host("host.example.com".to_string())
            .build();

        let mut passive_context = BlockContext::new();
        passive_context.insert(DocumentSshHost(None));
        let block = create_block_with_context(passive_context, None);

        resolver.push_block(&block);
        assert_eq!(resolver.ssh_host(), None);
    }
}
