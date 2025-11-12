import { getCurrentEditor, getEditorByRunbookId } from "@/lib/editor_utils";
import "@blocknote/core/fonts/inter.css";
import { BlockNoteSchema, defaultBlockSpecs } from "@blocknote/core";

/**
 * AI Agent tool functions for interacting with runbook editors
 */

/**
 * Get the current document from the active runbook editor as markdown with block IDs
 * Returns the document content as a markdown string with block IDs for reference
 */
export async function getCurrentDocument(): Promise<string> {
  console.log("[Tool] getCurrentDocument called");
  
  const editor = getCurrentEditor();
  if (!editor) {
    console.error("[Tool] No active runbook editor found");
    throw new Error("No active runbook editor found");
  }
  
  console.log("[Tool] Editor found, document has", editor.document.length, "blocks");
  
  // Build markdown with block IDs and types
  let result = `Document has ${editor.document.length} blocks:\n\n`;
  
  for (let i = 0; i < editor.document.length; i++) {
    const block = editor.document[i];
    result += `[${i}] ID: ${block.id} | Type: ${block.type}\n`;
    
    // Add block content preview
    if (block.props?.code) {
      result += `Code: ${block.props.code.substring(0, 100)}${block.props.code.length > 100 ? '...' : ''}\n`;
    }
    if (block.props?.name) {
      result += `Name: ${block.props.name}\n`;
    }
    if (block.content && Array.isArray(block.content)) {
      const text = block.content.map((c: any) => c.text || '').join('');
      if (text) {
        result += `Text: ${text.substring(0, 100)}${text.length > 100 ? '...' : ''}\n`;
      }
    }
    result += '\n';
  }
  
  return result;
}

/**
 * Get the current content of the active runbook editor as JSON string
 */
export async function getCurrentRunbookContent(): Promise<string> {
  const editor = getCurrentEditor();
  if (!editor) {
    throw new Error("No active runbook editor found");
  }
  
  return JSON.stringify(editor.document, null, 2);
}

/**
 * Insert a new block in the current runbook at the end
 */
export async function insertBlockInCurrentRunbook(blockType: string, content: any): Promise<void> {
  const editor = getCurrentEditor();
  if (!editor) {
    throw new Error("No active runbook editor found");
  }
  
  const lastBlock = editor.document[editor.document.length - 1];
  editor.insertBlocks(
    [
      {
        type: blockType,
        content: content,
      },
    ],
    lastBlock,
    "after"
  );
}

/**
 * Get all blocks of a specific type from the current runbook
 */
export async function getBlocksOfType(blockType: string): Promise<any[]> {
  const editor = getCurrentEditor();
  if (!editor) {
    throw new Error("No active runbook editor found");
  }
  
  return editor.document.filter((block: any) => block.type === blockType);
}

/**
 * Search for blocks containing specific text in the current runbook
 */
export async function searchBlocksByText(searchText: string): Promise<any[]> {
  const editor = getCurrentEditor();
  if (!editor) {
    throw new Error("No active runbook editor found");
  }
  
  return editor.document.filter((block: any) => {
    if (!block.content) return false;
    const blockText = JSON.stringify(block.content).toLowerCase();
    return blockText.includes(searchText.toLowerCase());
  });
}

/**
 * Example: Get all "run" blocks from the current runbook
 */
export async function getAllRunBlocks(): Promise<any[]> {
  return getBlocksOfType("run");
}

/**
 * Example: Get all "sql" or "sqlite" blocks from the current runbook
 */
export async function getAllSQLBlocks(): Promise<any[]> {
  const editor = getCurrentEditor();
  if (!editor) {
    throw new Error("No active runbook editor found");
  }
  
  return editor.document.filter(
    (block: any) => block.type === "sql" || block.type === "sqlite"
  );
}

