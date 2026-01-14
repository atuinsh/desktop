import { invoke, Channel } from "@tauri-apps/api/core";
import type { SessionEvent } from "../../rs-bindings/SessionEvent";
import { ModelSelection } from "@/rs-bindings/ModelSelection";
import { ChargeTarget } from "@/rs-bindings/ChargeTarget";

/**
 * Create or restore an AI session for a runbook.
 * Returns the session ID.
 *
 * @param restorePrevious - If true, attempts to restore the most recent session for this runbook.
 *                          If false, always creates a fresh session.
 */
export async function createSession(
  runbookId: string,
  blockTypes: string[],
  blockSummary: string,
  desktopUsername: string,
  chargeTarget: ChargeTarget,
  hubEndpoint: string,
  restorePrevious: boolean = true,
): Promise<string> {
  return await invoke<string>("ai_create_session", {
    runbookId,
    blockTypes,
    blockSummary,
    desktopUsername,
    chargeTarget,
    hubEndpoint,
    restorePrevious,
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
