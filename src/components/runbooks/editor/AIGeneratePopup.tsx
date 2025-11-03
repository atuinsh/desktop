import { useCallback, useRef, useEffect } from "react";
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
  const abortControllerRef = useRef<AbortController | null>(null);

  const handleGenerate = useCallback(async (prompt: string) => {
    track_event("runbooks.ai.generate_popup", { prompt_length: prompt.length });
    
    // Create abort controller for this generation
    abortControllerRef.current = new AbortController();
    
    // Get editor context if available
    const editorContext = getEditorContext ? await getEditorContext() : undefined;
    
    let blockCount = 0;
    
    const request: StreamGenerateBlocksRequest = { 
      prompt,
      editorContext,
      abortSignal: abortControllerRef.current.signal,
      onBlock: (block: BlockSpec) => {
        blockCount++;
        onBlockGenerated(block);
      },
      onComplete: () => {
        track_event("runbooks.ai.generate_success", { 
          blocks_generated: blockCount,
          prompt_length: prompt.length 
        });
        abortControllerRef.current = null;
        onGenerateComplete();
      },
      onError: (error: Error) => {
        if (error.name === 'AbortError') {
          track_event("runbooks.ai.generate_cancelled", { 
            blocks_generated: blockCount,
            prompt_length: prompt.length 
          });
        } else {
          track_event("runbooks.ai.generate_error", { 
            error: error.message,
            prompt_length: prompt.length 
          });
        }
        abortControllerRef.current = null;
        throw error;
      }
    };
    
    try {
      await streamGenerateBlocks(request);
    } catch (error) {
      // Silently handle abort errors (user cancelled)
      if (error instanceof Error && error.name !== 'AbortError') {
        throw error;
      }
    }
  }, [onBlockGenerated, onGenerateComplete, getEditorContext]);

  // Cancel ongoing generation when popup closes
  useEffect(() => {
    if (!isVisible && abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
  }, [isVisible]);

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
