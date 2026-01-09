use genai::chat::Tool;
use indoc::indoc;
use serde_json::json;

pub struct AITools;

impl AITools {
    pub fn get_runboook_document() -> Tool {
        Tool::new("get_runbook_document")
            .with_description(indoc! {"
                Get the current runbook document. Returns the document as a BlockNote JSON object
                with custom blocks. Use this to read the runbook content before making edits or
                answering questions about it.
            "})
            .with_schema(json!({
                "type": "object",
                "properties": {},
                "required": [],
            }))
    }

    pub fn get_block_docs(block_types: &[String]) -> Tool {
        Tool::new("get_block_docs")
            .with_description(indoc! {"
                Get documentation for specific block types. Use this to ensure you're generating blocks
                with the correct syntax and parameters, or to understand a block's capabilities.
                You can specify multiple block types to get documentation for.
            "})
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "block_types": {
                        "type": "array",
                        "items": {
                            "type": "string",
                            "enum": block_types,
                        },
                        "description": "The type of blocks to get documentation for. Only 'http' blocks are supported for now.",
                    },
                },
                "required": ["block_types"],
            }))
    }
}
