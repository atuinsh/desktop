import { generateText, streamText } from "ai";
import { createModel, type ModelConfig } from "./provider";
import { Settings } from "@/state/settings";
import { invoke } from "@tauri-apps/api/core";
import DevConsole from "../dev/dev_console";

export interface BlockSpec {
  type: string;
  props?: Record<string, any>;
  content?: any;
}

export interface GenerateBlocksRequest {
  prompt: string;
  apiKey?: string;
  apiEndpoint?: string;
  model?: string;
  editorContext?: {
    blocks: any[];
    currentBlockId: string;
    currentBlockIndex: number;
  };
}

export interface GenerateBlocksResponse {
  blocks: BlockSpec[];
  explanation?: string;
}

export interface StreamGenerateBlocksRequest extends GenerateBlocksRequest {
  onBlock: (block: BlockSpec) => void;
  onComplete: () => void;
  onError: (error: Error) => void;
  abortSignal?: AbortSignal;
}

const SYSTEM_PROMPT = `You are an expert at generating runbooks for operational tasks. You will be fed a prompt, and need to generate blocks. 

Blocks are the building blocks of a runbook, and can be anything from a terminal to a postgresql client.

You have spent your career as a devops engineer, and you are now an expert at creating practical, executable runbooks.

DEPENDENCY MANAGEMENT:
- Only include dependency checks when the user explicitly asks for installation steps
- Prefer suggesting commands that use common tools already installed on most systems (curl, grep, awk, etc.)
- If suggesting specialized tools, mention them but don't auto-install

STREAMING OUTPUT FORMAT:
- Output each block as a separate JSON object on its own line (JSON Lines format)
- Each line must be a complete, valid JSON object representing a single block
- Do NOT wrap blocks in an array or outer object
- Output blocks one at a time, one per line
- After all blocks, output a final line with just "DONE"

Available block types:
- run: Terminal command execution (props: code, name) - PREFERRED for single commands or anything that is long running/interactive
- script: Shell script execution (props: code, name, lang) - use only for multi-line scripts that do not require interactive input
- env: Environment variable (props: name, value)
- var: Template variable (props: name, value)
- var_display: Display template variable (props: name). only really useful for displaying the output of a script block
- local-var: Local variable (props: name, value)
- dropdown: Dropdown selection with variable output (props: name, options, optionsType, value, interpreter)
  - optionsType: "fixed" (comma-separated), "variable" (template var name), or "command" (shell command)
  - name: output variable name to store selected value
  - value: currently selected option from the dropdown
  - options: comma-separated values OR variable name OR shell command depending on optionsType
- directory: Change directory (props: path)
- http: HTTP request (props: url, method, headers, body)
- ssh-connect: SSH connection (props: userHost, a single string with user@host:port (port is optional and assumed to be 22 if not provided))
- host-select: Host selection (props: hosts)
- prometheus: Prometheus query (props: query, url)
- sqlite: SQLite query (props: query, database)
- postgres: PostgreSQL query (props: query, host, port, database, user)
- clickhouse: ClickHouse query (props: query, host, port, database)
- editor: Code editor (props: code, language, variableName) - for multi-line text content, scripts, configs
  - language: syntax highlighting language (e.g., "yaml", "json", "bash", "python", "dockerfile")
  - variableName: optional variable name to store editor content, accessible via {{ var.variableName }}
- paragraph: Text paragraph
- heading: Heading text

SSH WORKFLOW:
- Use 'ssh-connect' block to establish SSH connection to a remote host
- ALL subsequent 'run' and 'script' blocks will execute on that SSH host automatically
- Use 'host-select' block with hosts="localhost" to switch back to local execution
- SSH connections use your local SSH agent for authentication (no passwords needed if keys are set up)
- Each terminal block runs its own unique PTY - they do NOT share state between blocks
- Working directory and environment variables are reset for each block
- Example SSH workflow:
  1. ssh-connect: host="server.example.com", user="deploy" 
  2. run: code="pwd" (runs on server.example.com)
  3. run: code="ls -la" (runs on server.example.com)
  4. host-select: hosts="localhost"
  5. run: code="pwd" (runs on localhost)

Runbooks can also include template variables, which are represented by double curly braces: {{ var.variable_name }}. 

You can access the content of other blocks in the document using: {{ doc.content["<BLOCK_NAME>"].content }}

These use MiniJinja, which is similar to Jinja2.

EDITOR BLOCKS: Use 'editor' blocks instead of heredocs for multi-line content. Set the 'language' prop for syntax highlighting (e.g., "yaml", "json", "bash", "python", "dockerfile"). Use the 'variableName' prop to store content in a template variable for later use.

TEMPLATE VARIABLES WITH EDITOR: When you need large inputs that will be used later in the runbook, create an editor block with a variableName, then reference it using {{ var.variableName }} in subsequent blocks.

Output each block on a separate line as valid JSON, for example:
{"type": "paragraph", "content": [{"type": "text", "text": "Description text"}]}
{"type": "run", "props": {"code": "command", "name": "Step name"}}
{"type": "env", "props": {"name": "VAR_NAME", "value": "value"}}
DONE

Do NOT include a code fence. Do NOT wrap blocks in an outer object or array.

IMPORTANT GUIDELINES:
- Balance blocks with explanation: Start with a paragraph explaining the task, then provide blocks
- Use 'paragraph' blocks to explain what you're about to do or provide context between steps
- Prefer the MINIMUM number of blocks needed for execution - quality over quantity
- Use 'run' blocks for single commands, NOT 'script' blocks unless you need multi-line shell scripts
- NEVER include shebangs (#!/bin/bash) - these are handled automatically
- Use descriptive, action-oriented names for blocks
- Use common system tools when possible, avoid adding new dependencies unless explicitly requested
- Make runbooks readable and understandable, not just executable

TERMINAL BLOCK ISOLATION:
- Each 'run' and 'script' block spawns its own independent PTY process
- Blocks do NOT share state: working directory, environment variables, or shell session
- If you need to maintain state across blocks, use explicit commands:
  - Set working directory with 'directory' blocks or 'cd' in each block
  - Set environment with 'env' blocks or export in each block
  - Use template variables to pass data between blocks

Create practical, executable runbooks that accomplish the user's goal efficiently.`;

interface PlatformInfo {
  os: string;
  arch: string;
  packageManager: string;
}

async function getPlatformInfo(): Promise<PlatformInfo> {
  try {
    // Get OS information from backend
    const osInfo = await invoke<string>("get_platform_info");
    
    return {
      os: osInfo,
      arch: "unknown", // Backend doesn't provide arch yet
      packageManager: getPackageManager(osInfo)
    };
  } catch (error) {
    // Fallback to basic detection
    const userAgent = navigator.userAgent.toLowerCase();
    let os = "unknown";
    
    if (userAgent.includes("mac")) os = "macOS";
    else if (userAgent.includes("linux")) os = "Linux";
    else if (userAgent.includes("windows")) os = "Windows";
    
    return {
      os,
      arch: "unknown",
      packageManager: getPackageManager(os)
    };
  }
}

function getPackageManager(os: string): string {
  switch (os.toLowerCase()) {
    case "macos":
    case "darwin":
      return "brew";
    case "ubuntu":
    case "debian":
      return "apt";
    case "rhel":
    case "centos":
    case "fedora":
      return "yum/dnf";
    case "arch":
      return "pacman";
    case "windows":
      return "choco";
    case "linux":
      return "apt/yum (detect at runtime)";
    default:
      return "platform-specific";
  }
}

export async function generateBlocks(
  request: GenerateBlocksRequest,
): Promise<GenerateBlocksResponse> {
  // Check if AI is enabled first
  const aiEnabled = await Settings.aiEnabled();
  if (!aiEnabled) {
    throw new Error("AI features are disabled. Enable them in Settings to use AI-powered runbook generation.");
  }

  // Get API configuration from settings or use provided values
  const storedApiKey = await Settings.aiApiKey();
  const storedEndpoint = await Settings.aiApiEndpoint();
  const storedModel = await Settings.aiModel();
  
  const apiKey = request.apiKey || storedApiKey;
  const apiEndpoint = request.apiEndpoint || storedEndpoint;
  const modelName = request.model || storedModel;
  
  if (!apiKey) {
    throw new Error("No API key configured. Please set your API key in Settings.");
  }

  // Get platform information for dependency management
  const platform = await getPlatformInfo();
  
  // Build enhanced prompt with context
  let enhancedPrompt = `${request.prompt}

PLATFORM CONTEXT:
- OS: ${platform.os}
- Architecture: ${platform.arch}
- Package Manager: ${platform.packageManager}`;

  // Add editor context if provided
  if (request.editorContext) {
    const { blocks, currentBlockId, currentBlockIndex } = request.editorContext;
    const totalBlocks = blocks.length;
    
    enhancedPrompt += `

EDITOR CONTEXT:
- Current document has ${totalBlocks} blocks
- Cursor is at block ${currentBlockIndex + 1} (ID: ${currentBlockId})
- When user says "above" or "below", they mean relative to block ${currentBlockIndex + 1}
- Current document blocks:
\`\`\`json
${JSON.stringify(blocks, null, 2)}
\`\`\``;
  }

  const modelConfig: ModelConfig = {
    apiKey,
    baseURL: apiEndpoint || undefined,
    model: modelName || undefined,
  };

  const model = createModel(modelConfig);

  if (!model) {
    throw new Error("AI model not configured. Please check your API settings.");
  }

  try {
    const result = await generateText({
      model,
      system: SYSTEM_PROMPT,
      prompt: enhancedPrompt,
    });

    console.log("AI Response:", result.text);

    // Try to parse JSON
    let response: GenerateBlocksResponse;
    try {
      response = JSON.parse(result.text) as GenerateBlocksResponse;
    } catch (parseError) {
      console.error("JSON Parse Error:", parseError);
      console.error("Raw response:", result.text);
      throw new Error(
        `Invalid JSON response from AI: ${
          parseError instanceof Error ? parseError.message : "Unknown parse error"
        }`,
      );
    }

    return response;
  } catch (error) {
    console.error("Failed to generate block:", error);
    throw new Error(
      `Block generation failed: ${error instanceof Error ? error.message : "Unknown error"}`,
    );
  }
}

// Streaming version that calls callbacks as blocks are generated
export async function streamGenerateBlocks(
  request: StreamGenerateBlocksRequest,
): Promise<void> {
  // Check if AI is enabled first
  const aiEnabled = await Settings.aiEnabled();
  if (!aiEnabled) {
    const error = new Error("AI features are disabled. Enable them in Settings to use AI-powered runbook generation.");
    request.onError(error);
    throw error;
  }

  // Get API configuration from settings or use provided values
  const storedApiKey = await Settings.aiApiKey();
  const storedEndpoint = await Settings.aiApiEndpoint();
  const storedModel = await Settings.aiModel();
  
  const apiKey = request.apiKey || storedApiKey;
  const apiEndpoint = request.apiEndpoint || storedEndpoint;
  const modelName = request.model || storedModel;
  
  if (!apiKey) {
    const error = new Error("No API key configured. Please set your API key in Settings.");
    request.onError(error);
    throw error;
  }

  // Get platform information for dependency management
  const platform = await getPlatformInfo();
  
  // Build enhanced prompt with context
  let enhancedPrompt = `${request.prompt}

PLATFORM CONTEXT:
- OS: ${platform.os}
- Architecture: ${platform.arch}
- Package Manager: ${platform.packageManager}`;

  // Add editor context if provided
  if (request.editorContext) {
    const { blocks, currentBlockId, currentBlockIndex } = request.editorContext;
    const totalBlocks = blocks.length;
    
    enhancedPrompt += `

EDITOR CONTEXT:
- Current document has ${totalBlocks} blocks
- Cursor is at block ${currentBlockIndex + 1} (ID: ${currentBlockId})
- When user says "above" or "below", they mean relative to block ${currentBlockIndex + 1}
- Current document blocks:
\`\`\`json
${JSON.stringify(blocks, null, 2)}
\`\`\``;
  }

  const modelConfig: ModelConfig = {
    apiKey,
    baseURL: apiEndpoint || undefined,
    model: modelName || undefined,
  };

  const model = createModel(modelConfig);

  if (!model) {
    const error = new Error("AI model not configured. Please check your API settings.");
    request.onError(error);
    throw error;
  }

  try {
    const result = await streamText({
      model,
      system: SYSTEM_PROMPT,
      prompt: enhancedPrompt,
      abortSignal: request.abortSignal,
    });

    let buffer = "";
    
    // Process the stream
    for await (const chunk of result.textStream) {
      // Check if aborted
      if (request.abortSignal?.aborted) {
        const abortError = new Error('Generation cancelled');
        abortError.name = 'AbortError';
        throw abortError;
      }
      
      buffer += chunk;
      
      // Process complete lines
      const lines = buffer.split('\n');
      buffer = lines.pop() || ""; // Keep the incomplete line in buffer
      
      for (const line of lines) {
        const trimmed = line.trim();
        if (!trimmed) continue;
        
        // Check for completion marker
        if (trimmed === "DONE") {
          request.onComplete();
          return;
        }
        
        // Try to parse as JSON block
        try {
          const block = JSON.parse(trimmed) as BlockSpec;
          request.onBlock(block);
        } catch (parseError) {
          console.warn("Failed to parse line as JSON:", trimmed, parseError);
          // Continue processing, some lines might be partial
        }
      }
    }
    
    // Process any remaining buffer
    if (buffer.trim() && buffer.trim() !== "DONE") {
      try {
        const block = JSON.parse(buffer.trim()) as BlockSpec;
        request.onBlock(block);
      } catch (parseError) {
        console.warn("Failed to parse final buffer as JSON:", buffer, parseError);
      }
    }
    
    request.onComplete();
  } catch (error) {
    console.error("Failed to stream generate blocks:", error);
    const err = new Error(
      `Block generation failed: ${error instanceof Error ? error.message : "Unknown error"}`,
    );
    request.onError(err);
    throw err;
  }
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
    body?: string,
  ): BlockSpec => ({
    type: "http",
    props: { url, method, headers, body },
  }),
};

// Utility function to check if AI is enabled
export async function isAIEnabled(): Promise<boolean> {
  return await Settings.aiEnabled();
}

// Add to dev tools in development mode
if (import.meta.env.DEV) {
  DevConsole
    .addAppObject("createBlockPatterns", createBlockPatterns)
    .addAppObject("isAIEnabled", isAIEnabled);
}
