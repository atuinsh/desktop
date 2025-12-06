import { generateOrEditBlock, AIFeatureDisabledError, AIGenerationError } from "@/api/ai";
import { uuidv7 } from "uuidv7";
import DevConsole from "../dev/dev_console";

export interface BlockSpec {
  type: string;
  props?: Record<string, any>;
  content?: any;
}

export interface GenerateBlocksRequest {
  prompt: string;
  blockType?: string;
  documentMarkdown?: string;
  insertAfterIndex?: number;
  insertBeforeIndex?: number;
}

export interface GenerateBlocksResponse {
  blocks: BlockSpec[];
  explanation?: string;
}

export { AIFeatureDisabledError, AIGenerationError };

export async function generateBlocks(
  request: GenerateBlocksRequest
): Promise<GenerateBlocksResponse> {
  const response = await generateOrEditBlock({
    action: "generate",
    instruction: request.prompt,
    block_type: request.blockType,
    document_markdown: request.documentMarkdown,
    insert_after_index: request.insertAfterIndex,
    insert_before_index: request.insertBeforeIndex,
  });

  // Always generate our own ID, don't trust AI-provided ones
  const block = { ...response.block, id: uuidv7() };

  return {
    blocks: [block],
    explanation: `Generated block based on: "${request.prompt}"`,
  };
}

// Helper function to create common block patterns
export const createBlockPatterns = {
  terminal: (code: string, name?: string): BlockSpec => ({
    type: "run",
    props: { code, name: name || "Terminal Command" },
  }),

  script: (code: string, name?: string, lang = "bash"): BlockSpec => ({
    type: "script",
    props: { code, name: name || "Script", lang },
  }),

  editor: (code: string, name?: string, language?: string, variableName?: string): BlockSpec => ({
    type: "editor",
    props: {
      code,
      name: name || "Editor",
      language: language || "",
      variableName: variableName || "",
    },
  }),

  env: (name: string, value: string): BlockSpec => ({
    type: "env",
    props: { name, value },
  }),

  paragraph: (text: string): BlockSpec => ({
    type: "paragraph",
    content: [{ type: "text", text }],
  }),

  heading: (text: string, level = 1): BlockSpec => ({
    type: "heading",
    props: { level },
    content: [{ type: "text", text }],
  }),

  http: (
    url: string,
    method = "GET",
    headers?: Record<string, string>,
    body?: string
  ): BlockSpec => ({
    type: "http",
    props: { url, method, headers, body },
  }),
};

// Add to dev tools in development mode
if (import.meta.env.DEV) {
  DevConsole.addAppObject("createBlockPatterns", createBlockPatterns);
}
