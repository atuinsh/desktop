use async_trait::async_trait;
use regex::Regex;
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
    pub fn from_str_with_delimiter(value: &str, delimiter: &str) -> Result<Self, String> {
        if let Some(idx) = value.find(delimiter) {
            let label = value[..idx].to_string();
            let val = value[idx + delimiter.len()..].to_string();
            Ok(DropdownOption::builder().label(label).value(val).build())
        } else {
            Ok(DropdownOption::builder()
                .label(value.to_string())
                .value(value.to_string())
                .build())
        }
    }

    #[allow(dead_code)]
    pub fn vec_from_str(value: &str) -> Result<Vec<Self>, String> {
        Self::vec_from_str_with_delimiter(value, ":")
    }

    pub fn vec_from_str_with_delimiter(value: &str, delimiter: &str) -> Result<Vec<Self>, String> {
        if value.trim().is_empty() {
            return Ok(vec![]);
        }

        let re = Regex::new(r",\s*|\r?\n").unwrap();
        re.split(value)
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .map(|part| Self::from_str_with_delimiter(part, delimiter))
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
    pub options: String,
    pub interpreter: String,
    pub value: String,
    pub delimiter: String,
}

impl Dropdown {
    async fn resolve_options(
        &self,
        context: &ExecutionContext,
    ) -> Result<Vec<DropdownOption>, Box<dyn std::error::Error + Send + Sync>> {
        let all_three_options_blank = self.fixed_options.is_empty()
            && self.variable_options.is_empty()
            && self.command_options.is_empty();

        let options_source = if all_three_options_blank {
            &self.options
        } else {
            match self.options_type {
                DropdownOptionType::Fixed => &self.fixed_options,
                DropdownOptionType::Variable => &self.variable_options,
                DropdownOptionType::Command => &self.command_options,
            }
        };

        let delimiter = if self.delimiter.is_empty() {
            ":"
        } else {
            &self.delimiter
        };

        let options = match self.options_type {
            DropdownOptionType::Fixed => {
                let options =
                    DropdownOption::vec_from_str_with_delimiter(options_source, delimiter)?;
                Ok(options)
            }
            DropdownOptionType::Variable => {
                // resolve variable, set options based on output
                let value = context
                    .context_resolver
                    .get_var(options_source)
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                let options = DropdownOption::vec_from_str_with_delimiter(&value, delimiter)?;
                Ok(options)
            }
            DropdownOptionType::Command => {
                let command = context.context_resolver.resolve_template(options_source)?;

                let cwd = context.context_resolver.cwd().to_string();
                let envs = context.context_resolver.env_vars().clone();
                tracing::trace!("Running dropdown command in directory {cwd}");

                let output = Command::new(&self.interpreter)
                    .current_dir(cwd)
                    .envs(envs)
                    .arg("-c")
                    .arg(&command)
                    .output()
                    .await?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let options = DropdownOption::vec_from_str_with_delimiter(&stdout, delimiter)?;
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

        let options = props
            .get("options")
            .and_then(|v| v.as_str())
            .unwrap_or("") // Default to empty string if options is missing
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

        let delimiter = props
            .get("delimiter")
            .and_then(|v| v.as_str())
            .unwrap_or(":") // Default to ":" if delimiter is missing
            .to_string();

        Ok(Dropdown::builder()
            .id(id)
            .name(name)
            .options(options)
            .options_type(options_type)
            .fixed_options(fixed_options)
            .variable_options(variable_options)
            .command_options(command_options)
            .value(value)
            .interpreter(interpreter)
            .delimiter(delimiter)
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
        let _ = context.block_started().await;

        let resolved_options = self.resolve_options(&context).await?;
        tracing::trace!(
            "Resolved options for dropdown block {id}: {options:?}",
            id = self.id,
            options = resolved_options
        );
        context
            .update_block_state::<DropdownState, _>(self.id, move |state| {
                state.resolved = Some(ResolvedDropdownState {
                    options: resolved_options,
                });
            })
            .await?;

        let _ = context.block_finished(None, true).await;

        Ok(Some(context.handle()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests for DropdownOption parsing
    mod dropdown_option_parsing {
        use super::*;

        #[test]
        fn test_parse_simple_option() {
            let option: DropdownOption = "value".try_into().unwrap();
            assert_eq!(option.label, "value");
            assert_eq!(option.value, "value");
        }

        #[test]
        fn test_parse_label_value_option() {
            let option: DropdownOption = "Label:value".try_into().unwrap();
            assert_eq!(option.label, "Label");
            assert_eq!(option.value, "value");
        }

        #[test]
        fn test_parse_label_with_multiple_colons() {
            let option: DropdownOption = "Label:value:with:colons".try_into().unwrap();
            assert_eq!(option.label, "Label");
            assert_eq!(option.value, "value:with:colons");
        }

        #[test]
        fn test_vec_from_str_empty() {
            let options = DropdownOption::vec_from_str("").unwrap();
            assert!(options.is_empty());
        }

        #[test]
        fn test_vec_from_str_whitespace_only() {
            let options = DropdownOption::vec_from_str("   ").unwrap();
            assert!(options.is_empty());
        }

        #[test]
        fn test_vec_from_str_comma_separated() {
            let options = DropdownOption::vec_from_str("a, b, c").unwrap();
            assert_eq!(options.len(), 3);
            assert_eq!(options[0].value, "a");
            assert_eq!(options[1].value, "b");
            assert_eq!(options[2].value, "c");
        }

        #[test]
        fn test_vec_from_str_newline_separated() {
            let options = DropdownOption::vec_from_str("a\nb\nc").unwrap();
            assert_eq!(options.len(), 3);
            assert_eq!(options[0].value, "a");
            assert_eq!(options[1].value, "b");
            assert_eq!(options[2].value, "c");
        }

        #[test]
        fn test_vec_from_str_crlf_separated() {
            let options = DropdownOption::vec_from_str("a\r\nb\r\nc").unwrap();
            assert_eq!(options.len(), 3);
            assert_eq!(options[0].value, "a");
            assert_eq!(options[1].value, "b");
            assert_eq!(options[2].value, "c");
        }

        #[test]
        fn test_vec_from_str_with_labels() {
            let options = DropdownOption::vec_from_str("Label A:a, Label B:b").unwrap();
            assert_eq!(options.len(), 2);
            assert_eq!(options[0].label, "Label A");
            assert_eq!(options[0].value, "a");
            assert_eq!(options[1].label, "Label B");
            assert_eq!(options[1].value, "b");
        }

        #[test]
        fn test_vec_from_str_filters_empty_parts() {
            let options = DropdownOption::vec_from_str("a,,b,  ,c").unwrap();
            assert_eq!(options.len(), 3);
            assert_eq!(options[0].value, "a");
            assert_eq!(options[1].value, "b");
            assert_eq!(options[2].value, "c");
        }
    }

    // Tests for DropdownOptionType
    mod dropdown_option_type {
        use super::*;

        #[test]
        fn test_try_from_fixed() {
            let opt_type: DropdownOptionType = "fixed".try_into().unwrap();
            assert_eq!(opt_type, DropdownOptionType::Fixed);
        }

        #[test]
        fn test_try_from_variable() {
            let opt_type: DropdownOptionType = "variable".try_into().unwrap();
            assert_eq!(opt_type, DropdownOptionType::Variable);
        }

        #[test]
        fn test_try_from_command() {
            let opt_type: DropdownOptionType = "command".try_into().unwrap();
            assert_eq!(opt_type, DropdownOptionType::Command);
        }

        #[test]
        fn test_try_from_invalid() {
            let result: Result<DropdownOptionType, _> = "invalid".try_into();
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("Invalid dropdown option type"));
        }
    }

    // Tests for options source selection (the fallback behavior)
    mod options_source_selection {
        use super::*;

        fn create_dropdown(
            options: &str,
            options_type: DropdownOptionType,
            fixed_options: &str,
            variable_options: &str,
            command_options: &str,
        ) -> Dropdown {
            Dropdown::builder()
                .id(Uuid::new_v4())
                .name("test".to_string())
                .options(options.to_string())
                .options_type(options_type)
                .fixed_options(fixed_options.to_string())
                .variable_options(variable_options.to_string())
                .command_options(command_options.to_string())
                .value("".to_string())
                .interpreter("/bin/sh".to_string())
                .delimiter(":".to_string())
                .build()
        }

        #[test]
        fn test_all_three_blank_uses_options_field() {
            let dropdown = create_dropdown(
                "fallback1, fallback2", // options field
                DropdownOptionType::Fixed,
                "", // fixed_options blank
                "", // variable_options blank
                "", // command_options blank
            );

            // Check the logic: all three are blank
            let all_three_blank = dropdown.fixed_options.is_empty()
                && dropdown.variable_options.is_empty()
                && dropdown.command_options.is_empty();
            assert!(all_three_blank);

            // The options source should be the `options` field
            let options_source = if all_three_blank {
                &dropdown.options
            } else {
                match dropdown.options_type {
                    DropdownOptionType::Fixed => &dropdown.fixed_options,
                    DropdownOptionType::Variable => &dropdown.variable_options,
                    DropdownOptionType::Command => &dropdown.command_options,
                }
            };

            assert_eq!(options_source, "fallback1, fallback2");

            // Parse and verify
            let parsed = DropdownOption::vec_from_str(options_source).unwrap();
            assert_eq!(parsed.len(), 2);
            assert_eq!(parsed[0].value, "fallback1");
            assert_eq!(parsed[1].value, "fallback2");
        }

        #[test]
        fn test_fixed_options_not_blank_uses_fixed() {
            let dropdown = create_dropdown(
                "fallback1, fallback2",
                DropdownOptionType::Fixed,
                "fixed1, fixed2", // fixed_options has value
                "",
                "",
            );

            let all_three_blank = dropdown.fixed_options.is_empty()
                && dropdown.variable_options.is_empty()
                && dropdown.command_options.is_empty();
            assert!(!all_three_blank);

            let options_source = if all_three_blank {
                &dropdown.options
            } else {
                match dropdown.options_type {
                    DropdownOptionType::Fixed => &dropdown.fixed_options,
                    DropdownOptionType::Variable => &dropdown.variable_options,
                    DropdownOptionType::Command => &dropdown.command_options,
                }
            };

            assert_eq!(options_source, "fixed1, fixed2");
        }

        #[test]
        fn test_variable_options_not_blank_uses_variable() {
            let dropdown = create_dropdown(
                "fallback1, fallback2",
                DropdownOptionType::Variable,
                "",
                "myVariable", // variable_options has value
                "",
            );

            let all_three_blank = dropdown.fixed_options.is_empty()
                && dropdown.variable_options.is_empty()
                && dropdown.command_options.is_empty();
            assert!(!all_three_blank);

            let options_source = if all_three_blank {
                &dropdown.options
            } else {
                match dropdown.options_type {
                    DropdownOptionType::Fixed => &dropdown.fixed_options,
                    DropdownOptionType::Variable => &dropdown.variable_options,
                    DropdownOptionType::Command => &dropdown.command_options,
                }
            };

            assert_eq!(options_source, "myVariable");
        }

        #[test]
        fn test_command_options_not_blank_uses_command() {
            let dropdown = create_dropdown(
                "fallback1, fallback2",
                DropdownOptionType::Command,
                "",
                "",
                "echo 'a\nb\nc'", // command_options has value
            );

            let all_three_blank = dropdown.fixed_options.is_empty()
                && dropdown.variable_options.is_empty()
                && dropdown.command_options.is_empty();
            assert!(!all_three_blank);

            let options_source = if all_three_blank {
                &dropdown.options
            } else {
                match dropdown.options_type {
                    DropdownOptionType::Fixed => &dropdown.fixed_options,
                    DropdownOptionType::Variable => &dropdown.variable_options,
                    DropdownOptionType::Command => &dropdown.command_options,
                }
            };

            assert_eq!(options_source, "echo 'a\nb\nc'");
        }

        #[test]
        fn test_only_one_field_populated_still_uses_type_based_selection() {
            // Even if only fixed_options is populated, but options_type is Variable,
            // the logic should still check all_three_blank first
            let dropdown = create_dropdown(
                "fallback",
                DropdownOptionType::Variable,
                "fixed_value", // Only this is populated
                "",
                "",
            );

            let all_three_blank = dropdown.fixed_options.is_empty()
                && dropdown.variable_options.is_empty()
                && dropdown.command_options.is_empty();
            assert!(!all_three_blank);

            // Since not all three are blank, use type-based selection
            // options_type is Variable, so it returns variable_options (which is empty)
            let options_source = if all_three_blank {
                &dropdown.options
            } else {
                match dropdown.options_type {
                    DropdownOptionType::Fixed => &dropdown.fixed_options,
                    DropdownOptionType::Variable => &dropdown.variable_options,
                    DropdownOptionType::Command => &dropdown.command_options,
                }
            };

            assert_eq!(options_source, "");
        }
    }

    // Tests for FromDocument
    mod from_document {
        use super::*;

        #[test]
        fn test_from_document_valid() {
            let id = Uuid::new_v4();
            let json = serde_json::json!({
                "id": id.to_string(),
                "props": {
                    "name": "myDropdown",
                    "options": "opt1, opt2",
                    "optionsType": "fixed",
                    "fixedOptions": "fixed1, fixed2",
                    "variableOptions": "",
                    "commandOptions": "",
                    "value": "opt1",
                    "interpreter": "/bin/bash"
                }
            });

            let dropdown = Dropdown::from_document(&json).unwrap();
            assert_eq!(dropdown.id, id);
            assert_eq!(dropdown.name, "myDropdown");
            assert_eq!(dropdown.options, "opt1, opt2");
            assert_eq!(dropdown.options_type, DropdownOptionType::Fixed);
            assert_eq!(dropdown.fixed_options, "fixed1, fixed2");
            assert_eq!(dropdown.variable_options, "");
            assert_eq!(dropdown.command_options, "");
            assert_eq!(dropdown.value, "opt1");
            assert_eq!(dropdown.interpreter, "/bin/bash");
        }

        #[test]
        fn test_from_document_missing_options_defaults_empty() {
            let id = Uuid::new_v4();
            let json = serde_json::json!({
                "id": id.to_string(),
                "props": {
                    "name": "myDropdown",
                    "optionsType": "fixed",
                    "fixedOptions": "",
                    "variableOptions": "",
                    "commandOptions": "",
                    "value": "",
                    "interpreter": "/bin/sh"
                }
            });

            let dropdown = Dropdown::from_document(&json).unwrap();
            assert_eq!(dropdown.options, "");
        }

        #[test]
        fn test_from_document_missing_id() {
            let json = serde_json::json!({
                "props": {
                    "name": "myDropdown",
                    "optionsType": "fixed",
                    "fixedOptions": "",
                    "variableOptions": "",
                    "commandOptions": "",
                    "value": "",
                    "interpreter": "/bin/sh"
                }
            });

            let result = Dropdown::from_document(&json);
            assert!(result.is_err());
        }

        #[test]
        fn test_from_document_invalid_options_type() {
            let id = Uuid::new_v4();
            let json = serde_json::json!({
                "id": id.to_string(),
                "props": {
                    "name": "myDropdown",
                    "optionsType": "invalid",
                    "fixedOptions": "",
                    "variableOptions": "",
                    "commandOptions": "",
                    "value": "",
                    "interpreter": "/bin/sh"
                }
            });

            let result = Dropdown::from_document(&json);
            assert!(result.is_err());
        }

        #[test]
        fn test_from_document_missing_name() {
            let id = Uuid::new_v4();
            let json = serde_json::json!({
                "id": id.to_string(),
                "props": {
                    "optionsType": "fixed",
                    "fixedOptions": "",
                    "variableOptions": "",
                    "commandOptions": "",
                    "value": "",
                    "interpreter": "/bin/sh"
                }
            });

            let result = Dropdown::from_document(&json);
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("Missing name"));
        }
    }

    // Tests for serialization
    mod serialization {
        use super::*;

        #[test]
        fn test_dropdown_option_serialization_roundtrip() {
            let option = DropdownOption::builder()
                .label("My Label".to_string())
                .value("my_value".to_string())
                .build();

            let json = serde_json::to_string(&option).unwrap();
            let deserialized: DropdownOption = serde_json::from_str(&json).unwrap();

            assert_eq!(option, deserialized);
        }

        #[test]
        fn test_dropdown_serialization_roundtrip() {
            let dropdown = Dropdown::builder()
                .id(Uuid::new_v4())
                .name("test".to_string())
                .options("a, b, c".to_string())
                .options_type(DropdownOptionType::Fixed)
                .fixed_options("a, b, c".to_string())
                .variable_options("".to_string())
                .command_options("".to_string())
                .value("a".to_string())
                .interpreter("/bin/sh".to_string())
                .delimiter(":".to_string())
                .build();

            let json = serde_json::to_string(&dropdown).unwrap();
            let deserialized: Dropdown = serde_json::from_str(&json).unwrap();

            assert_eq!(dropdown, deserialized);
        }
    }

    // Tests for custom delimiter parsing
    mod custom_delimiter {
        use super::*;

        #[test]
        fn test_from_str_with_custom_delimiter() {
            let option = DropdownOption::from_str_with_delimiter("Label|value", "|").unwrap();
            assert_eq!(option.label, "Label");
            assert_eq!(option.value, "value");
        }

        #[test]
        fn test_from_str_with_multi_char_delimiter() {
            let option = DropdownOption::from_str_with_delimiter("Label::value", "::").unwrap();
            assert_eq!(option.label, "Label");
            assert_eq!(option.value, "value");
        }

        #[test]
        fn test_from_str_with_arrow_delimiter() {
            let option =
                DropdownOption::from_str_with_delimiter("Display Name->actual_value", "->")
                    .unwrap();
            assert_eq!(option.label, "Display Name");
            assert_eq!(option.value, "actual_value");
        }

        #[test]
        fn test_from_str_no_delimiter_found() {
            let option = DropdownOption::from_str_with_delimiter("just_a_value", "|").unwrap();
            assert_eq!(option.label, "just_a_value");
            assert_eq!(option.value, "just_a_value");
        }

        #[test]
        fn test_value_contains_colon_with_pipe_delimiter() {
            // User has colons in their data, using pipe as delimiter
            let option =
                DropdownOption::from_str_with_delimiter("My Label|http://example.com:8080", "|")
                    .unwrap();
            assert_eq!(option.label, "My Label");
            assert_eq!(option.value, "http://example.com:8080");
        }

        #[test]
        fn test_vec_from_str_with_custom_delimiter() {
            let options =
                DropdownOption::vec_from_str_with_delimiter("A|1, B|2, C|3", "|").unwrap();
            assert_eq!(options.len(), 3);
            assert_eq!(options[0].label, "A");
            assert_eq!(options[0].value, "1");
            assert_eq!(options[1].label, "B");
            assert_eq!(options[1].value, "2");
            assert_eq!(options[2].label, "C");
            assert_eq!(options[2].value, "3");
        }

        #[test]
        fn test_vec_from_str_with_multi_char_delimiter() {
            let options =
                DropdownOption::vec_from_str_with_delimiter("Label A::a\nLabel B::b", "::")
                    .unwrap();
            assert_eq!(options.len(), 2);
            assert_eq!(options[0].label, "Label A");
            assert_eq!(options[0].value, "a");
            assert_eq!(options[1].label, "Label B");
            assert_eq!(options[1].value, "b");
        }

        #[test]
        fn test_default_colon_delimiter() {
            // vec_from_str should use ":" as default
            let options = DropdownOption::vec_from_str("Label:value").unwrap();
            assert_eq!(options.len(), 1);
            assert_eq!(options[0].label, "Label");
            assert_eq!(options[0].value, "value");
        }
    }
}
