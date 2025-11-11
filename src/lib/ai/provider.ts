// Setup the AI Provider using official OpenRouter SDK
import { createOpenRouter } from "@openrouter/ai-sdk-provider";

export interface ModelConfig {
    apiKey: string;
    baseURL?: string;
    model?: string;
}

export const createModel = (config: ModelConfig) => {
    console.log("[Provider] Creating OpenRouter model");
    
    const openrouter = createOpenRouter({
        apiKey: config.apiKey,
    });

    // Default to a model with proven tool calling support on OpenRouter
    // anthropic/claude-3.5-sonnet has excellent tool calling support
    const modelName = config.model || "anthropic/claude-3.5-sonnet";
    console.log("[Provider] Using model:", modelName);

    return openrouter(modelName);
};
