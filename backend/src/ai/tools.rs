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
                        "description": "The block types to get documentation for.",
                    },
                },
                "required": ["block_types"],
            }))
    }

    pub fn get_default_shell() -> Tool {
        Tool::new("get_default_shell")
            .with_description(indoc! {"
                Get the default shell for the user. Use this to determine the shell to use for new script or terminal blocks.
            "})
            .with_schema(json!({
                "type": "object",
                "properties": {},
                "required": [],
            }))
    }

    pub fn insert_blocks(_block_types: &[String]) -> Tool {
        Tool::new("insert_blocks")
            .with_description(indoc! {"
                Insert one or more blocks into the runbook at a specified position.
                Use this to add new content to the runbook. Each block object is defined
                by a 'type' property, which is the name of the block type, and a 'props' property,
                which is an object containing the properties of the block as specified in the block documentation.

                In addition to custom blocks, you can insert any of the built-in BlockNote blocks by using their
                name as the 'type' property and their associated properties as the 'props' property.

                Before you use this tool, you should use the 'get_block_docs' tool for the
                block types you're planning on using to ensure you're using the correct block types and properties.

                The 'blocks' array should be a JavaScript array and not a serialized JSON string.
            "})
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "blocks": {
                        "type": "array",
                        "description": "Array of block objects to insert. Each block object must have a 'type' property and a 'props' property.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "type": {
                                    "type": "string",
                                },
                                "props": {
                                    "type": "object"
                                }
                            }

                        }
                    },
                    "position": {
                        "type": "string",
                        "enum": ["before", "after", "end"],
                        "description": "Where to insert: 'before' or 'after' the reference block, or 'end' of document"
                    },
                    "reference_block_id": {
                        "type": "string",
                        "description": "ID of the block to insert before/after. Required unless position is 'end'"
                    },
                },
                "required": ["blocks", "position"]
            }))
    }

    pub fn update_block() -> Tool {
        Tool::new("update_block")
            .with_description(indoc! {"
                Update an existing block in the runbook. Replaces the block's properties
                and/or content while preserving its ID. Use this to make minor changes to
                a single block's properties or content.

                Before you use this tool, you should use the 'get_block_docs' tool for the
                block types you're planning on using to ensure you're using the correct block types and properties.

                When updating blocks, take care to update its name as well, if it has one and the content has meaningfully changed.
            "})
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "block_id": {
                        "type": "string",
                        "description": "ID of the block to update"
                    },
                    "props": {
                        "type": "object",
                        "description": "New properties for the block"
                    },
                    "content": {
                        "type": "array",
                        "description": "New content for the block (for heading, paragraph, and other BlockNote blocks)",
                        "items": { "type": "object" }
                    }
                },
                "required": ["block_id"]
            }))
    }

    pub fn replace_blocks() -> Tool {
        Tool::new("replace_blocks")
            .with_description(indoc! {"
                Replace one or more existing blocks in the runbook with new blocks.
                The blocks at the specified IDs will be removed and the new blocks
                will be inserted in their place. You can delete blocks by specifying
                their IDs in the 'block_ids' array and not including any blocks in the
                'new_blocks' array.

                Before you use this tool, you should use the 'get_block_docs' tool for the
                block types you're planning on using to ensure you're using the correct block types and properties.
            "})
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "block_ids": {
                        "type": "array",
                        "description": "IDs of the blocks to be replaced",
                        "items": { "type": "string" }
                    },
                    "new_blocks": {
                        "type": "array",
                        "description": "Array of new block objects to insert in place of the removed blocks. Each block object must have a 'type' property and a 'props' property.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "type": {
                                    "type": "string",
                                },
                                "props": {
                                    "type": "object"
                                }
                            }

                        }
                    }
                },
                "required": ["block_ids", "new_blocks"]
            }))
    }

    pub fn submit_blocks() -> Tool {
        Tool::new("submit_blocks")
            .with_description(indoc! {"
                Submit the generated blocks to be inserted into the runbook.
                This tool should be called exactly once at the end of your response
                with all the blocks you want to insert. After calling this tool,
                the interaction ends unless the user requests edits.

                Each block object must have a 'type' property (the block type name)
                and a 'props' property (the block's properties as specified in the block documentation).
            "})
            .with_schema(json!({
                "type": "object",
                "properties": {
                    "blocks": {
                        "type": "array",
                        "description": "Array of block objects to insert. Each must have 'type' and 'props'.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "type": {
                                    "type": "string",
                                    "description": "The block type name"
                                },
                                "props": {
                                    "type": "object",
                                    "description": "The block's properties"
                                }
                            },
                            "required": ["type", "props"]
                        }
                    }
                },
                "required": ["blocks"]
            }))
    }
}
