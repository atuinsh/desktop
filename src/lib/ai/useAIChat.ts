import { AIMessage } from "@/rs-bindings/AIMessage";
import { AIToolCall } from "@/rs-bindings/AIToolCall";
import { SessionEvent } from "@/rs-bindings/SessionEvent";
import {
  sendMessage as sendMessageCommand,
  sendToolResult,
  subscribeSession,
  cancelSession,
  changeModel as changeModelCommand,
} from "./commands";
import { useCallback, useEffect, useMemo, useState } from "react";
import { State } from "@/rs-bindings/State";
import { ModelSelection } from "@/rs-bindings/ModelSelection";

export interface AIChatAPI {
  sessionId: string;
  messages: Array<AIMessage>;
  queuedMessages: Array<AIMessage>;
  streamingContent: string | null;
  isStreaming: boolean;
  pendingToolCalls: Array<AIToolCall>;
  error: string | null;
  state: State["type"];
  sendMessage: (message: string) => void;
  changeModel: (model: ModelSelection) => Promise<void>;
  addToolOutput: (output: AIToolOutput) => void;
  cancel: () => void;
}

export interface AIToolOutput {
  toolCallId: string;
  success: boolean;
  result: string;
}

export default function useAIChat(sessionId: string): AIChatAPI {
  const [state, setState] = useState<State["type"]>("idle");
  const [messages, setMessages] = useState<AIMessage[]>([]);
  const [queuedMessages, setQueuedMessages] = useState<AIMessage[]>([]);
  const [streamingContent, setStreamingContent] = useState<string | null>(null);
  const [pendingToolCalls, setPendingToolCalls] = useState<AIToolCall[]>([]);
  const [error, setError] = useState<string | null>(null);

  const isStreaming = streamingContent !== null;
  const isIdle = state === "idle";

  // Reset all state when sessionId changes - ensures clean slate for new/restored sessions
  useEffect(() => {
    setState("idle");
    setMessages([]);
    setQueuedMessages([]);
    setStreamingContent(null);
    setPendingToolCalls([]);
    setError(null);
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId) return;

    const handleEvent = (event: SessionEvent) => {
      switch (event.type) {
        case "stateChanged":
          setState(event.state.type);
          break;
        case "streamStarted":
          setStreamingContent("");
          setError(null);
          // Drain queued messages to main messages list - they've been sent to backend
          setQueuedMessages((queued) => {
            if (queued.length > 0) {
              setMessages((prev) => [...prev, ...queued]);
            }
            return [];
          });
          break;

        case "chunk":
          setStreamingContent((prev) => (prev ?? "") + event.content);
          break;

        case "responseComplete":
          // Move streaming content to a completed message
          setStreamingContent((content) => {
            if (content) {
              setMessages((prev) => [
                ...prev,
                {
                  role: "assistant",
                  content: { parts: [{ type: "text", data: content }] },
                },
              ]);
            }
            return null;
          });
          break;

        case "toolsRequested":
          setPendingToolCalls(event.calls);
          // Add assistant message with tool calls (only if not already in messages from history)
          setStreamingContent((content) => {
            setMessages((prev) => {
              // Check if these tool calls already exist in messages (e.g., from history restore)
              const existingToolCallIds = new Set(
                prev.flatMap((msg) =>
                  msg.content.parts
                    .filter((part): part is { type: "toolCall"; data: AIToolCall } => part.type === "toolCall")
                    .map((part) => part.data.id),
                ),
              );
              const newCalls = event.calls.filter((call) => !existingToolCallIds.has(call.id));

              // If all tool calls already exist, don't add a duplicate message
              if (newCalls.length === 0) {
                return prev;
              }

              const parts: AIMessage["content"]["parts"] = [];
              if (content) {
                parts.push({ type: "text", data: content });
              }
              newCalls.forEach((call) => {
                parts.push({ type: "toolCall", data: call });
              });
              return [...prev, { role: "assistant", content: { parts } }];
            });
            return null;
          });
          break;

        case "error":
          setStreamingContent(null);
          setError(event.message);
          break;

        case "cancelled":
          setStreamingContent(null);
          // Add cancelled tool response messages for any pending tool calls
          setPendingToolCalls((prev) => {
            if (prev.length > 0) {
              const cancelledMessages: AIMessage[] = prev.map((call) => ({
                role: "tool" as const,
                content: {
                  parts: [
                    {
                      type: "toolResponse" as const,
                      data: {
                        callId: call.id,
                        result: "User cancelled this operation",
                      },
                    },
                  ],
                },
              }));
              setMessages((msgs) => [...msgs, ...cancelledMessages]);
            }
            return [];
          });
          break;

        case "history":
          // Set messages and pending tool calls from session history
          console.log("[useAIChat] Received history event:", {
            messageCount: event.messages.length,
            pendingToolCalls: event.pendingToolCalls,
          });
          setMessages(event.messages);
          setPendingToolCalls(event.pendingToolCalls ?? []);
          break;
      }
    };

    subscribeSession(sessionId, handleEvent);
  }, [sessionId]);

  const sendMessage = useCallback(
    async (message: string) => {
      const userMessage: AIMessage = {
        role: "user",
        content: { parts: [{ type: "text", data: message }] },
      };

      // If busy (streaming or waiting for tools), queue the message for display
      // The backend will also queue it and process when ready
      if (!isIdle) {
        setQueuedMessages((prev) => [...prev, userMessage]);
      } else {
        setMessages((prev) => [...prev, userMessage]);
      }
      setError(null);

      await sendMessageCommand(sessionId, message);
    },
    [sessionId, isIdle],
  );

  const changeModel = useCallback(
    async (model: ModelSelection) => {
      await changeModelCommand(sessionId, model);
    },
    [sessionId],
  );

  const addToolOutput = useCallback(
    async (output: AIToolOutput) => {
      setPendingToolCalls((prev) => prev.filter((call) => call.id !== output.toolCallId));

      // Add tool response message
      const toolMessage: AIMessage = {
        role: "tool",
        content: {
          parts: [
            {
              type: "toolResponse",
              data: {
                callId: output.toolCallId,
                result: output.result,
              },
            },
          ],
        },
      };
      setMessages((prev) => [...prev, toolMessage]);

      await sendToolResult(sessionId, output.toolCallId, output.success, output.result);
    },
    [sessionId],
  );

  const cancel = useCallback(async () => {
    await cancelSession(sessionId);
  }, [sessionId]);

  const api: AIChatAPI = useMemo(
    () => ({
      sessionId,
      state,
      messages,
      queuedMessages,
      streamingContent,
      isStreaming,
      isIdle,
      pendingToolCalls,
      error,
      sendMessage,
      changeModel,
      addToolOutput,
      cancel,
    }),
    [
      sessionId,
      state,
      messages,
      queuedMessages,
      streamingContent,
      isStreaming,
      pendingToolCalls,
      error,
      sendMessage,
      changeModel,
      addToolOutput,
      cancel,
    ],
  );

  return api;
}
