import { getCurrentEditor } from "@/lib/editor_utils";

/**
 * Simple block manipulation - directly insert blocks at the end of the document
 */
export function insertBlocksAtEnd(blocks: any[]): void {
  const editor = getCurrentEditor();
  if (!editor) {
    throw new Error("No active runbook editor found");
  }

  const lastBlock = editor.document[editor.document.length - 1];
  if (lastBlock) {
    editor.insertBlocks(blocks, lastBlock, "after");
  }
}

/**
 * Insert a single block at the end - for streaming one block at a time
 */
export function insertBlockAtEnd(block: any): void {
  insertBlocksAtEnd([block]);
}

/**
 * Insert blocks after a specific block ID
 */
export function insertBlocksAfter(afterBlockId: string, blocks: any[]): void {
  const editor = getCurrentEditor();
  if (!editor) {
    throw new Error("No active runbook editor found");
  }

  const targetBlock = editor.document.find((b: any) => b.id === afterBlockId);
  if (!targetBlock) {
    throw new Error(`Block not found: ${afterBlockId}`);
  }

  editor.insertBlocks(blocks, targetBlock, "after");
}

/**
 * Delete a block by ID
 */
export function deleteBlockById(blockId: string): void {
  const editor = getCurrentEditor();
  if (!editor) {
    throw new Error("No active runbook editor found");
  }

  console.log("[deleteBlockById] Looking for block:", blockId);
  console.log("[deleteBlockById] Available blocks:", editor.document.map((b: any) => b.id));

  const block = editor.document.find((b: any) => b.id === blockId);
  if (!block) {
    throw new Error(`Block not found: ${blockId}. Available: ${editor.document.map((b: any) => b.id).join(', ')}`);
  }

  console.log("[deleteBlockById] Deleting block:", block.type, block.id);
  editor.removeBlocks([block]);
  console.log("[deleteBlockById] Block deleted successfully");
}

/**
 * Delete multiple blocks by IDs
 */
export function deleteBlocksByIds(blockIds: string[]): void {
  const editor = getCurrentEditor();
  if (!editor) {
    throw new Error("No active runbook editor found");
  }

  const blocks = blockIds
    .map(id => editor.document.find((b: any) => b.id === id))
    .filter((block): block is any => {
      if (!block) {
        console.warn(`[deleteBlocksByIds] Block not found`);
        return false;
      }
      return true;
    });

  if (blocks.length > 0) {
    console.log("[deleteBlocksByIds] Deleting", blocks.length, "blocks");
    editor.removeBlocks(blocks);
  }
}

/**
 * Replace a block by ID with new blocks
 */
export function replaceBlockById(blockId: string, newBlocks: any[]): void {
  const editor = getCurrentEditor();
  if (!editor) {
    throw new Error("No active runbook editor found");
  }

  const blockIndex = editor.document.findIndex((b: any) => b.id === blockId);
  if (blockIndex === -1) {
    throw new Error(`Block not found: ${blockId}`);
  }

  const targetBlock = editor.document[blockIndex];
  editor.removeBlocks([targetBlock]);

  // Insert new blocks at the same position
  const referenceBlock = editor.document[Math.max(0, blockIndex - 1)];
  if (referenceBlock) {
    editor.insertBlocks(newBlocks, referenceBlock, "after");
  } else if (editor.document[0]) {
    editor.insertBlocks(newBlocks, editor.document[0], "before");
  }
}

/**
 * Update a block's properties by ID
 */
export function updateBlockById(blockId: string, updates: { props?: any; content?: any }): void {
  const editor = getCurrentEditor();
  if (!editor) {
    throw new Error("No active runbook editor found");
  }

  const block = editor.document.find((b: any) => b.id === blockId);
  if (!block) {
    throw new Error(`Block not found: ${blockId}`);
  }

  editor.updateBlock(block, updates);
}
