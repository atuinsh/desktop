// Setup the AI Provider using OpenAI-compatible API
// Supports OpenRouter, OpenAI, and any other OpenAI-compatible endpoint
import { createOpenAI } from "@ai-sdk/openai";

export interface ModelConfig {
    apiKey: string;
    baseURL?: string;
    model?: string;
}

export const createModel = (config: ModelConfig) => {
    const provider = createOpenAI({
        apiKey: config.apiKey,
        baseURL: config.baseURL || "https://openrouter.ai/api/v1",
        compatibility: "compatible",
    });

    // Default to a good general model, or use user-specified model
    const modelName = config.model || "anthropic/claude-sonnet-4";

    return provider(modelName);
};
