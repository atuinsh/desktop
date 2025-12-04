use serde::{Deserialize, Serialize};
use ts_rs::TS;
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::{
    blocks::{Block, BlockBehavior, FromDocument},
    context::BlockState,
    execution::{ExecutionContext, ExecutionHandle},
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TS)]
#[ts(export)]
struct MarkdownRenderState {
    resolved_variable_name: Option<String>,
}

impl BlockState for MarkdownRenderState {}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownRender {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into), default = String::new())]
    pub variable_name: String,
}

impl FromDocument for MarkdownRender {
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

        let variable_name = props
            .get("variableName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(MarkdownRender::builder()
            .id(id)
            .variable_name(variable_name)
            .build())
    }
}

#[async_trait::async_trait]
impl BlockBehavior for MarkdownRender {
    fn id(&self) -> Uuid {
        self.id
    }

    fn into_block(self) -> Block {
        Block::MarkdownRender(self)
    }

    fn create_state(&self) -> Option<Box<dyn BlockState>> {
        Some(Box::new(MarkdownRenderState {
            resolved_variable_name: None,
        }))
    }

    async fn execute(
        self,
        context: ExecutionContext,
    ) -> Result<Option<ExecutionHandle>, Box<dyn std::error::Error + Send + Sync>> {
        let resolved_variable_name = context
            .context_resolver
            .resolve_template(&self.variable_name)
            .unwrap_or_default();
        context
            .update_block_state::<MarkdownRenderState, _>(self.id, |state| {
                state.resolved_variable_name = Some(resolved_variable_name);
            })
            .await?;

        Ok(None)
    }
}
