import { invoke, Channel } from "@tauri-apps/api/core";
import type { SessionEvent } from "../../rs-bindings/SessionEvent";
import { ModelSelection } from "@/rs-bindings/ModelSelection";
import { ChargeTarget } from "@/rs-bindings/ChargeTarget";
import { BlockInfo } from "@/rs-bindings/BlockInfo";

/**
 * Create or restore an AI chat session for a runbook.
 * Returns the session ID.
 *
 * @param restorePrevious - If true, attempts to restore the most recent session for this runbook.
 *                          If false, always creates a fresh session.
 */
export async function createSession(
  runbookId: string,
  model: Option<ModelSelection>,
  blockInfos: Array<BlockInfo>,
  desktopUsername: string,
  chargeTarget: ChargeTarget,
  hubEndpoint: string,
  restorePrevious: boolean = true,
): Promise<string> {
  return await invoke<string>("ai_create_session", {
    runbookId,
    model: model.unwrapOr(undefined),
    blockInfos,
    desktopUsername,
    chargeTarget,
    hubEndpoint,
    restorePrevious,
  });
}

/**
 * Create an AI generator session for inline block generation.
 * Returns the session ID.
 */
export async function createGeneratorSession(
  runbookId: string,
  model: Option<ModelSelection>,
  blockInfos: Array<BlockInfo>,
  currentDocument: unknown,
  insertAfter: string,
  desktopUsername: string,
  chargeTarget: ChargeTarget,
  hubEndpoint: string,
): Promise<string> {
  return await invoke<string>("ai_create_generator_session", {
    runbookId,
    model: model.unwrapOr(undefined),
    blockInfos,
    currentDocument,
    insertAfter,
    desktopUsername,
    chargeTarget,
    hubEndpoint,
  });
}

/**
 * Subscribe to events from an AI session.
 * Returns a channel that will receive SessionEvent messages.
 */
export async function subscribeSession(
  sessionId: string,
  onEvent: (event: SessionEvent) => void,
): Promise<void> {
  const channel = new Channel<SessionEvent>();
  channel.onmessage = onEvent;
  await invoke("ai_subscribe_session", { sessionId, channel });
}

/**
 * Change the model of an AI session.
 */
export async function changeModel(sessionId: string, model: ModelSelection): Promise<void> {
  await invoke("ai_change_model", { sessionId, model });
}

/**
 * Change the charge target of an AI session.
 */
export async function changeChargeTarget(
  sessionId: string,
  chargeTarget: ChargeTarget,
): Promise<void> {
  await invoke("ai_change_charge_target", { sessionId, chargeTarget });
}

/**
 * Change the active user of an AI session.
 */
export async function changeUser(sessionId: string, user: string): Promise<void> {
  await invoke("ai_change_user", { sessionId, user });
}

/**
 * Send a user message to an AI session.
 */
export async function sendMessage(sessionId: string, message: string): Promise<void> {
  await invoke("ai_send_message", { sessionId, message });
}

/**
 * Send a tool result to an AI session.
 */
export async function sendToolResult(
  sessionId: string,
  toolCallId: string,
  success: boolean,
  result: string,
): Promise<void> {
  await invoke("ai_send_tool_result", { sessionId, toolCallId, success, result });
}

/**
 * Cancel the current operation in an AI session.
 */
export async function cancelSession(sessionId: string): Promise<void> {
  await invoke("ai_cancel_session", { sessionId });
}

/**
 * Destroy an AI session and clean up resources.
 */
export async function destroySession(sessionId: string): Promise<void> {
  await invoke("ai_destroy_session", { sessionId });
}

/**
 * Send an edit request to an InlineBlockGeneration session.
 * This continues the conversation after submit_blocks with the user's edit instructions.
 */
export async function sendEditRequest(
  sessionId: string,
  editPrompt: string,
  toolCallId: string,
): Promise<void> {
  await invoke("ai_send_edit_request", { sessionId, editPrompt, toolCallId });
}
