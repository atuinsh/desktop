import { invoke, Channel } from "@tauri-apps/api/core";
import type { SessionEvent } from "../../rs-bindings/SessionEvent";

/**
 * Create a new AI session.
 * Returns the session ID.
 */
export async function createSession(): Promise<string> {
  return await invoke<string>("ai_create_session");
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
