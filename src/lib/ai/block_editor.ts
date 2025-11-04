import { generateText } from "ai";
import { createModel, type ModelConfig } from "./provider";
import { Settings } from "@/state/settings";
import { HTTP_LLM_PROMPT as HTTP_LLM_PROMPT } from "@/lib/blocks/http/schema";

export interface EditBlockRequest {
  prompt: string;
  currentBlock: any;
  apiKey?: string;
  apiEndpoint?: string;
  model?: string;
  editorContext?: {
    blocks: any[];
    currentBlockId: string;
    currentBlockIndex: number;
  };
}

export interface EditBlockResponse {
  updatedBlock: any;
  explanation?: string;
}

// Helper to get HTTP LLM prompt from new structure
const getHttpLLMPrompt = () => HTTP_LLM_PROMPT;

const getSystemPromptForBlockType = (blockType: string) => {
  const basePrompt = `You are an expert at editing runbook blocks. You will be given a block and a modification request.
Your job is to update the block according to the user's request while preserving the block structure.

TEMPLATE VARIABLE SYSTEM:
- Runbooks support template variables using {{ var.variable_name }} syntax
- Variables can be created with 'var' blocks (synced across runbooks) or 'local-var' blocks (device-only)
- Variables can be referenced in commands, SQL queries, HTTP requests, scripts, etc.
- Common pattern: store output in variables for reuse (e.g., API responses, database IDs, file paths)
- Suggest using template variables when appropriate for dynamic content

IMPORTANT: Return ONLY a valid JSON object with the updated block. Do not include any markdown formatting or explanation text.

The response should be a JSON object with:
- type: the block type (keep same unless specifically asked to change)
- props: the updated properties
- content: the updated content (if applicable)  
- id: preserve the original ID

`;

  const typeSpecificPrompts = {
    run: `For 'run' blocks (terminal commands):
- Focus on the 'code' property which contains the command
- Can reference template variables: {{ var.variable_name }}
- Common requests: fix syntax, optimize performance, add error handling, use template variables
- Example: {"type": "run", "props": {"code": "curl {{ var.api_url }}/users", "name": "Get users", "outputVariable": "users_response"}, "id": "original-id"}`,

    script: `For 'script' blocks:
- Focus on the 'code' property which contains the script
- Can reference template variables: {{ var.variable_name }}
- Can store output in variables using 'outputVariable' prop
- Update 'lang' property if changing script language
- Common requests: optimize, add error handling, convert language, use template variables
- Example: {"type": "script", "props": {"code": "echo 'Processing {{ var.file_path }}'", "lang": "bash", "outputVariable": "process_result"}, "id": "original-id"}`,

    postgres: `For 'postgres' blocks (SQL queries):
- Focus on the 'code' property which contains the SQL
- Can reference template variables in queries: {{ var.variable_name }}
- Can store query results in variables using 'outputVariable' prop
- Common requests: optimize query, fix syntax, add indexing hints, use dynamic values
- Example: {"type": "postgres", "props": {"code": "SELECT * FROM users WHERE id = {{ var.user_id }}", "outputVariable": "user_data"}, "id": "original-id"}`,

    sqlite: `For 'sqlite' blocks (SQL queries):
- Focus on the 'code' property which contains the SQL
- Can reference template variables in queries: {{ var.variable_name }}
- Can store query results in variables using 'outputVariable' prop
- Common requests: optimize query, fix syntax, convert to different SQL dialect, use dynamic values
- Example: {"type": "sqlite", "props": {"code": "SELECT * FROM logs WHERE date >= '{{ var.start_date }}'", "outputVariable": "filtered_logs"}, "id": "original-id"}`,

    clickhouse: `For 'clickhouse' blocks (ClickHouse queries):
- Focus on the 'code' property which contains the ClickHouse SQL
- Can reference template variables in queries: {{ var.variable_name }}
- Can store query results in variables using 'outputVariable' prop
- Common requests: optimize for ClickHouse, add proper aggregations, use dynamic values
- Example: {"type": "clickhouse", "props": {"code": "SELECT count() FROM events WHERE tenant_id = {{ var.tenant_id }}", "outputVariable": "event_count"}, "id": "original-id"}`,

    http: getHttpLLMPrompt(),

    prometheus: `For 'prometheus' blocks (PromQL queries):
- Focus on the 'code' property which contains the PromQL
- Can reference template variables in queries: {{ var.variable_name }}
- Can store query results in variables using 'outputVariable' prop
- Common requests: optimize query, add alerting conditions, improve time ranges, use dynamic values
- Example: {"type": "prometheus", "props": {"code": "rate(http_requests_total{service='{{ var.service_name }}'}[5m])", "outputVariable": "request_rate"}, "id": "original-id"}`,

    var: `For 'var' blocks (template variables):
- Focus on 'name' and 'value' properties
- These create variables accessible via {{ var.name }} in other blocks
- Synced across runbooks in the same account
- Common requests: change variable name, update default value, create new variables
- Example: {"type": "var", "props": {"name": "api_endpoint", "value": "https://api.example.com"}, "id": "original-id"}`,

    "local-var": `For 'local-var' blocks (local template variables):
- Focus on 'name' and 'value' properties
- These create device-only variables accessible via {{ var.name }} in other blocks
- Not synced across devices (good for credentials, local paths)
- Common requests: change variable name, update default value, create local variables
- Example: {"type": "local-var", "props": {"name": "local_db_path", "value": "/usr/local/data/app.db"}, "id": "original-id"}`,

    env: `For 'env' blocks (environment variables):
- Focus on 'name' and 'value' properties
- Can reference template variables in values: {{ var.variable_name }}
- Common requests: change env var name, update value, use dynamic values
- Example: {"type": "env", "props": {"name": "DATABASE_URL", "value": "postgres://user:pass@{{ var.db_host }}:5432/{{ var.db_name }}"}, "id": "original-id"}`,

    paragraph: `For 'paragraph' blocks (text content):
- Focus on the 'content' array which contains text formatting
- Can reference template variables in text: {{ var.variable_name }}
- Common requests: improve clarity, add detail, fix formatting, use dynamic content
- Example: {"type": "paragraph", "content": [{"type": "text", "text": "Processing data for {{ var.environment }} environment"}], "id": "original-id"}`
  };

  return basePrompt + (typeSpecificPrompts[blockType as keyof typeof typeSpecificPrompts] || typeSpecificPrompts.paragraph);
};

export async function editBlock(request: EditBlockRequest): Promise<EditBlockResponse> {
  const aiEnabled = await Settings.aiEnabled();
  if (!aiEnabled) {
    throw new Error("AI is not enabled. Please enable AI in settings.");
  }
  
  // Get API configuration from settings or use provided values
  const storedApiKey = await Settings.aiApiKey();
  const storedEndpoint = await Settings.aiApiEndpoint();
  const storedModel = await Settings.aiModel();
  
  const apiKey = request.apiKey || storedApiKey;
  const apiEndpoint = request.apiEndpoint || storedEndpoint;
  const modelName = request.model || storedModel;
  
  if (!apiKey || apiKey.trim() === '') {
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

  const systemPrompt = getSystemPromptForBlockType(request.currentBlock.type);
  
  let userPrompt = `Current block:
${JSON.stringify(request.currentBlock, null, 2)}

User request: ${request.prompt}`;

  // Add editor context if provided
  if (request.editorContext) {
    const { blocks, currentBlockId, currentBlockIndex } = request.editorContext;
    const totalBlocks = blocks.length;
    
    userPrompt += `

RUNBOOK CONTEXT:
- Current document has ${totalBlocks} blocks
- Editing block ${currentBlockIndex + 1} of ${totalBlocks} (ID: ${currentBlockId})
- Full runbook for context:
\`\`\`json
${JSON.stringify(blocks, null, 2)}
\`\`\``;
  }

  userPrompt += `

Update the block according to the user's request. Return only the updated block as valid JSON.`;

  try {
    const result = await generateText({
      model,
      system: systemPrompt,
      prompt: userPrompt,
      temperature: 0.1, // Lower temperature for more consistent edits
    });

    // Parse the AI response as JSON
    let updatedBlock;
    try {
      // Clean up response in case AI added markdown formatting
      const cleanedResponse = result.text.replace(/```json\n?|\n?```/g, '').trim();
      updatedBlock = JSON.parse(cleanedResponse);
    } catch (parseError) {
      throw new Error(`Failed to parse AI response as JSON: ${parseError}`);
    }

    // Ensure updatedBlock exists
    if (!updatedBlock) {
      throw new Error('AI returned an invalid response format');
    }
    
    // Ensure the ID is preserved
    if (!updatedBlock.id) {
      updatedBlock.id = request.currentBlock.id;
    }

    return {
      updatedBlock,
      explanation: `Updated ${request.currentBlock.type} block based on: "${request.prompt}"`
    };
  } catch (error) {
    console.error('Block editing failed:', error);
    throw error;
  }
}

// Check if AI editing is enabled (reuse the same check as block generation)
export async function isAIEditEnabled(): Promise<boolean> {
  return await Settings.aiEnabled();
}
