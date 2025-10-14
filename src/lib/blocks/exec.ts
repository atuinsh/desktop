// Helpers to make writing blocks easier
// Called "exec", as these are all helpers for executing commands

import { templateString } from "@/state/templates";
import { KVStore } from "@/state/kv";

/**
 * Find the first parent block of a specific type or types
 * @param editor The editor instance
 * @param id The ID of the current block
 * @param types A single type string or an array of type strings to look for
 * @returns The first parent block of the specified type(s), or null if none found
 */
export const findFirstParentOfType = (editor: any, id: string, types: string | string[]): any => {
  // TODO: the types for blocknote aren't working. Now I'm doing this sort of shit,
  // really need to fix that.
  const document = editor.document;
  var lastOfType = null;
  const typeArray = Array.isArray(types) ? types : [types];

  // Iterate through ALL of the blocks.
  for (let i = 0; i < document.length; i++) {
    if (document[i].id == id) return lastOfType;

    if (typeArray.includes(document[i].type)) lastOfType = document[i];
  }

  return lastOfType;
};

export const findAllParentsOfType = (editor: any, id: string, types: string | string[]): any[] => {
  const document = editor.document;
  const typeArray = Array.isArray(types) ? types : [types];
  let blocks: any[] = [];

  // Iterate through ALL of the blocks.
  for (let i = 0; i < document.length; i++) {
    if (document[i].id == id) return blocks;

    if (typeArray.includes(document[i].type)) blocks.push(document[i]);
  }

  return blocks;
};

/**
 * Calculate the current working directory for a block by finding its parent directory block
 * @param editor The editor instance
 * @param blockId The ID of the current block
 * @param runbookId The current runbook ID (for templating)
 * @returns Promise<string> The templated directory path, or "~" if no directory parent found
 */
export const getCurrentDirectory = async (editor: any, blockId: string, runbookId: string | null): Promise<string> => {

  const resolvePath = async (block: any) => {
    if (block.type === "directory") {
      return (await templateString(blockId, block.props.path || "~", editor.document, runbookId)).trim();
    } else if (block.type === "local-directory") {
      const kvStore = await KVStore.open_default();
      const storedPath = await kvStore.get<string>(`block.${block.id}.path`);

      if (storedPath) {
        return (await templateString(blockId, storedPath, editor.document, runbookId)).trim();
      }
    }
    return "~";
  };

  // Check for both directory and local-directory blocks
  const directoryBlocks = findAllParentsOfType(editor, blockId, ["directory", "local-directory"]);

  if (directoryBlocks.length == 0) {
    return "~";
  }

  const lastParent= directoryBlocks[directoryBlocks.length - 1];

  // First, handle the case where the LAST block is an absolute path. IE, it starts with a /
  const path = await resolvePath(lastParent);

  if (path.startsWith("/") || path.startsWith("~")) {
    return path;
  }

  let paths = [path];
  let foundAbsolute = false;

  // If the last parent is a relative path, then we need to keep iterating up the list until we find an absolute path
  // All blocks we find along the way should be included
  for (let i = directoryBlocks.length - 2; i >= 0; i--) {
    const block = directoryBlocks[i];
    const path = await resolvePath(block);
    paths = [path, ...paths];

    if (path.startsWith("/") || path.startsWith("~")) {
      foundAbsolute = true;
      break;
    }
  }

  // If no absolute path was found, prepend ~ to make it relative to home
  if (!foundAbsolute) {
    paths = ["~", ...paths];
  }

  return paths.join("/");
};