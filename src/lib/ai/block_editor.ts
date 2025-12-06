import { generateOrEditBlock, AIFeatureDisabledError, AIGenerationError } from "@/api/ai";

export interface EditBlockRequest {
  prompt: string;
  currentBlock: any;
  documentMarkdown?: string;
  blockIndex?: number;
}

export interface EditBlockResponse {
  updatedBlock: any;
  explanation?: string;
}

export { AIFeatureDisabledError, AIGenerationError };

export async function editBlock(request: EditBlockRequest): Promise<EditBlockResponse> {
  const response = await generateOrEditBlock({
    action: "edit",
    block: request.currentBlock,
    instruction: request.prompt,
    block_type: request.currentBlock?.type,
    document_markdown: request.documentMarkdown,
    block_index: request.blockIndex,
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
