use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use ts_rs::TS;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::{
    blocks::{Block, BlockBehavior, FromDocument},
    client::LocalValueProvider,
    context::{BlockContext, BlockState, ContextResolver, DocumentVar},
    execution::{ExecutionContext, ExecutionHandle},
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum DropdownOptionType {
    Fixed,
    Variable,
    Command,
}

impl TryFrom<&str> for DropdownOptionType {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "fixed" => Ok(DropdownOptionType::Fixed),
            "variable" => Ok(DropdownOptionType::Variable),
            "command" => Ok(DropdownOptionType::Command),
            _ => Err(format!("Invalid dropdown option type: {value}")),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder, TS)]
#[ts(export)]
pub struct DropdownOption {
    pub label: String,
    pub value: String,
}

impl DropdownOption {
    pub fn vec_from_str(value: &str) -> Result<Vec<Self>, String> {
        value
            // Split on ",", ", ", or newlines using regex
            .split([',', '\n'])
            .flat_map(|part| part.split(", ")) // extra split for ", " if not caught by the char ','
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .map(|part| part.try_into())
            .collect()
    }
}

impl TryFrom<&str> for DropdownOption {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let colon_index = value.find(':');
        if let Some(colon_index) = colon_index {
            let label = value[..colon_index].to_string();
            let value = value[colon_index + 1..].to_string();
            Ok(DropdownOption::builder().label(label).value(value).build())
        } else {
            Ok(DropdownOption::builder()
                .label(value.to_string())
                .value(value.to_string())
                .build())
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS)]
#[ts(export)]
struct DropdownState {
    resolved: Option<ResolvedDropdownState>,
}

impl BlockState for DropdownState {}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS)]
#[ts(export)]
struct ResolvedDropdownState {
    options: Vec<DropdownOption>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Dropdown {
    pub id: Uuid,
    pub name: String,
    pub options_type: DropdownOptionType,
    pub fixed_options: String,
    pub variable_options: String,
    pub command_options: String,
    pub interpreter: String,
    pub value: String,
}

impl Dropdown {
    async fn resolve_options(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DropdownOption>, Box<dyn std::error::Error + Send + Sync>> {
        let options = match self.options_type {
            DropdownOptionType::Fixed => {
                let options = DropdownOption::vec_from_str(&self.fixed_options)?;
                Ok(options)
            }
            DropdownOptionType::Variable => {
                // resolve variable, set options based on output
                let value = context
                    .context_resolver
                    .get_var(&self.variable_options)
                    .ok_or("Variable not found")?;
                let options = DropdownOption::vec_from_str(value)?;
                Ok(options)
            }
            DropdownOptionType::Command => {
                let command = context
                    .context_resolver
                    .resolve_template(&self.command_options)?;
                let output = Command::new(&self.interpreter)
                    .arg("-c")
                    .arg(&command)
                    .output()
                    .await?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let options = DropdownOption::vec_from_str(&stdout)?;
                Ok(options)
            }
        };

        options
    }
}

impl FromDocument for Dropdown {
    fn from_document(block_data: &serde_json::Value) -> Result<Self, String> {
        let id = block_data
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or("Invalid or missing id")?;

        let props = block_data
            .get("props")
            .and_then(|p| p.as_object())
            .ok_or("Invalid or missing props")?;

        let name = props
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("Missing name")?
            .to_string();

        let options_type = props
            .get("optionsType")
            .and_then(|v| v.as_str())
            .ok_or("Missing options_type")?
            .try_into()
            .map_err(|e| format!("Invalid dropdown option type: {e}"))?;

        let fixed_options = props
            .get("fixedOptions")
            .and_then(|v| v.as_str())
            .ok_or("Missing fixed_options")?
            .to_string();

        let variable_options = props
            .get("variableOptions")
            .and_then(|v| v.as_str())
            .ok_or("Missing variable_options")?
            .to_string();

        let command_options = props
            .get("commandOptions")
            .and_then(|v| v.as_str())
            .ok_or("Missing command_options")?
            .to_string();

        let value = props
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or("Missing value")?
            .to_string();

        let interpreter = props
            .get("interpreter")
            .and_then(|v| v.as_str())
            .ok_or("Missing interpreter")?
            .to_string();

        Ok(Dropdown::builder()
            .id(id)
            .name(name)
            .options_type(options_type)
            .fixed_options(fixed_options)
            .variable_options(variable_options)
            .command_options(command_options)
            .value(value)
            .interpreter(interpreter)
            .build())
    }
}

#[async_trait]
impl BlockBehavior for Dropdown {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::Dropdown(self)
    }

    fn create_state(&self) -> Option<Box<dyn BlockState>> {
        Some(Box::new(DropdownState { resolved: None }))
    }

    async fn passive_context(
        &self,
        resolver: &ContextResolver,
        _block_local_value_provider: Option<&dyn LocalValueProvider>,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let name = resolver.resolve_template(&self.name)?;
        let value = resolver.resolve_template(&self.value)?;
        if name.is_empty() {
            return Ok(None);
        }

        let mut context = BlockContext::new();
        context.insert(DocumentVar::new(name, value, self.value.clone()));
        Ok(Some(context))
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        let resolved_options = self.resolve_options(&context).await?;
        context
            .update_block_state::<DropdownState, _>(self.id, |state| {
                state.resolved = Some(ResolvedDropdownState {
                    options: resolved_options,
                });
            })
            .await?;

        let _ = context.block_finished(None, true).await;

        Ok(Some(context.handle()))
    }
}
