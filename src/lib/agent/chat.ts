import { streamText, generateText, type CoreMessage, tool } from "ai";
import { createOpenRouter } from "@openrouter/ai-sdk-provider";
import { Settings } from "@/state/settings";
import { z } from "zod";
import { getCurrentDocument } from "./tools";
import { insertBlockAtEnd, insertBlocksAfter, deleteBlockById, deleteBlocksByIds, replaceBlockById, updateBlockById } from "./block_tools";

/**
 * Helper to control multi-step execution in AI SDK.
 * AI SDK v5 defaults to stepCountIs(1), stopping after one step.
 */
const stepCountIs = (n: number) => ({ stepCount }: { stepCount: number }) => stepCount >= n;

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
}

/**
 * Available tools for the AI agent
 */
const tools = {
  getCurrentDocument: tool({
    description: "Get the current runbook document as markdown. Returns the full content of the active runbook.",
    inputSchema: z.object({}),
    execute: async () => {
      const result = await getCurrentDocument();
      console.log("[Tool] getCurrentDocument executed, length:", result.length);
      return result;
    },
  }),
  
  addBlocks: tool({
    description: `Add up to 3 blocks to the end of the current runbook. Limit to 3 blocks per call for performance.

CRITICAL: Text content uses BlockNote schema, NOT markdown!
- Correct: content: [{type: "text", text: "bold", styles: {bold: true}}]
- Wrong: content: [{type: "text", text: "**bold**"}]

Block examples:
- Paragraph: {type: "paragraph", content: [{type: "text", text: "Hello"}]}
- Bold: {type: "paragraph", content: [{type: "text", text: "Important", styles: {bold: true}}]}
- Run: {type: "run", props: {code: "ls -la", name: "List files"}}
- Var: {type: "var", props: {name: "API_KEY", value: "secret"}}`,
    inputSchema: z.object({
      blocks: z.array(z.object({
        type: z.string(),
        props: z.record(z.any()).optional(),
        content: z.any().optional(),
      })).max(3).describe("Maximum 3 blocks per call"),
    }),
    execute: async ({ blocks }) => {
      console.log("[Tool] addBlocks executing with", blocks.length, "blocks");
      for (const block of blocks) {
        insertBlockAtEnd(block);
      }
      return `Added ${blocks.length} block(s) to end of runbook`;
    },
  }),

  insertBlocksAfter: tool({
    description: `Insert up to 3 blocks after a specific block (by ID). Use this to insert blocks in the middle of the runbook.`,
    inputSchema: z.object({
      afterBlockId: z.string().describe("ID of the block to insert after"),
      blocks: z.array(z.object({
        type: z.string(),
        props: z.record(z.any()).optional(),
        content: z.any().optional(),
      })).max(3).describe("Maximum 3 blocks per call"),
    }),
    execute: async ({ afterBlockId, blocks }) => {
      console.log("[Tool] insertBlocksAfter executing after", afterBlockId, "with", blocks.length, "blocks");
      try {
        insertBlocksAfter(afterBlockId, blocks);
        return `Inserted ${blocks.length} block(s) after ${afterBlockId}`;
      } catch (error) {
        const msg = error instanceof Error ? error.message : String(error);
        console.error("[Tool] insertBlocksAfter failed:", msg);
        throw error;
      }
    },
  }),

  deleteBlock: tool({
    description: "Delete a block from the current runbook by its ID. For multiple blocks, use deleteBlocks instead.",
    inputSchema: z.object({
      blockId: z.string().describe("The ID of the block to delete"),
    }),
    execute: async ({ blockId }) => {
      console.log("[Tool] deleteBlock executing for", blockId);
      try {
        deleteBlockById(blockId);
        return `Deleted block ${blockId}`;
      } catch (error) {
        const msg = error instanceof Error ? error.message : String(error);
        console.error("[Tool] deleteBlock failed:", msg);
        throw error;
      }
    },
  }),

  deleteBlocks: tool({
    description: "Delete multiple blocks from the current runbook by their IDs. More efficient than calling deleteBlock multiple times.",
    inputSchema: z.object({
      blockIds: z.array(z.string()).describe("Array of block IDs to delete"),
    }),
    execute: async ({ blockIds }) => {
      console.log("[Tool] deleteBlocks executing for", blockIds.length, "blocks");
      try {
        deleteBlocksByIds(blockIds);
        return `Deleted ${blockIds.length} blocks`;
      } catch (error) {
        const msg = error instanceof Error ? error.message : String(error);
        console.error("[Tool] deleteBlocks failed:", msg);
        throw error;
      }
    },
  }),

  replaceBlock: tool({
    description: "Replace a block in the current runbook with new blocks",
    inputSchema: z.object({
      blockId: z.string().describe("The ID of the block to replace"),
      newBlocks: z.array(z.object({
        type: z.string(),
        props: z.record(z.any()).optional(),
        content: z.any().optional(),
      })).describe("The new blocks to insert in place of the old block"),
    }),
    execute: async ({ blockId, newBlocks }) => {
      console.log("[Tool] replaceBlock executing for", blockId, "with", newBlocks.length, "blocks");
      replaceBlockById(blockId, newBlocks);
      return `Replaced block ${blockId} with ${newBlocks.length} new block(s)`;
    },
  }),

  updateBlock: tool({
    description: "Update a block's properties or content without replacing it entirely. Use this to modify code, change names, update text, etc.",
    inputSchema: z.object({
      blockId: z.string().describe("The ID of the block to update"),
      props: z.record(z.any()).optional().describe("Properties to update (e.g., {code: 'new code', name: 'new name'})"),
      content: z.any().optional().describe("Content to update (e.g., [{type: 'text', text: 'new text'}])"),
    }),
    execute: async ({ blockId, props, content }) => {
      console.log("[Tool] updateBlock executing for", blockId);
      const updates: any = {};
      if (props) updates.props = props;
      if (content) updates.content = content;
      updateBlockById(blockId, updates);
      return `Updated block ${blockId}`;
    },
  }),
};

export interface ToolCallEvent {
  toolName: string;
  args: any;
  result?: any;
}

export interface ChatStreamOptions {
  messages: ChatMessage[];
  onChunk?: (text: string) => void;
  onToolCall?: (toolCall: ToolCallEvent) => void;
  onComplete?: (fullText: string) => void;
  onError?: (error: Error) => void;
}

/**
 * Stream a chat response from the AI using OpenRouter with tool support.
 * 
 * The AI can call tools during generation. By default AI SDK stops after one step,
 * but we override this with stopWhen to allow multi-step tool execution where:
 * 1. Model decides to call a tool
 * 2. Tool executes and returns result
 * 3. Model processes result and generates final response
 */
export async function streamChatResponse(options: ChatStreamOptions): Promise<void> {
  const { messages, onChunk, onToolCall, onComplete, onError } = options;

  try {
    const aiEnabled = await Settings.aiEnabled();
    if (!aiEnabled) {
      throw new Error("AI is not enabled. Please enable AI in settings.");
    }

    const apiKey = await Settings.aiApiKey();
    if (!apiKey || apiKey.trim() === "") {
      throw new Error("No API key configured. Please set your API key in Settings.");
    }

    const modelName = await Settings.aiModel() || "anthropic/claude-3.5-sonnet";

    const openrouter = createOpenRouter({ apiKey });

    const coreMessages: CoreMessage[] = messages.map((msg) => ({
      role: msg.role,
      content: msg.content,
    }));

    console.log("[Agent] Streaming with model:", modelName);

    let fullText = "";

    const result = streamText({
      model: openrouter(modelName),
      messages: coreMessages,
      tools,
      temperature: 0.7,
      stopWhen: stepCountIs(5), // Allow up to 5 steps for tool execution
      onStepFinish: (step: any) => {
        console.log("[Agent] Step finished:", {
          finishReason: step.finishReason,
          toolCalls: step.toolCalls?.length || 0,
          toolResults: step.toolResults?.length || 0,
        });
      },
    } as any); // Type assertion due to OpenRouter provider v2/AI SDK v1 compatibility

    // Consume fullStream to enable multi-step tool execution
    for await (const part of result.fullStream) {
      const partType = (part as any).type;
      console.log("[Agent] Stream part type:", partType);
      
      if (partType === 'text-delta') {
        const delta = (part as any).textDelta || (part as any).text;
        fullText += delta;
        onChunk?.(delta);
      } else if (partType === 'tool-call') {
        const toolCall = part as any;
        console.log("[Agent] Tool call streaming:", toolCall.toolName, toolCall.args);
        onToolCall?.({
          toolName: toolCall.toolName,
          args: toolCall.args,
        });
      } else if (partType === 'tool-result') {
        const toolResult = part as any;
        console.log("[Agent] Tool result streaming:", toolResult.toolName, toolResult.result);
        onToolCall?.({
          toolName: toolResult.toolName,
          args: toolResult.args,
          result: toolResult.result,
        });
      }
    }

    console.log("[Agent] Stream complete, length:", fullText.length);
    onComplete?.(fullText);
  } catch (error) {
    console.error("[Agent] Error:", error);
    onError?.(error as Error);
  }
}

/**
 * Generate a single chat response (non-streaming)
 */
export async function generateChatResponse(
  messages: ChatMessage[]
): Promise<string> {
  const aiEnabled = await Settings.aiEnabled();
  if (!aiEnabled) {
    throw new Error("AI is not enabled. Please enable AI in settings.");
  }

  const apiKey = await Settings.aiApiKey();
  if (!apiKey || apiKey.trim() === "") {
    throw new Error("No API key configured. Please set your API key in Settings.");
  }

  const modelName = await Settings.aiModel() || "anthropic/claude-3.5-sonnet";

  const openrouter = createOpenRouter({ apiKey });

  const coreMessages: CoreMessage[] = messages.map((msg) => ({
    role: msg.role,
    content: msg.content,
  }));

  const result = await generateText({
    model: openrouter(modelName),
    messages: coreMessages,
    tools,
    temperature: 0.1,
    stopWhen: stepCountIs(5), // Allow multi-step tool execution
  } as any);

  return result.text;
}

/**
 * Check if AI is enabled and configured
 */
export async function isAIChatEnabled(): Promise<boolean> {
  const aiEnabled = await Settings.aiEnabled();
  if (!aiEnabled) return false;

  const apiKey = await Settings.aiApiKey();
  return !!(apiKey && apiKey.trim() !== "");
}
