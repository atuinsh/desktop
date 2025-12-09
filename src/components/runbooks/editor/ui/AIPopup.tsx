import { useCallback } from "react";
import { AIPopupBase } from "./AIPopupBase";
import { AIFeatureDisabledError } from "@/lib/ai/block_editor";
import track_event from "@/tracking";

interface EditorContext {
  documentMarkdown?: string;
  currentBlockId: string;
  currentBlockIndex: number;
}

interface AIPopupProps {
  isOpen: boolean;
  onClose: () => void;
  editor: any;
  currentBlock: any;
  position?: { x: number; y: number };
  getEditorContext?: () => Promise<EditorContext | undefined>;
}

const blockSuggestions: Record<string, string[]> = {
  run: [
    "Use template variables for dynamic values",
    "Add error handling and validation",
    "Fix syntax and optimize performance",
    "Convert to different shell language",
  ],
  script: [
    "Use template variables for dynamic values",
    "Add error handling and validation",
    "Store output in a variable",
    "Fix syntax and optimize performance",
    "Convert to different shell language",
  ],
  postgres: [
    "Use template variables in query",
    "Store query results in variable",
    "Optimize this SQL query",
    "Add proper indexing suggestions",
    "Convert to different SQL dialect",
  ],
  sqlite: [
    "Use template variables in query",
    "Store query results in variable",
    "Optimize this SQL query",
    "Add proper indexing suggestions",
    "Convert to different SQL dialect",
  ],
  clickhouse: [
    "Use template variables in query",
    "Store query results in variable",
    "Optimize this SQL query",
    "Add proper indexing suggestions",
    "Convert to different SQL dialect",
  ],
  http: [
    "Use template variables for dynamic URLs",
    "Store response in a variable",
    "Add authentication headers",
    "Improve error handling",
    "Add request validation",
  ],
  prometheus: [
    "Use template variables for dynamic queries",
    "Store metrics in a variable",
    "Improve PromQL query performance",
    "Add alerting conditions",
    "Optimize time range",
  ],
  var: [
    "Change variable name",
    "Update default value",
    "Add description or comments",
    "Create related variables",
    "Set environment-specific values",
  ],
  "local-var": [
    "Change variable name",
    "Update default value",
    "Add description or comments",
    "Create related variables",
    "Set environment-specific values",
  ],
  env: [
    "Use template variables in value",
    "Add environment-specific values",
    "Improve security practices",
    "Add validation or defaults",
    "Update variable naming",
  ],
};

const getBlockSuggestions = (blockType: string): string[] => {
  return (
    blockSuggestions[blockType] || [
      "Use template variables for dynamic content",
      "Improve this content",
      "Add more detail",
      "Fix formatting",
      "Make it clearer",
    ]
  );
};

const getBlockTypeDisplay = (blockType: string) => {
  const typeMap: Record<string, string> = {
    run: "Terminal",
    script: "Script",
    postgres: "PostgreSQL",
    sqlite: "SQLite",
    clickhouse: "ClickHouse",
    http: "HTTP Request",
    prometheus: "Prometheus",
    var: "Variable",
    env: "Environment",
    paragraph: "Text",
  };
  return typeMap[blockType] || blockType;
};

export default function AIPopup({
  isOpen,
  onClose,
  editor,
  currentBlock,
  position = { x: 0, y: 0 },
  getEditorContext,
}: AIPopupProps) {
  const blockType = currentBlock?.type || "paragraph";
  const suggestions = getBlockSuggestions(blockType);
  const blockTypeDisplay = getBlockTypeDisplay(blockType);

  const handleEdit = useCallback(
    async (prompt: string) => {
      if (!currentBlock) return;

      track_event("runbooks.ai.edit_request", {
        blockType,
        promptLength: prompt.length,
        blockId: currentBlock.id,
      });

      try {
        const { editBlock } = await import("@/lib/ai/block_editor");

        // Get editor context for document-aware editing
        const context = getEditorContext ? await getEditorContext() : undefined;

        const result = await editBlock({
          prompt,
          currentBlock,
          documentMarkdown: context?.documentMarkdown,
          blockIndex: context?.currentBlockIndex,
        });

        if (result.updatedBlock) {
          editor.updateBlock(currentBlock.id, result.updatedBlock);

          track_event("runbooks.ai.edit_success", {
            blockType,
            blockId: currentBlock.id,
          });
        }
      } catch (error) {
        if (error instanceof AIFeatureDisabledError) {
          track_event("runbooks.ai.edit_feature_disabled", {
            blockType,
            blockId: currentBlock.id,
          });
        } else {
          track_event("runbooks.ai.edit_error", {
            blockType,
            blockId: currentBlock.id,
            error: error instanceof Error ? error.message : "Unknown error",
          });
        }
        throw error;
      }
    },
    [blockType, currentBlock, editor, getEditorContext]
  );

  const handleSuggestionClick = useCallback(
    (suggestion: string) => {
      track_event("runbooks.ai.suggestion_clicked", {
        blockType,
        suggestion,
        blockId: currentBlock?.id,
      });
    },
    [blockType, currentBlock?.id]
  );

  return (
    <AIPopupBase
      isVisible={isOpen}
      position={position}
      onClose={onClose}
      onSubmit={handleEdit}
      title={`Edit ${blockTypeDisplay} block`}
      placeholder={`Describe how to modify this ${blockTypeDisplay.toLowerCase()} block...`}
      submitButtonText="Apply"
      submitButtonLoadingText="Applying..."
      suggestions={suggestions}
      showSuggestions={true}
      onSuggestionClick={handleSuggestionClick}
    />
  );
}
