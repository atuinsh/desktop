/**
 * Shared AI tool execution.
 *
 * Provides an AIToolRunner class that can be instantiated by components
 * to execute AI tools with configurable auto-approval settings.
 */

import { BlockNoteEditor } from "@blocknote/core";
import AIBlockRegistry from "./block_registry";
import { Settings } from "@/state/settings";
import { AIToolCall } from "@/rs-bindings/AIToolCall";

// =============================================================================
// Constants
// =============================================================================

/**
 * Default tools that are auto-approved (read-only, non-sensitive, no side effects).
 * Components can override this via AIToolRunner.setAutoApproveTools().
 */
export const DEFAULT_AUTO_APPROVE_TOOLS = [
  "get_block_docs",
  "get_default_shell",
];

/**
 * All known tool names.
 */
export const ALL_TOOL_NAMES = [
  "get_runbook_document",
  "get_block_docs",
  "get_default_shell",
  "insert_blocks",
  "update_block",
  "replace_blocks",
  "submit_blocks", // Used by inline generation
];

// =============================================================================
// Types
// =============================================================================

export interface ToolResult {
  success: boolean;
  result: string;
}

// =============================================================================
// Individual tool execution functions
// =============================================================================

async function executeGetRunbookDocument(editor: BlockNoteEditor): Promise<any> {
  return { blocks: editor.document };
}

async function executeGetBlockDocs(
  _editor: BlockNoteEditor,
  params: { block_types: string[] },
): Promise<string> {
  return params.block_types.reduce((acc, blockType) => {
    acc += AIBlockRegistry.getInstance().getBlockDocs(blockType);
    return acc;
  }, "");
}

async function executeGetDefaultShell(): Promise<string> {
  return await Settings.getSystemDefaultShell();
}

async function executeInsertBlocks(
  editor: BlockNoteEditor,
  params: { blocks: any[]; position: "before" | "after" | "end"; reference_block_id?: string },
): Promise<any> {
  const { blocks, position, reference_block_id } = params;

  if (position === "end") {
    const lastBlock = editor.document[editor.document.length - 1];
    editor.insertBlocks(blocks, lastBlock.id, "after");
  } else if (reference_block_id) {
    editor.insertBlocks(blocks, reference_block_id, position);
  } else {
    throw new Error("reference_block_id required for 'before' or 'after' position");
  }

  return { success: true };
}

async function executeUpdateBlock(
  editor: BlockNoteEditor,
  params: { block_id: string; props?: any; content?: any },
): Promise<any> {
  const { block_id, props, content } = params;

  const updates: any = {};
  if (props) updates.props = props;
  if (content) updates.content = content;

  editor.updateBlock(block_id, updates);
  return { success: true };
}

async function executeReplaceBlocks(
  editor: BlockNoteEditor,
  params: { block_ids: string[]; new_blocks: any[] },
): Promise<any> {
  const { block_ids, new_blocks } = params;

  if (block_ids.length === 0) {
    throw new Error("block_ids cannot be empty");
  }

  const blocksToReplace = editor.document.filter((b: any) => block_ids.includes(b.id));
  if (blocksToReplace.length === 0) {
    throw new Error("No blocks found with the specified IDs");
  }

  editor.replaceBlocks(blocksToReplace, new_blocks);
  return { success: true };
}

// =============================================================================
// AIToolRunner Class
// =============================================================================

/**
 * Manages tool execution for AI sessions with configurable auto-approval.
 *
 * Usage:
 * ```
 * const runner = new AIToolRunner();
 * runner.setEditor(editor);
 *
 * // For inline generation, auto-approve all read-only tools:
 * runner.setAutoApproveTools(DEFAULT_AUTO_APPROVE_TOOLS);
 *
 * // For assistant, start with minimal auto-approve and add more as user allows:
 * runner.setAutoApproveTools(["get_block_docs"]);
 * // Later, when user clicks "always allow":
 * runner.addAutoApproveTool("insert_blocks");
 * ```
 */
export class AIToolRunner {
  private editor: BlockNoteEditor | null = null;
  private autoApproveTools: Set<string>;

  constructor(initialAutoApprove: string[] = []) {
    this.autoApproveTools = new Set(initialAutoApprove);
  }

  /**
   * Set the editor instance for tool execution.
   * Must be called before executing tools that require the editor.
   */
  setEditor(editor: BlockNoteEditor | null): void {
    this.editor = editor;
  }

  /**
   * Get the current editor instance.
   */
  getEditor(): BlockNoteEditor | null {
    return this.editor;
  }

  /**
   * Replace the set of auto-approved tools.
   */
  setAutoApproveTools(tools: string[]): void {
    this.autoApproveTools = new Set(tools);
  }

  /**
   * Add a tool to the auto-approve set.
   * Useful when user clicks "always allow this session".
   */
  addAutoApproveTool(tool: string): void {
    this.autoApproveTools.add(tool);
  }

  /**
   * Remove a tool from the auto-approve set.
   */
  removeAutoApproveTool(tool: string): void {
    this.autoApproveTools.delete(tool);
  }

  /**
   * Check if a tool is currently auto-approved.
   */
  isAutoApprovable(toolName: string): boolean {
    return this.autoApproveTools.has(toolName);
  }

  /**
   * Get the list of currently auto-approved tools.
   */
  getAutoApproveTools(): string[] {
    return [...this.autoApproveTools];
  }

  /**
   * Execute a tool by name with the given parameters.
   */
  async executeTool(toolName: string, params: any): Promise<ToolResult> {
    if (!this.editor) {
      return { success: false, result: "No editor available" };
    }

    try {
      let result: any;
      switch (toolName) {
        case "get_runbook_document":
          result = await executeGetRunbookDocument(this.editor);
          break;
        case "get_block_docs":
          result = await executeGetBlockDocs(this.editor, params);
          break;
        case "get_default_shell":
          result = await executeGetDefaultShell();
          break;
        case "insert_blocks":
          result = await executeInsertBlocks(this.editor, params);
          break;
        case "update_block":
          result = await executeUpdateBlock(this.editor, params);
          break;
        case "replace_blocks":
          result = await executeReplaceBlocks(this.editor, params);
          break;
        default:
          throw new Error(`Unknown tool: ${toolName}`);
      }
      return { success: true, result: JSON.stringify(result) };
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unknown error";
      return { success: false, result: message };
    }
  }

  /**
   * Execute a tool call from an AIToolCall object.
   */
  async executeToolCall(toolCall: AIToolCall): Promise<ToolResult> {
    return this.executeTool(toolCall.name, toolCall.args);
  }

  /**
   * Execute multiple tool calls that are auto-approvable.
   * Returns a map of tool call ID to result.
   * Non-auto-approvable tools are skipped.
   */
  async executeAutoApprovableToolCalls(
    toolCalls: AIToolCall[],
  ): Promise<Map<string, ToolResult>> {
    const results = new Map<string, ToolResult>();

    for (const toolCall of toolCalls) {
      if (this.isAutoApprovable(toolCall.name)) {
        const result = await this.executeToolCall(toolCall);
        results.set(toolCall.id, result);
      }
    }

    return results;
  }
}

// =============================================================================
// Convenience exports for backwards compatibility
// =============================================================================

/**
 * @deprecated Use AIToolRunner class instead.
 * Execute a tool directly without a runner instance.
 */
export async function executeTool(
  editor: BlockNoteEditor,
  toolName: string,
  params: any,
): Promise<ToolResult> {
  const runner = new AIToolRunner();
  runner.setEditor(editor);
  return runner.executeTool(toolName, params);
}

/**
 * @deprecated Use AIToolRunner.isAutoApprovable() instead.
 * Check if a tool is in the default auto-approve list.
 */
export function isAutoApprovable(toolName: string): boolean {
  return DEFAULT_AUTO_APPROVE_TOOLS.includes(toolName);
}

/**
 * Backwards compatibility - expose as Set for existing code.
 * @deprecated Use DEFAULT_AUTO_APPROVE_TOOLS array or AIToolRunner instead.
 */
export const AUTO_APPROVE_TOOLS = new Set(DEFAULT_AUTO_APPROVE_TOOLS);
