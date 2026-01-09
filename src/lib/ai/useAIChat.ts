import { AIMessage } from "@/rs-bindings/AIMessage";
import { AIToolCall } from "@/rs-bindings/AIToolCall";
import { SessionEvent } from "@/rs-bindings/SessionEvent";
import {
  sendMessage as sendMessageCommand,
  sendToolResult,
  subscribeSession,
} from "./commands";
import { useCallback, useEffect, useMemo, useState } from "react";

export interface AIChatAPI {
  sessionId: string;
  messages: Array<AIMessage>;
  streamingContent: string | null;
  isStreaming: boolean;
  pendingToolCalls: Array<AIToolCall>;
  error: string | null;
  sendMessage: (message: string) => void;
  addToolOutput: (output: AIToolOutput) => void;
}

export interface AIToolOutput {
  toolCallId: string;
  success: boolean;
  result: string;
}

export default function useAIChat(sessionId: string): AIChatAPI {
  const [messages, setMessages] = useState<AIMessage[]>([]);
  const [streamingContent, setStreamingContent] = useState<string | null>(null);
  const [pendingToolCalls, setPendingToolCalls] = useState<AIToolCall[]>([]);
  const [error, setError] = useState<string | null>(null);

  const isStreaming = streamingContent !== null;

  useEffect(() => {
    if (!sessionId) return;

    const handleEvent = (event: SessionEvent) => {
      switch (event.type) {
        case "streamStarted":
          setStreamingContent("");
          setError(null);
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
          // Add assistant message with tool calls
          setStreamingContent((content) => {
            const parts: AIMessage["content"]["parts"] = [];
            if (content) {
              parts.push({ type: "text", data: content });
            }
            event.calls.forEach((call) => {
              parts.push({ type: "toolCall", data: call });
            });
            setMessages((prev) => [
              ...prev,
              { role: "assistant", content: { parts } },
            ]);
            return null;
          });
          break;

        case "error":
          setStreamingContent(null);
          setError(event.message);
          break;

        case "cancelled":
          setStreamingContent(null);
          break;
      }
    };

    subscribeSession(sessionId, handleEvent);
  }, [sessionId]);

  const sendMessage = useCallback(
    async (message: string) => {
      // Add user message
      const userMessage: AIMessage = {
        role: "user",
        content: { parts: [{ type: "text", data: message }] },
      };
      setMessages((prev) => [...prev, userMessage]);
      setError(null);

      await sendMessageCommand(sessionId, message);
    },
    [sessionId],
  );

  const addToolOutput = useCallback(
    async (output: AIToolOutput) => {
      setPendingToolCalls((prev) =>
        prev.filter((call) => call.id !== output.toolCallId),
      );

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

      await sendToolResult(
        sessionId,
        output.toolCallId,
        output.success,
        output.result,
      );
    },
    [sessionId],
  );

  const api = useMemo(
    () => ({
      sessionId,
      messages,
      streamingContent,
      isStreaming,
      pendingToolCalls,
      error,
      sendMessage,
      addToolOutput,
    }),
    [sessionId, messages, streamingContent, isStreaming, pendingToolCalls, error, sendMessage, addToolOutput],
  );

  return api;
}
