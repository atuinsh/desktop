import { useEffect, useRef, useState, useMemo } from "react";
import type { SessionInfo, SessionEvent, AIMessage, AIToolCall, State } from "../types";

interface SessionDetailProps {
  session: SessionInfo;
  events: { sessionId: string; event: SessionEvent }[];
}

type ViewMode = "conversation" | "events";

// Extract text content from a message
function getMessageText(message: AIMessage): string {
  const parts = message.content.parts || [];
  return parts
    .filter((p): p is { type: "text"; data: string } => p.type === "text")
    .map((p) => p.data)
    .join("");
}

// Extract tool calls from a message
function getMessageToolCalls(message: AIMessage): AIToolCall[] {
  const parts = message.content.parts || [];
  return parts
    .filter((p): p is { type: "toolCall"; data: AIToolCall } => p.type === "toolCall")
    .map((p) => p.data);
}

// Extract tool responses from a message
function getMessageToolResponses(message: AIMessage): { callId: string; result: string }[] {
  const parts = message.content.parts || [];
  return parts
    .filter((p): p is { type: "toolResponse"; data: { callId: string; result: string } } => p.type === "toolResponse")
    .map((p) => p.data);
}

function MessageView({ message, index }: { message: AIMessage; index: number }) {
  const [expanded, setExpanded] = useState(false);
  const text = getMessageText(message);
  const toolCalls = getMessageToolCalls(message);
  const toolResponses = getMessageToolResponses(message);

  const roleColors: Record<string, string> = {
    system: "bg-purple-500/20 border-purple-500/50",
    user: "bg-blue-500/20 border-blue-500/50",
    assistant: "bg-green-500/20 border-green-500/50",
    tool: "bg-orange-500/20 border-orange-500/50",
  };

  const roleLabels: Record<string, string> = {
    system: "System",
    user: "User",
    assistant: "Assistant",
    tool: "Tool",
  };

  return (
    <div className={`p-3 border-l-4 ${roleColors[message.role] || "bg-default-100 border-default-300"} mb-2 rounded-r`}>
      <div className="flex items-center gap-2 mb-2">
        <span className="text-xs font-semibold uppercase tracking-wide">
          {roleLabels[message.role] || message.role}
        </span>
        <span className="text-xs text-default-400">#{index}</span>
        <button
          onClick={() => setExpanded(!expanded)}
          className="ml-auto text-xs text-default-400 hover:text-default-600"
        >
          {expanded ? "Collapse" : "Expand JSON"}
        </button>
      </div>

      {text && (
        <div className="text-sm whitespace-pre-wrap break-words mb-2">
          {message.role === "system" ? (
            <details>
              <summary className="cursor-pointer text-default-500">
                System prompt ({text.length} chars)
              </summary>
              <pre className="mt-2 text-xs overflow-x-auto bg-default-100 dark:bg-default-800 p-2 rounded">
                {text}
              </pre>
            </details>
          ) : (
            text
          )}
        </div>
      )}

      {toolCalls.length > 0 && (
        <div className="mt-2 space-y-2">
          {toolCalls.map((call, i) => (
            <div key={i} className="bg-default-100 dark:bg-default-800 p-2 rounded text-sm">
              <div className="font-medium text-purple-500">{call.name}</div>
              <pre className="text-xs mt-1 overflow-x-auto whitespace-pre-wrap">
                {JSON.stringify(call.args, null, 2)}
              </pre>
            </div>
          ))}
        </div>
      )}

      {toolResponses.length > 0 && (
        <div className="mt-2 space-y-2">
          {toolResponses.map((resp, i) => (
            <div key={i} className="bg-default-100 dark:bg-default-800 p-2 rounded text-sm">
              <div className="text-xs text-default-400">Tool Response: {resp.callId}</div>
              <pre className="text-xs mt-1 overflow-x-auto whitespace-pre-wrap max-h-40 overflow-y-auto">
                {resp.result}
              </pre>
            </div>
          ))}
        </div>
      )}

      {expanded && (
        <pre className="mt-2 text-xs overflow-x-auto bg-default-50 dark:bg-default-900 p-2 rounded border border-divider">
          {JSON.stringify(message, null, 2)}
        </pre>
      )}
    </div>
  );
}

function StreamingIndicator({ content, state }: { content: string; state: State | null }) {
  if (!state || state.type === "idle") return null;

  return (
    <div className="p-3 border-l-4 bg-green-500/10 border-green-500/50 mb-2 rounded-r animate-pulse">
      <div className="flex items-center gap-2 mb-2">
        <span className="text-xs font-semibold uppercase tracking-wide">Assistant</span>
        <span className="text-xs text-green-500">
          {state.type === "sending" && "Sending..."}
          {state.type === "streaming" && "Streaming..."}
          {state.type === "pendingTools" && "Awaiting tool results..."}
        </span>
      </div>
      {content && (
        <div className="text-sm whitespace-pre-wrap break-words">{content}</div>
      )}
    </div>
  );
}

function ConversationView({ events }: { events: { sessionId: string; event: SessionEvent }[] }) {
  const scrollRef = useRef<HTMLDivElement>(null);

  // Build conversation state from events
  const { messages, streamingContent, currentState, pendingToolCalls } = useMemo(() => {
    let messages: AIMessage[] = [];
    let streamingContent = "";
    let currentState: State | null = null;
    let pendingToolCalls: AIToolCall[] = [];

    for (const { event } of events) {
      switch (event.type) {
        case "history":
          messages = event.messages;
          pendingToolCalls = event.pendingToolCalls;
          break;
        case "stateChanged":
          currentState = event.state;
          if (event.state.type === "idle") {
            streamingContent = "";
          }
          break;
        case "streamStarted":
          streamingContent = "";
          break;
        case "chunk":
          streamingContent += event.content;
          break;
        case "responseComplete":
          streamingContent = "";
          break;
        case "toolsRequested":
          pendingToolCalls = event.calls;
          break;
      }
    }

    return { messages, streamingContent, currentState, pendingToolCalls };
  }, [events]);

  // Auto-scroll to bottom
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages.length, streamingContent]);

  return (
    <div ref={scrollRef} className="flex-1 overflow-y-auto p-4">
      {messages.length === 0 && !streamingContent ? (
        <div className="text-sm text-default-400">No messages yet</div>
      ) : (
        <>
          {messages.map((msg, i) => (
            <MessageView key={i} message={msg} index={i} />
          ))}
          <StreamingIndicator content={streamingContent} state={currentState} />
          {pendingToolCalls.length > 0 && currentState?.type === "pendingTools" && (
            <div className="p-3 border-l-4 bg-purple-500/20 border-purple-500/50 mb-2 rounded-r">
              <div className="text-xs font-semibold uppercase tracking-wide mb-2">
                Pending Tool Calls
              </div>
              {pendingToolCalls.map((call, i) => (
                <div key={i} className="bg-default-100 dark:bg-default-800 p-2 rounded text-sm mb-1">
                  <div className="font-medium text-purple-500">{call.name}</div>
                  <pre className="text-xs mt-1 overflow-x-auto whitespace-pre-wrap">
                    {JSON.stringify(call.args, null, 2)}
                  </pre>
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}

function EventItem({ event, index }: { event: SessionEvent; index: number }) {
  const [expanded, setExpanded] = useState(false);

  const getEventLabel = (e: SessionEvent): string => {
    switch (e.type) {
      case "stateChanged":
        return `State: ${e.state.type}`;
      case "streamStarted":
        return "Stream Started";
      case "chunk":
        return `Chunk (${e.content.length} chars)`;
      case "responseComplete":
        return "Response Complete";
      case "toolsRequested":
        return `Tools Requested (${e.calls.length})`;
      case "error":
        return `Error: ${e.message}`;
      case "cancelled":
        return "Cancelled";
      case "history":
        return `History (${e.messages.length} messages)`;
      default:
        return "Unknown Event";
    }
  };

  const getEventColor = (e: SessionEvent): string => {
    switch (e.type) {
      case "stateChanged":
        return "text-blue-500";
      case "streamStarted":
        return "text-green-500";
      case "chunk":
        return "text-default-400";
      case "responseComplete":
        return "text-green-600";
      case "toolsRequested":
        return "text-purple-500";
      case "error":
        return "text-red-500";
      case "cancelled":
        return "text-yellow-500";
      case "history":
        return "text-cyan-500";
      default:
        return "text-default-500";
    }
  };

  return (
    <div className="border-b border-divider last:border-b-0">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full text-left p-2 hover:bg-default-100 dark:hover:bg-default-800 transition-colors flex items-center gap-2"
      >
        <span className="text-xs text-default-400 font-mono w-8">{index}</span>
        <span className={`text-sm font-medium ${getEventColor(event)}`}>
          {getEventLabel(event)}
        </span>
        <span className="ml-auto text-xs text-default-400">
          {expanded ? "[-]" : "[+]"}
        </span>
      </button>
      {expanded && (
        <div className="p-2 bg-default-50 dark:bg-default-900 border-t border-divider">
          <pre className="text-xs overflow-x-auto whitespace-pre-wrap break-words font-mono">
            {JSON.stringify(event, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

function EventStreamView({ events }: { events: { sessionId: string; event: SessionEvent }[] }) {
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [events.length]);

  return (
    <div ref={scrollRef} className="flex-1 overflow-y-auto">
      {events.length === 0 ? (
        <div className="p-4 text-sm text-default-400">No events yet</div>
      ) : (
        events.map((e, i) => <EventItem key={i} event={e.event} index={i} />)
      )}
    </div>
  );
}

export default function SessionDetail({ session, events }: SessionDetailProps) {
  const [viewMode, setViewMode] = useState<ViewMode>("conversation");

  return (
    <div className="h-full flex flex-col">
      {/* Session info header */}
      <div className="flex-none p-4 border-b border-divider bg-default-50 dark:bg-default-900">
        <h2 className="text-base font-semibold">
          {session.kind === "assistantChat" ? "Assistant Chat" : "Inline Generation"}
        </h2>
        <div className="mt-2 text-sm text-default-500 space-y-1">
          <div>
            <span className="font-medium">Session ID:</span>{" "}
            <span className="font-mono text-xs">{session.id}</span>
          </div>
          <div>
            <span className="font-medium">Runbook ID:</span>{" "}
            <span className="font-mono text-xs">{session.runbookId}</span>
          </div>
        </div>
      </div>

      {/* View mode tabs */}
      <div className="flex-none flex border-b border-divider">
        <button
          onClick={() => setViewMode("conversation")}
          className={`flex-1 px-4 py-2 text-sm font-medium transition-colors ${
            viewMode === "conversation"
              ? "bg-primary-100 dark:bg-primary-900/30 text-primary-600 dark:text-primary-400 border-b-2 border-primary-500"
              : "hover:bg-default-100 dark:hover:bg-default-800 text-default-600"
          }`}
        >
          Conversation
        </button>
        <button
          onClick={() => setViewMode("events")}
          className={`flex-1 px-4 py-2 text-sm font-medium transition-colors ${
            viewMode === "events"
              ? "bg-primary-100 dark:bg-primary-900/30 text-primary-600 dark:text-primary-400 border-b-2 border-primary-500"
              : "hover:bg-default-100 dark:hover:bg-default-800 text-default-600"
          }`}
        >
          Events ({events.length})
        </button>
      </div>

      {/* View content */}
      {viewMode === "conversation" ? (
        <ConversationView events={events} />
      ) : (
        <EventStreamView events={events} />
      )}
    </div>
  );
}
