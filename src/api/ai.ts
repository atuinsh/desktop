import { post } from "./http";
import { platform, arch } from "@tauri-apps/plugin-os";
import { invoke } from "@tauri-apps/api/core";

type BlockName = string;
type BlockType = string;

export interface SystemInfo {
  arch: string;
  os: string;
  shell: string;
}

export interface AIBlockRequest {
  action: "edit" | "generate";
  block?: any;
  instruction: string;
  block_type?: string;
  document_markdown?: string;
  block_index?: number;
  insert_before_index?: number;
  insert_after_index?: number;
  system_info?: SystemInfo;
  runbook_id?: string;
  context?: AIContext;
}

export interface AIContext {
  variables: Array<string>;
  named_blocks: Array<[BlockName, BlockType]>;
  environment_variables: Array<string>;
  working_directory: string | null;
  ssh_host: string | null;
}

export interface AISingleBlockResponse {
  block: {
    type: string;
    props: Record<string, any>;
    content?: any[];
    id: string;
  };
}

type AIBlock = AISingleBlockResponse["block"];

export interface AIMultiBlockResponse {
  blocks: Array<AIBlock>;
}

export type AIResponse = AISingleBlockResponse | AIMultiBlockResponse;

export interface AIBlockError {
  error: string;
  details?: string;
}

export class AIFeatureDisabledError extends Error {
  constructor(message: string = "AI feature not enabled for this account") {
    super(message);
    this.name = "AIFeatureDisabledError";
  }
}

export class AIGenerationError extends Error {
  details?: string;

  constructor(message: string, details?: string) {
    super(message);
    this.name = "AIGenerationError";
    this.details = details;
  }
}

export class AIQuotaExceededError extends Error {
  constructor(message: string = "AI quota exceeded") {
    super(message);
    this.name = "AIQuotaExceededError";
  }
}

async function getSystemInfo(): Promise<SystemInfo> {
  const [osName, archName, shell] = await Promise.all([
    Promise.resolve(platform()),
    Promise.resolve(arch()),
    invoke<string>("get_default_shell").catch(() => "unknown"),
  ]);

  return {
    os: osName,
    arch: archName,
    shell,
  };
}

export async function generateOrEditBlock(request: AIBlockRequest): Promise<AIResponse> {
  try {
    // Get system info for context
    const systemInfo = await getSystemInfo();

    const body: Record<string, any> = {
      action: request.action,
      instruction: request.instruction,
      system_info: systemInfo,
    };

    if (request.block !== undefined) {
      body.block = request.block;
    }
    if (request.block_type !== undefined) {
      body.block_type = request.block_type;
    }
    if (request.document_markdown !== undefined) {
      body.document_markdown = request.document_markdown;
    }
    if (request.block_index !== undefined) {
      body.block_index = request.block_index;
    }
    if (request.insert_before_index !== undefined) {
      body.insert_before_index = request.insert_before_index;
    }
    if (request.insert_after_index !== undefined) {
      body.insert_after_index = request.insert_after_index;
    }
    if (request.runbook_id !== undefined) {
      body.runbook_id = request.runbook_id;
    }
    if (request.context !== undefined) {
      body.context = request.context;
    }

    const response = await post<AIResponse>("/ai/blocks", body);

    return response;
  } catch (error: any) {
    if (error?.code === 403) {
      throw new AIFeatureDisabledError();
    }

    if (error?.code === 429) {
      throw new AIQuotaExceededError();
    }

    if (error?.code === 400) {
      throw new AIGenerationError("Invalid request", error?.details);
    }

    if (error?.code === 500) {
      const details = error?.details || "An error occurred during AI generation";
      throw new AIGenerationError("AI generation failed", details);
    }

    throw new AIGenerationError(
      "Failed to connect to AI service",
      error?.message || "Unknown error",
    );
  }
}
