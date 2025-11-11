import { streamText, generateText, type CoreMessage } from "ai";
import { createModel, type ModelConfig } from "@/lib/ai/provider";
import { Settings } from "@/state/settings";

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
}

export interface ChatStreamOptions {
  messages: ChatMessage[];
  onChunk?: (text: string) => void;
  onComplete?: (fullText: string) => void;
  onError?: (error: Error) => void;
}

/**
 * Stream a chat response from the AI
 */
export async function streamChatResponse(options: ChatStreamOptions): Promise<void> {
  const { messages, onChunk, onComplete, onError } = options;

  try {
    // Check if AI is enabled
    const aiEnabled = await Settings.aiEnabled();
    if (!aiEnabled) {
      throw new Error("AI is not enabled. Please enable AI in settings.");
    }

    // Get API configuration from settings
    const apiKey = await Settings.aiApiKey();
    const apiEndpoint = await Settings.aiApiEndpoint();
    const modelName = await Settings.aiModel();

    if (!apiKey || apiKey.trim() === "") {
      throw new Error("No API key configured. Please set your API key in Settings.");
    }

    const modelConfig: ModelConfig = {
      apiKey,
      baseURL: apiEndpoint || undefined,
      model: modelName || undefined,
    };

    const model = createModel(modelConfig);

    if (!model) {
      throw new Error("AI model not configured. Please set up your API settings.");
    }

    // Convert our ChatMessage format to CoreMessage format
    const coreMessages: CoreMessage[] = messages.map((msg) => ({
      role: msg.role,
      content: msg.content,
    }));

    // Stream the response
    const result = await streamText({
      model,
      messages: coreMessages,
      temperature: 0.7,
    });

    let fullText = "";

    // Process the stream
    for await (const chunk of result.textStream) {
      fullText += chunk;
      onChunk?.(chunk);
    }

    onComplete?.(fullText);
  } catch (error) {
    console.error("Chat stream failed:", error);
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
  const apiEndpoint = await Settings.aiApiEndpoint();
  const modelName = await Settings.aiModel();

  if (!apiKey || apiKey.trim() === "") {
    throw new Error("No API key configured. Please set your API key in Settings.");
  }

  const modelConfig: ModelConfig = {
    apiKey,
    baseURL: apiEndpoint || undefined,
    model: modelName || undefined,
  };

  const model = createModel(modelConfig);

  if (!model) {
    throw new Error("AI model not configured. Please set up your API settings.");
  }

  const coreMessages: CoreMessage[] = messages.map((msg) => ({
    role: msg.role,
    content: msg.content,
  }));

  const result = await generateText({
    model,
    messages: coreMessages,
    temperature: 0.7,
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

