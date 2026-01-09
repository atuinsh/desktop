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
}
