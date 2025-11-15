import { invoke } from "@tauri-apps/api/core";
import Logger from "@/lib/logger";

const logger = new Logger("Runtime");

export async function executeBlock(runbookId: string, blockId: string): Promise<string | null> {
  try {
    logger.info(`Executing block ${blockId} in runbook ${runbookId}`);
    const result = await invoke<string | null>("execute_block", { runbookId, blockId });
    logger.info(`Block ${blockId} in runbook ${runbookId} returned execution handle ID: ${result}`);
    return result;
  } catch (error) {
    logger.warn(`Failed to execute block ${blockId} in runbook ${runbookId}`, error);
    throw error;
  }
}

export async function cancelExecution(executionId: string) {
  try {
    logger.info(`Cancelling execution ${executionId}`);
    await invoke<void>("cancel_block_execution", { executionId });
    logger.info(`Execution ${executionId} cancelled`);
  } catch (error) {
    logger.warn(`Failed to cancel execution ${executionId}`, error);
    throw error;
  }
}

export async function resetRunbookState(runbookId: string) {
  try {
    logger.info(`Resetting runbook state for ${runbookId}`);
    await invoke<void>("reset_runbook_state", { documentId: runbookId });
    logger.info(`Runbook state for ${runbookId} reset`);
  } catch (error) {
    logger.warn(`Failed to reset runbook state for ${runbookId}`, error);
    throw error;
  }
}
