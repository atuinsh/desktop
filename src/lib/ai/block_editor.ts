import {
  generateOrEditBlock,
  AIFeatureDisabledError,
  AIGenerationError,
  AIQuotaExceededError,
} from "@/api/ai";

export interface EditBlockRequest {
  prompt: string;
  currentBlock: any;
  documentMarkdown?: string;
  blockIndex?: number;
  runbookId?: string;
}

export interface EditBlockResponse {
  updatedBlock: any;
  explanation?: string;
}

export { AIFeatureDisabledError, AIGenerationError, AIQuotaExceededError };

export async function editBlock(request: EditBlockRequest): Promise<EditBlockResponse> {
  const response = await generateOrEditBlock({
    action: "edit",
    block: request.currentBlock,
    instruction: request.prompt,
    block_type: request.currentBlock?.type,
    document_markdown: request.documentMarkdown,
    block_index: request.blockIndex,
    runbook_id: request.runbookId,
  });

  // Always use the original block's ID, never trust AI-provided ones
  const updatedBlock = {
    ...response.block,
    id: request.currentBlock?.id,
  };

  return {
    updatedBlock,
    explanation: `Updated ${request.currentBlock?.type || "block"} based on: "${request.prompt}"`,
  };
}
