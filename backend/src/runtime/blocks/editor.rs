use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use uuid::Uuid;

use crate::runtime::blocks::{
    document::block_context::{BlockContext, ContextResolver, DocumentVar},
    Block, BlockBehavior, FromDocument,
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, TypedBuilder)]
#[serde(rename_all = "camelCase")]
pub struct Editor {
    #[builder(setter(into))]
    pub id: Uuid,

    #[builder(setter(into))]
    pub name: String,

    #[builder(setter(into))]
    pub code: String,

    #[builder(setter(into))]
    pub var_name: Option<String>,

    #[builder(setter(into))]
    pub language: String,
}

impl FromDocument for Editor {
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

        let editor = Editor::builder()
            .id(id)
            .name(
                props
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Editor")
                    .to_string(),
            )
            .code(
                props
                    .get("code")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
            .var_name(
                props
                    .get("outputVariable")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            )
            .language(
                props
                    .get("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("text")
                    .to_string(),
            )
            .build();

        Ok(editor)
    }
}

impl BlockBehavior for Editor {
    fn into_block(self) -> Block {
        Block::Editor(self)
    }

    // TODO: If the variable sync switch is on for the editor, we need to replace the editor's code
    // with the value stored in the variable.
    fn passive_context(
        &self,
        resolver: &ContextResolver,
    ) -> Result<Option<BlockContext>, Box<dyn std::error::Error + Send + Sync>> {
        let mut context = BlockContext::new();
        if self.var_name.is_some() {
            let var_name = self.var_name.as_ref().unwrap();
            let var_value = resolver.resolve_template(&self.code)?;
            context.insert(DocumentVar(var_name.clone(), var_value));
        }
        Ok(Some(context))
    }
}
