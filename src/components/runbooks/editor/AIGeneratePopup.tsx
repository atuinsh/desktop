import { useCallback } from "react";
import {
  generateBlocks,
  BlockSpec,
  AIFeatureDisabledError,
  AIQuotaExceededError,
} from "@/lib/ai/block_generator";
import { AIPopupBase } from "./ui/AIPopupBase";
import track_event from "@/tracking";

interface EditorContext {
  documentMarkdown?: string;
  currentBlockId: string;
  currentBlockIndex: number;
  runbookId?: string;
}

interface AIGeneratePopupProps {
  isVisible: boolean;
  position: { x: number; y: number };
  onBlockGenerated: (block: BlockSpec) => void;
  onGenerateComplete: () => void;
  onClose: () => void;
  getEditorContext?: () => Promise<EditorContext | undefined>;
}

export function AIGeneratePopup({
  isVisible,
  position,
  onBlockGenerated,
  onGenerateComplete,
  onClose,
  getEditorContext,
}: AIGeneratePopupProps) {
  const handleGenerate = useCallback(
    async (prompt: string) => {
      track_event("runbooks.ai.generate_popup", { prompt_length: prompt.length });

      try {
        // Get editor context for document-aware generation
        const context = getEditorContext ? await getEditorContext() : undefined;

        const result = await generateBlocks({
          prompt,
          documentMarkdown: context?.documentMarkdown,
          insertAfterIndex: context?.currentBlockIndex,
          runbookId: context?.runbookId,
        });

        if (result.blocks.length > 0) {
          track_event("runbooks.ai.generate_success", {
            blocks_generated: result.blocks.length,
            prompt_length: prompt.length,
          });

          for (const block of result.blocks) {
            onBlockGenerated(block);
          }
        }

        onGenerateComplete();
      } catch (error) {
        if (error instanceof AIFeatureDisabledError) {
          track_event("runbooks.ai.generate_feature_disabled", {
            prompt_length: prompt.length,
          });
        } else if (error instanceof AIQuotaExceededError) {
          track_event("runbooks.ai.generate_quota_exceeded", {
            prompt_length: prompt.length,
          });
        } else {
          track_event("runbooks.ai.generate_error", {
            error: error instanceof Error ? error.message : "Unknown error",
            prompt_length: prompt.length,
          });
        }
        throw error;
      }
    },
    [onBlockGenerated, onGenerateComplete, getEditorContext]
  );

  return (
    <AIPopupBase
      isVisible={isVisible}
      position={position}
      onClose={onClose}
      onSubmit={handleGenerate}
      title="Generate block"
      placeholder="e.g., curl command to fetch users, SQL query to find recent orders..."
      submitButtonText="Generate"
      submitButtonLoadingText="Generating..."
      showSuggestions={false}
    />
  );
}
