import { invoke } from "@tauri-apps/api/core";
import Logger from "@/lib/logger";

const logger = new Logger("Runtime");

export async function executeBlock(runbookId: string, blockId: string) {
  try {
    logger.info(`Executing block ${blockId} in runbook ${runbookId}`);
    const result = await invoke<string>("execute_block", { runbookId, blockId });
    logger.info(`Block ${blockId} in runbook ${runbookId} returned execution handle ID: ${result}`);
    return result;
  } catch (error) {
    logger.error(`Failed to execute block ${blockId} in runbook ${runbookId}`, error);
    throw error;
  }
}

export async function cancelExecution(executionId: string) {
  try {
    logger.info(`Cancelling execution ${executionId}`);
    await invoke<void>("cancel_block_execution", { executionId });
    logger.info(`Execution ${executionId} cancelled`);
  } catch (error) {
    logger.error(`Failed to cancel execution ${executionId}`, error);
    throw error;
  }
}
