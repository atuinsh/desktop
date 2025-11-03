import { useCallback } from "react";
import { streamGenerateBlocks, StreamGenerateBlocksRequest, BlockSpec } from "@/lib/ai/block_generator";
import { AIPopupBase } from "./ui/AIPopupBase";
import track_event from "@/tracking";

interface AIGeneratePopupProps {
  isVisible: boolean;
  position: { x: number; y: number };
  onBlockGenerated: (block: BlockSpec) => void;
  onGenerateComplete: () => void;
  onClose: () => void;
  getEditorContext?: () => Promise<{
    blocks: any[];
    currentBlockId: string;
    currentBlockIndex: number;
  } | undefined>;
}

export function AIGeneratePopup({ 
  isVisible, 
  position, 
  onBlockGenerated, 
  onGenerateComplete, 
  onClose, 
  getEditorContext 
}: AIGeneratePopupProps) {
  const handleGenerate = useCallback(async (prompt: string) => {
    track_event("runbooks.ai.generate_popup", { prompt_length: prompt.length });
    
    // Get editor context if available
    const editorContext = getEditorContext ? await getEditorContext() : undefined;
    
    let blockCount = 0;
    
    const request: StreamGenerateBlocksRequest = { 
      prompt,
      editorContext,
      onBlock: (block: BlockSpec) => {
        blockCount++;
        onBlockGenerated(block);
      },
      onComplete: () => {
        track_event("runbooks.ai.generate_success", { 
          blocks_generated: blockCount,
          prompt_length: prompt.length 
        });
        onGenerateComplete();
      },
      onError: (error: Error) => {
        track_event("runbooks.ai.generate_error", { 
          error: error.message,
          prompt_length: prompt.length 
        });
        throw error;
      }
    };
    
    await streamGenerateBlocks(request);
  }, [onBlockGenerated, onGenerateComplete, getEditorContext]);

  return (
    <AIPopupBase
      isVisible={isVisible}
      position={position}
      onClose={onClose}
      onSubmit={handleGenerate}
      title="Generate blocks"
      placeholder="e.g., Deploy a React app to production, Set up a PostgreSQL backup script..."
      submitButtonText="Generate"
      submitButtonLoadingText="Generating..."
      showSuggestions={false}
    />
  );
}
