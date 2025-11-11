import { streamText, generateText, type CoreMessage, tool } from "ai";
import { createOpenRouter } from "@openrouter/ai-sdk-provider";
import { Settings } from "@/state/settings";
import { z } from "zod";
import { getCurrentDocument } from "./tools";

// Import stepCountIs for controlling multi-step execution
const stepCountIs = (n: number) => ({ stepCount }: { stepCount: number }) => stepCount >= n;

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
}

// Define tools with correct inputSchema property
const tools = {
  getCurrentDocument: tool({
    description: "Get the current runbook document as markdown. Returns the full content of the active runbook.",
    inputSchema: z.object({}),
    execute: async () => {
      console.log("[Tool] getCurrentDocument called");
      const result = await getCurrentDocument();
      console.log("[Tool] Document length:", result.length);
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
 * Stream a chat response from the AI using OpenRouter with tool support
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

    const openrouter = createOpenRouter({
      apiKey,
    });

    const coreMessages: CoreMessage[] = messages.map((msg) => ({
      role: msg.role,
      content: msg.content,
    }));

    console.log("[Agent] Streaming with model:", modelName);
    console.log("[Agent] Messages:", coreMessages.length);

    let fullText = "";

    // Use streamText with tools - override default stopWhen to allow multi-step
    const result = streamText({
      model: openrouter(modelName),
      messages: coreMessages,
      tools,
      temperature: 0.7,
      // Override default stopWhen (which is stepCountIs(1)) to allow multiple steps
      stopWhen: stepCountIs(5),
      onStepFinish: (step: any) => {
        console.log("[Agent] Step finished:", {
          stepNumber: step.stepType,
          finishReason: step.finishReason,
          toolCalls: step.toolCalls?.length || 0,
          toolResults: step.toolResults?.length || 0,
          hasText: !!step.text,
        });
      },
    } as any); // Type assertion needed due to OpenRouter v2/v1 compatibility

    // Must consume fullStream to allow tool execution to complete
    for await (const part of result.fullStream) {
      const partType = (part as any).type;
      
      if (partType === 'text-delta') {
        const delta = (part as any).textDelta || (part as any).text;
        fullText += delta;
        onChunk?.(delta);
      } else if (partType === 'tool-call') {
        console.log("[Agent] Tool called:", (part as any).toolName);
      } else if (partType === 'tool-result') {
        console.log("[Agent] Tool result received:", (part as any).toolName);
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

  const openrouter = createOpenRouter({
    apiKey,
  });

  const coreMessages: CoreMessage[] = messages.map((msg) => ({
    role: msg.role,
    content: msg.content,
  }));

  const result = await generateText({
    model: openrouter(modelName),
    messages: coreMessages,
    tools,
    temperature: 0.1,
  });

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
