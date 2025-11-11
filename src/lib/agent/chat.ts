import { streamText, generateText, type CoreMessage, tool } from "ai";
import { createOpenRouter } from "@openrouter/ai-sdk-provider";
import { Settings } from "@/state/settings";
import { z } from "zod";
import { getCurrentDocument } from "./tools";

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
};

export interface ChatStreamOptions {
  messages: ChatMessage[];
  onChunk?: (text: string) => void;
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
  const { messages, onChunk, onComplete, onError } = options;

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
      
      if (partType === 'text-delta') {
        const delta = (part as any).textDelta || (part as any).text;
        fullText += delta;
        onChunk?.(delta);
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
