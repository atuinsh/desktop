// Atuin MCP Server types

export interface AtuinCliResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}

// Runbook / Workspace types (matching .atrb YAML format)

export interface RunbookFile {
  id: string;
  name: string;
  version: number;
  forkedFrom?: string | null;
  content: Block[];
}

export interface Block {
  id: string;
  type: string;
  props: Record<string, unknown>;
  content?: InlineContent[];
  children?: Block[];
}

export interface InlineContent {
  type: string;
  text?: string;
  styles?: Record<string, boolean>;
  href?: string;
  content?: InlineContent[];
}

export interface WorkspaceConfig {
  workspace: {
    id: string;
    name: string;
  };
}

export interface WorkspaceInfo {
  id: string;
  name: string;
  path: string;
  runbooks: RunbookSummary[];
}

export interface RunbookSummary {
  id: string;
  name: string;
  path: string;
  blockCount: number;
}

// Block execution types (for desktop API)

export interface BlockExecutionRequest {
  runbookId: string;
  block: Block;
  context?: Record<string, string>;
}

export interface BlockExecutionResult {
  success: boolean;
  output?: string;
  error?: string;
}
