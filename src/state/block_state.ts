import { invoke } from "@tauri-apps/api/core";

/**
 * Set a local state property for a specific block.
 * This data is persisted in SQLite but remains local to the user (not synced).
 *
 * @param runbookId The ID of the runbook containing the block
 * @param blockId The ID of the block
 * @param propertyName The name of the property to set
 * @param propertyValue The value to set
 * @returns A promise that resolves to true if the property was changed, false if it was already set to the same value
 */
export async function setBlockLocalState(
  runbookId: string,
  blockId: string,
  propertyName: string,
  propertyValue: string,
): Promise<boolean> {
  return invoke<boolean>("set_block_local_state", {
    runbookId,
    blockId,
    propertyName,
    propertyValue,
  });
}

/**
 * Get a local state property for a specific block.
 *
 * @param runbookId The ID of the runbook containing the block
 * @param blockId The ID of the block
 * @param propertyName The name of the property to get
 * @returns A promise that resolves to the property value, or null if it doesn't exist
 */
export async function getBlockLocalState(
  runbookId: string,
  blockId: string,
  propertyName: string,
): Promise<string | null> {
  return invoke<string | null>("get_block_local_state", {
    runbookId,
    blockId,
    propertyName,
  });
}

/**
 * Get all local state properties for a specific block.
 *
 * @param runbookId The ID of the runbook containing the block
 * @param blockId The ID of the block
 * @returns A promise that resolves to a map of property names to values
 */
export async function getBlockLocalStateAll(
  runbookId: string,
  blockId: string,
): Promise<Record<string, string>> {
  return invoke<Record<string, string>>("get_block_local_state_all", {
    runbookId,
    blockId,
  });
}

/**
 * Delete a local state property for a specific block.
 *
 * @param runbookId The ID of the runbook containing the block
 * @param blockId The ID of the block
 * @param propertyName The name of the property to delete
 * @returns A promise that resolves to true if the property was deleted, false if it didn't exist
 */
export async function deleteBlockLocalState(
  runbookId: string,
  blockId: string,
  propertyName: string,
): Promise<boolean> {
  return invoke<boolean>("delete_block_local_state", {
    runbookId,
    blockId,
    propertyName,
  });
}

/**
 * Delete all local state properties for a specific block.
 *
 * @param runbookId The ID of the runbook containing the block
 * @param blockId The ID of the block
 * @returns A promise that resolves to the number of properties deleted
 */
export async function deleteBlockLocalStateAll(
  runbookId: string,
  blockId: string,
): Promise<number> {
  return invoke<number>("delete_block_local_state_all", {
    runbookId,
    blockId,
  });
}

