use std::{collections::HashMap, path::PathBuf};

use minijinja::{value::Object, Environment, Value};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    blocks::BlockBehavior,
    client::LocalValueProvider,
    context::{BlockWithContext, DocumentCwd, DocumentEnvVar, DocumentSshHost, DocumentVar},
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
                BlockWithContext::new(block.clone().into_block(), context, None, None);
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
    pub fn from_blocks(blocks: &[BlockWithContext]) -> Self {
        // Process blocks in order (earlier blocks can be overridden by later ones)
        let mut resolver = Self::new();
        for block in blocks {
            resolver.push_block(block);
        }

        resolver
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
            extra_template_context: HashMap::new(),
        }
    }

    /// Update the resolver with the context of a block.
    /// Values are overwritten or merged as appropriate.
    pub fn push_block(&mut self, block: &BlockWithContext) {
        let passive_context = block.passive_context();
        let active_context = block.active_context();

        for ctx in [passive_context, active_context] {
            if let Some(var) = ctx.get::<DocumentVar>() {
                if let Ok(resolved_value) = self.resolve_template(&var.value) {
                    self.vars.insert(
                        var.name.clone(),
                        DocumentVar::new(var.name.clone(), resolved_value, var.source.clone()),
                    );
                } else {
                    log::warn!("Failed to resolve template for variable {}", var.name);
                }
            }

            if let Some(dir) = ctx.get::<DocumentCwd>() {
                if let Ok(resolved_value) = self.resolve_template(&dir.0) {
                    if resolved_value.is_empty() {
                        self.cwd = default_cwd();
                        continue;
                    }

                    let path = PathBuf::from(&resolved_value);
                    if path.is_absolute() {
                        self.cwd = path.to_string_lossy().to_string();
                    } else {
                        self.cwd = PathBuf::from(self.cwd.clone())
                            .join(&path)
                            .to_string_lossy()
                            .to_string();
                    }
                } else {
                    log::warn!("Failed to resolve template for directory {}", dir.0);
                }
            }

            if let Some(env) = ctx.get::<DocumentEnvVar>() {
                if let Ok(resolved_value) = self.resolve_template(&env.1) {
                    self.env_vars.insert(env.0.clone(), resolved_value);
                } else {
                    log::warn!(
                        "Failed to resolve template for environment variable {}",
                        env.0
                    );
                }
            }

            if let Some(host) = ctx.get::<DocumentSshHost>() {
                if let Some(host) = host.0.as_ref() {
                    if let Ok(resolved_value) = self.resolve_template(host) {
                        self.ssh_host = Some(resolved_value);
                    } else {
                        log::warn!("Failed to resolve template for SSH host {}", host);
                    }
                }
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
}

fn default_cwd() -> String {
    // Defaults to home directory because placeholder in directory blocks is `~`
    dirs::home_dir()
        .or(std::env::current_dir().ok())
        .unwrap_or("/".into())
        .to_string_lossy()
        .to_string()
}

impl Default for ContextResolver {
    fn default() -> Self {
        Self::new()
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
            extra_template_context: self.extra_template_context.unwrap_or_default(),
        }
    }
}
