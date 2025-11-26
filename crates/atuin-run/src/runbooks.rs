use std::path::{Path, PathBuf};

use uuid::Uuid;

type Result<T> = std::result::Result<T, RunbookError>;

#[derive(thiserror::Error, Debug)]
pub enum RunbookError {
    #[error("Failed to read runbook file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to parse YAML: {0}")]
    YamlParseError(#[from] serde_yaml::Error),

    #[error("Failed to parse JSON: {0}")]
    JsonParseError(#[from] serde_json::Error),

    #[error("Invalid runbook file {0}: {1}")]
    InvalidRunbookFile(PathBuf, String),
}

pub struct Runbook {
    pub id: Uuid,
    pub content: Vec<serde_json::Value>,
}

impl Runbook {
    pub fn new(id: Uuid, content: Vec<serde_json::Value>) -> Self {
        Self { id, content }
    }
}

pub async fn load_runbook(path_or_id: &str) -> Result<Runbook> {
    let path = PathBuf::from(path_or_id);
    if path.is_file() {
        load_runbook_from_file(&path).await
    } else {
        load_runbook_from_id(path_or_id).await
    }
}

// TODO: Find `atuin.toml` and load workspace information
async fn load_runbook_from_file(path: impl AsRef<Path>) -> Result<Runbook> {
    let content = tokio::fs::read_to_string(path.as_ref()).await?;
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(&content)?;
    let json_value: serde_json::Value = serde_yaml::from_value(yaml_value)?;
    load_runbook_from_json_value(json_value, path)
}

// TODO: handle API credentials
async fn load_runbook_from_id(_id: &str) -> Result<Runbook> {
    todo!()
}

fn load_runbook_from_json_value(
    json_value: serde_json::Value,
    path: impl AsRef<Path>,
) -> Result<Runbook> {
    let id = json_value
        .get("id")
        .and_then(|v| v.as_str())
        .and_then(|v| Uuid::parse_str(v).ok())
        .ok_or_else(|| {
            RunbookError::InvalidRunbookFile(
                path.as_ref().to_path_buf(),
                "id not found or not a valid UUID".to_string(),
            )
        })?;

    let content = json_value
        .get("content")
        .and_then(|v| v.as_array())
        .cloned()
        .ok_or_else(|| {
            RunbookError::InvalidRunbookFile(
                path.as_ref().to_path_buf(),
                "content not found or not an array".to_string(),
            )
        })?;

    Ok(Runbook::new(id, content))
}
