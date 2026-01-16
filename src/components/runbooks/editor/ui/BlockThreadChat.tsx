import { useState, useRef, useEffect, useCallback, memo, Component, ReactNode } from "react";
import { Streamdown } from "streamdown";
import {
  Button,
  Textarea,
  Spinner,
  ScrollShadow,
} from "@heroui/react";
import {
  SendIcon,
  XIcon,
  CheckIcon,
  BotIcon,
  UserIcon,
  WrenchIcon,
  ChevronRightIcon,
  ChevronDownIcon,
  StopCircleIcon,
} from "lucide-react";
import { BlockNoteEditor } from "@blocknote/core";
import { cn } from "@/lib/utils";
import { AIMessage } from "@/rs-bindings/AIMessage";
import { AIToolCall } from "@/rs-bindings/AIToolCall";
import useAIChat from "@/lib/ai/useAIChat";
import { createSession, destroySession } from "@/lib/ai/commands";
import AIBlockRegistry from "@/lib/ai/block_registry";
import { useStore } from "@/state/store";
import AtuinEnv from "@/atuin_env";

// Tools scoped for block-local editing
const BLOCK_TOOL_NAMES = ["get_block_docs", "update_block"];
const AUTO_APPROVE_TOOLS = new Set(["get_block_docs"]);

// Error boundary for Streamdown
class MarkdownErrorBoundary extends Component<
  { children: ReactNode; fallback: ReactNode },
  { hasError: boolean }
> {
  constructor(props: { children: ReactNode; fallback: ReactNode }) {
    super(props);
    this.state = { hasError: false };
  }

  static getDerivedStateFromError() {
    return { hasError: true };
  }

  render() {
    if (this.state.hasError) {
      return this.props.fallback;
    }
    return this.props.children;
  }
}

const MarkdownContent = memo(function MarkdownContent({
  content,
  isStreaming = false,
}: {
  content: string;
  isStreaming?: boolean;
}) {
  return (
    <MarkdownErrorBoundary fallback={<span>{content}</span>}>
      <Streamdown isAnimating={isStreaming}>{content}</Streamdown>
    </MarkdownErrorBoundary>
  );
});

interface BlockThreadChatProps {
  blockId: string;
  blockType: string;
  editor: BlockNoteEditor | null;
  runbookId: string;
  onClose: () => void;
}

function formatToolParams(params: any): string {
  if (!params) return "";
  try {
    return JSON.stringify(params, null, 2);
  } catch {
    return String(params);
  }
}

function ToolParamsDisplay({ params }: { params: any }) {
  const [expanded, setExpanded] = useState(false);

  if (!params) return null;

  const formatted = formatToolParams(params);

  return (
    <div className="mt-1">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-1 text-xs text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
      >
        {expanded ? (
          <ChevronDownIcon className="h-3 w-3" />
        ) : (
          <ChevronRightIcon className="h-3 w-3" />
        )}
        {expanded ? "Hide" : "Show"}
      </button>
      {expanded && (
        <pre className="mt-1 p-1.5 bg-gray-100 dark:bg-gray-800 rounded text-xs overflow-x-auto max-h-24 overflow-y-auto">
          {formatted}
        </pre>
      )}
    </div>
  );
}

function CompactToolCallUI({
  toolCall,
  isPending,
  isRejected,
  onApprove,
  onDeny,
}: {
  toolCall: AIToolCall;
  isPending: boolean;
  isRejected: boolean;
  onApprove: (toolCall: AIToolCall) => void;
  onDeny: (toolCall: AIToolCall) => void;
}) {
  const isApproved = !isPending && !isRejected;

  return (
    <div className="mt-1.5 p-1.5 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-800 rounded text-xs">
      <div className="flex items-center gap-1.5">
        <WrenchIcon className="h-3 w-3 text-amber-600 dark:text-amber-400" />
        <span className="font-medium text-amber-700 dark:text-amber-300 truncate">
          {toolCall.name}
        </span>
        {isPending && <Spinner size="sm" className="h-3 w-3" />}
        {isApproved && <CheckIcon className="h-3 w-3 text-green-500" />}
        {isRejected && <XIcon className="h-3 w-3 text-red-500" />}
      </div>
      <ToolParamsDisplay params={toolCall.args} />
      {isPending && (
        <div className="flex gap-1.5 mt-1.5">
          <Button
            size="sm"
            variant="flat"
            onPress={() => onDeny(toolCall)}
            className="text-xs h-6 min-w-0 px-2 text-red-600"
          >
            Deny
          </Button>
          <Button
            size="sm"
            color="success"
            onPress={() => onApprove(toolCall)}
            className="text-xs h-6 min-w-0 px-2"
          >
            Allow
          </Button>
        </div>
      )}
    </div>
  );
}

function getMessageText(message: AIMessage): string {
  return message.content.parts
    .filter((part): part is { type: "text"; data: string } => part.type === "text")
    .map((part) => part.data)
    .join("");
}

function getToolCalls(message: AIMessage): AIToolCall[] {
  return message.content.parts
    .filter((part): part is { type: "toolCall"; data: AIToolCall } => part.type === "toolCall")
    .map((part) => part.data);
}

function CompactMessage({
  message,
  pendingToolCalls,
  rejectedToolCalls,
  onApprove,
  onDeny,
}: {
  message: AIMessage;
  pendingToolCalls: AIToolCall[];
  rejectedToolCalls: string[];
  onApprove: (toolCall: AIToolCall) => void;
  onDeny: (toolCall: AIToolCall) => void;
}) {
  const isUser = message.role === "user";
  const isAssistant = message.role === "assistant";
  const isTool = message.role === "tool";

  const text = getMessageText(message);
  const toolCalls = getToolCalls(message);

  // Hide tool response messages (they're shown inline with tool calls)
  if (isTool) return null;

  return (
    <div
      className={cn(
        "flex gap-1.5 py-1.5 px-2 rounded text-xs",
        isUser && "bg-blue-50 dark:bg-blue-950/30",
        isAssistant && "bg-gray-50 dark:bg-gray-800/50"
      )}
    >
      <div className="flex-shrink-0 mt-0.5">
        {isUser && <UserIcon className="h-3 w-3 text-blue-600 dark:text-blue-400" />}
        {isAssistant && <BotIcon className="h-3 w-3 text-purple-600 dark:text-purple-400" />}
      </div>

      <div className="flex-1 min-w-0">
        {text && (
          <div className="whitespace-pre-wrap break-words text-gray-800 dark:text-gray-200">
            <MarkdownContent content={text} />
          </div>
        )}

        {toolCalls.map((toolCall) => (
          <CompactToolCallUI
            key={toolCall.id}
            toolCall={toolCall}
            isPending={pendingToolCalls.some((tc) => tc.id === toolCall.id)}
            isRejected={rejectedToolCalls.includes(toolCall.id)}
            onApprove={onApprove}
            onDeny={onDeny}
          />
        ))}
      </div>
    </div>
  );
}

// Tool execution for block-scoped operations
async function executeUpdateBlock(
  editor: BlockNoteEditor,
  targetBlockId: string,
  params: { block_id: string; props?: any; content?: any }
): Promise<any> {
  // Only allow updating the target block
  if (params.block_id !== targetBlockId) {
    throw new Error(`Can only update block ${targetBlockId}, not ${params.block_id}`);
  }

  const { props, content } = params;
  const updates: any = {};
  if (props) updates.props = props;
  if (content) updates.content = content;

  editor.updateBlock(targetBlockId, updates);
  return { success: true };
}

async function executeGetBlockDocs(params: { block_types: string[] }): Promise<string> {
  return params.block_types.reduce((acc, blockType) => {
    acc += AIBlockRegistry.getInstance().getBlockDocs(blockType);
    return acc;
  }, "");
}

export default function BlockThreadChat({
  blockId,
  blockType,
  editor,
  runbookId,
  onClose,
}: BlockThreadChatProps) {
  const [inputValue, setInputValue] = useState("");
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [isCreatingSession, setIsCreatingSession] = useState(true);
  const [rejectedToolCalls, setRejectedToolCalls] = useState<string[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const autoApprovedRef = useRef<Set<string>>(new Set());
  const user = useStore((state) => state.user);

  // Get the block content for initial context
  const getBlockContext = useCallback(() => {
    if (!editor) return null;
    const block = editor.getBlock(blockId);
    if (!block) return null;
    return {
      id: block.id,
      type: block.type,
      props: (block as any).props,
      content: (block as any).content,
    };
  }, [editor, blockId]);

  // Create session on mount
  useEffect(() => {
    const blockRegistry = AIBlockRegistry.getInstance();
    let mounted = true;

    createSession(
      `${runbookId}:block:${blockId}`, // Unique session ID for this block
      blockRegistry.getBlockTypes(),
      blockRegistry.getBlockSummary(),
      user.username,
      "user", // charge target
      AtuinEnv.url("/api/ai/proxy/"),
      false // don't restore - always fresh for block threads
    )
      .then((id) => {
        if (mounted) {
          setSessionId(id);
          setIsCreatingSession(false);
        }
      })
      .catch((err) => {
        console.error("Failed to create block AI session:", err);
        if (mounted) {
          setIsCreatingSession(false);
        }
      });

    return () => {
      mounted = false;
    };
  }, [runbookId, blockId, user.username]);

  // Cleanup session on unmount
  useEffect(() => {
    return () => {
      if (sessionId) {
        destroySession(sessionId).catch(console.error);
      }
    };
  }, [sessionId]);

  const chat = useAIChat(sessionId || "");
  const {
    state,
    messages,
    streamingContent,
    pendingToolCalls,
    error,
    sendMessage,
    addToolOutput,
    cancel,
  } = chat;

  const canCancel = state !== "idle";

  // Auto-scroll to bottom
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, streamingContent]);

  // Focus input when ready
  useEffect(() => {
    if (sessionId && !isCreatingSession) {
      setTimeout(() => textareaRef.current?.focus(), 100);
    }
  }, [sessionId, isCreatingSession]);

  const handleSend = useCallback(() => {
    if (!inputValue.trim() || state !== "idle" || !sessionId) return;

    // Include block context in first message
    const blockContext = getBlockContext();
    let message = inputValue.trim();

    if (messages.length === 0 && blockContext) {
      // Prepend context about the target block
      message = `[Context: You are helping with a specific "${blockType}" block (ID: ${blockId}). Here is the current block state:\n${JSON.stringify(blockContext, null, 2)}\n\nYou can ONLY edit this specific block using update_block with block_id="${blockId}".]\n\nUser request: ${message}`;
    }

    sendMessage(message);
    setInputValue("");
  }, [inputValue, state, sessionId, messages.length, blockType, blockId, getBlockContext, sendMessage]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    } else if (e.key === "Escape") {
      onClose();
    }
  };

  const handleApprove = useCallback(
    async (toolCall: AIToolCall) => {
      if (!editor) return;

      try {
        let result: any;
        const args = toolCall.args as Record<string, any> | null;
        if (!args) {
          throw new Error("Tool call missing arguments");
        }

        switch (toolCall.name) {
          case "get_block_docs":
            result = await executeGetBlockDocs(args as { block_types: string[] });
            break;
          case "update_block":
            result = await executeUpdateBlock(
              editor,
              blockId,
              args as { block_id: string; props?: any; content?: any }
            );
            break;
          default:
            throw new Error(`Unknown or disallowed tool: ${toolCall.name}`);
        }
        addToolOutput({
          toolCallId: toolCall.id,
          success: true,
          result: JSON.stringify(result),
        });
      } catch (error) {
        const message = error instanceof Error ? error.message : "Unknown error";
        addToolOutput({
          toolCallId: toolCall.id,
          success: false,
          result: message,
        });
      }
    },
    [editor, blockId, addToolOutput]
  );

  const handleDeny = useCallback(
    (toolCall: AIToolCall) => {
      setRejectedToolCalls((prev) => [...prev, toolCall.id]);
      addToolOutput({
        toolCallId: toolCall.id,
        success: false,
        result: "User denied tool execution",
      });
    },
    [addToolOutput]
  );

  // Auto-approve safe tools
  useEffect(() => {
    if (!editor) return;

    for (const toolCall of pendingToolCalls) {
      if (!BLOCK_TOOL_NAMES.includes(toolCall.name)) {
        handleDeny(toolCall);
        continue;
      }

      if (
        AUTO_APPROVE_TOOLS.has(toolCall.name) &&
        !autoApprovedRef.current.has(toolCall.id)
      ) {
        autoApprovedRef.current.add(toolCall.id);
        handleApprove(toolCall);
      }
    }
  }, [pendingToolCalls, editor, handleApprove, handleDeny]);

  const isConnected = sessionId !== null && !isCreatingSession;

  return (
    <div className="flex flex-col bg-white dark:bg-gray-900 rounded-lg border border-purple-200 dark:border-purple-800 shadow-lg max-h-96 overflow-hidden">
      {/* Header */}
      <div className="flex-shrink-0 flex items-center justify-between px-2 py-1.5 border-b border-gray-200 dark:border-gray-700 bg-purple-50 dark:bg-purple-950/30">
        <div className="flex items-center gap-1.5 min-w-0">
          <div
            className={cn("h-1.5 w-1.5 rounded-full flex-shrink-0", isConnected ? "bg-green-500" : "bg-yellow-500")}
          />
          <span className="text-xs font-medium text-purple-900 dark:text-purple-100 truncate">
            {blockType}
          </span>
        </div>
        <Button
          size="sm"
          isIconOnly
          variant="light"
          onPress={onClose}
          className="h-5 w-5 min-w-0"
        >
          <XIcon className="h-3 w-3" />
        </Button>
      </div>

      {/* Messages */}
      <ScrollShadow className="flex-1 min-h-0 overflow-y-auto p-1.5 space-y-1" ref={scrollRef}>
        {isCreatingSession && (
          <div className="flex items-center justify-center py-4">
            <Spinner size="sm" />
          </div>
        )}

        {!isCreatingSession && messages.length === 0 && !streamingContent && (
          <div className="text-center py-3 text-gray-500 dark:text-gray-400">
            <BotIcon className="h-6 w-6 mx-auto mb-1 opacity-50" />
            <p className="text-xs">Ask AI about this block</p>
          </div>
        )}

        {error && (
          <div className="p-1.5 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 rounded text-xs text-red-800 dark:text-red-200">
            {error}
          </div>
        )}

        {messages.map((message, idx) => (
          <CompactMessage
            key={idx}
            message={message}
            pendingToolCalls={pendingToolCalls}
            rejectedToolCalls={rejectedToolCalls}
            onApprove={handleApprove}
            onDeny={handleDeny}
          />
        ))}

        {streamingContent !== null && (
          <div className="flex gap-1.5 py-1.5 px-2 rounded text-xs bg-gray-50 dark:bg-gray-800/50">
            <BotIcon className="h-3 w-3 text-purple-600 dark:text-purple-400 flex-shrink-0 mt-0.5" />
            <div className="flex-1 min-w-0">
              {streamingContent ? (
                <div className="whitespace-pre-wrap break-words text-gray-800 dark:text-gray-200">
                  <MarkdownContent content={streamingContent} isStreaming />
                </div>
              ) : (
                <span className="text-gray-500">Thinking...</span>
              )}
            </div>
          </div>
        )}
      </ScrollShadow>

      {/* Input */}
      <div className="flex-shrink-0 p-1.5 border-t border-gray-200 dark:border-gray-700">
        <div className="flex gap-1">
          <Textarea
            ref={textareaRef}
            value={inputValue}
            onValueChange={setInputValue}
            onKeyDown={handleKeyDown}
            placeholder="Ask about this block..."
            minRows={1}
            maxRows={3}
            disabled={!isConnected}
            variant="bordered"
            size="sm"
            classNames={{
              input: "text-xs",
              inputWrapper: "min-h-[28px] py-1",
            }}
          />
          {canCancel ? (
            <Button
              isIconOnly
              size="sm"
              color="danger"
              onPress={cancel}
              className="h-7 w-7 min-w-0"
            >
              <StopCircleIcon className="h-3 w-3" />
            </Button>
          ) : (
            <Button
              isIconOnly
              size="sm"
              color="primary"
              onPress={handleSend}
              disabled={!inputValue.trim() || !isConnected}
              className="h-7 w-7 min-w-0 bg-purple-600"
            >
              <SendIcon className="h-3 w-3" />
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
