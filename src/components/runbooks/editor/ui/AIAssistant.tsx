import React, { useState, useRef, useEffect, useCallback, memo, Component, ReactNode } from "react";
import { Streamdown } from "streamdown";
import { Button, Textarea, Spinner, ScrollShadow } from "@heroui/react";
import {
  SparklesIcon,
  SendIcon,
  XIcon,
  CheckIcon,
  AlertCircleIcon,
  BotIcon,
  UserIcon,
  WrenchIcon,
  ChevronRightIcon,
  ChevronDownIcon,
} from "lucide-react";
import { BlockNoteEditor } from "@blocknote/core";
import { cn } from "@/lib/utils";
import { AIMessage } from "@/rs-bindings/AIMessage";
import { AIToolCall } from "@/rs-bindings/AIToolCall";
import useAIChat from "@/lib/ai/useAIChat";
import { createSession, destroySession } from "@/lib/ai/commands";

// Error boundary for Streamdown - falls back to plain text if it fails
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

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.warn("Streamdown render failed, falling back to plain text:", error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return this.props.fallback;
    }
    return this.props.children;
  }
}

// Memoized markdown renderer with error boundary
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

interface AIAssistantProps {
  runbookId: string;
  editor: BlockNoteEditor | null;
  getContext: () => Promise<AIContext>;
  isOpen: boolean;
  onClose: () => void;
}

export interface AIContext {
  variables: string[];
  named_blocks: [string, string][];
  working_directory: string | null;
  environment_variables: string[];
  ssh_host: string | null;
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
        {expanded ? "Hide parameters" : "Show parameters"}
      </button>
      {expanded && (
        <pre className="mt-1 p-2 bg-gray-100 dark:bg-gray-800 rounded text-xs overflow-x-auto max-h-40 overflow-y-auto">
          {formatted}
        </pre>
      )}
    </div>
  );
}

// Extract text content from AIMessage
function getMessageText(message: AIMessage): string {
  return message.content.parts
    .filter((part): part is { type: "text"; data: string } => part.type === "text")
    .map((part) => part.data)
    .join("");
}

// Extract tool calls from AIMessage
function getToolCalls(message: AIMessage): AIToolCall[] {
  return message.content.parts
    .filter((part): part is { type: "toolCall"; data: AIToolCall } => part.type === "toolCall")
    .map((part) => part.data);
}

function MessageBubble({
  message,
  pendingToolCalls,
  onApprove,
  onDeny,
}: {
  message: AIMessage;
  pendingToolCalls: AIToolCall[];
  onApprove: (toolCall: AIToolCall) => void;
  onDeny: (toolCall: AIToolCall) => void;
}) {
  const isUser = message.role === "user";
  const isAssistant = message.role === "assistant";
  const isTool = message.role === "tool";

  const text = getMessageText(message);
  const toolCalls = getToolCalls(message);

  return (
    <div
      className={cn(
        "flex gap-2 py-2 px-3 rounded-lg",
        isUser && "bg-blue-50 dark:bg-blue-950/30",
        isAssistant && "bg-gray-50 dark:bg-gray-800/50",
        isTool && "bg-gray-100 dark:bg-gray-800",
      )}
    >
      <div className="flex-shrink-0 mt-0.5">
        {isUser && <UserIcon className="h-4 w-4 text-blue-600 dark:text-blue-400" />}
        {isAssistant && <BotIcon className="h-4 w-4 text-purple-600 dark:text-purple-400" />}
        {isTool && <WrenchIcon className="h-4 w-4 text-gray-500 dark:text-gray-400" />}
      </div>

      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span
            className={cn(
              "text-xs font-medium",
              isUser && "text-blue-700 dark:text-blue-300",
              isAssistant && "text-purple-700 dark:text-purple-300",
              isTool && "text-gray-600 dark:text-gray-400",
            )}
          >
            {isUser && "You"}
            {isAssistant && "Assistant"}
            {isTool && "Tool Response"}
          </span>
        </div>

        {text && (
          <div
            className={cn(
              "text-sm mt-1 whitespace-pre-wrap break-words",
              isUser && "text-blue-900 dark:text-blue-100",
              isAssistant && "text-gray-800 dark:text-gray-200",
              isTool && "text-gray-700 dark:text-gray-300",
            )}
          >
            <MarkdownContent content={text} />
          </div>
        )}

        {/* Render tool calls */}
        {toolCalls.map((toolCall) => {
          const isPending = pendingToolCalls.some((tc) => tc.id === toolCall.id);
          return (
            <div
              key={toolCall.id}
              className="mt-2 p-2 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-800 rounded overflow-x-auto"
            >
              <div className="flex items-center gap-2">
                <WrenchIcon className="h-4 w-4 text-amber-600 dark:text-amber-400" />
                <span className="text-sm font-medium text-amber-700 dark:text-amber-300">
                  Tool: {toolCall.name}
                </span>
                {isPending && <Spinner size="sm" variant="dots" />}
                {!isPending && <CheckIcon className="h-3 w-3 text-green-500" />}
              </div>
              <ToolParamsDisplay params={toolCall.args} />
              {isPending && (
                <div className="flex gap-2 mt-2">
                  <Button
                    size="sm"
                    variant="light"
                    onPress={() => onDeny(toolCall)}
                    className="text-red-600 dark:text-red-400"
                  >
                    Deny
                  </Button>
                  <Button
                    size="sm"
                    color="success"
                    onPress={() => onApprove(toolCall)}
                    className="bg-green-600 text-white"
                  >
                    Allow
                  </Button>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// Tool execution functions
async function executeGetRunbookDocument(editor: BlockNoteEditor): Promise<any> {
  return { blocks: editor.document };
}

async function executeInsertBlocks(
  editor: BlockNoteEditor,
  params: { blocks: any[]; position: "before" | "after" | "end"; reference_block_id?: string },
): Promise<any> {
  const { blocks, position, reference_block_id } = params;

  if (position === "end") {
    const lastBlock = editor.document[editor.document.length - 1];
    editor.insertBlocks(blocks, lastBlock.id, "after");
  } else if (reference_block_id) {
    editor.insertBlocks(blocks, reference_block_id, position);
  } else {
    throw new Error("reference_block_id required for 'before' or 'after' position");
  }

  return { success: true };
}

async function executeUpdateBlock(
  editor: BlockNoteEditor,
  params: { block_id: string; props?: any; content?: any },
): Promise<any> {
  const { block_id, props, content } = params;

  const updates: any = {};
  if (props) updates.props = props;
  if (content) updates.content = content;

  editor.updateBlock(block_id, updates);
  return { success: true };
}

async function executeReplaceBlocks(
  editor: BlockNoteEditor,
  params: { block_ids: string[]; new_blocks: any[] },
): Promise<any> {
  const { block_ids, new_blocks } = params;

  if (block_ids.length === 0) {
    throw new Error("block_ids cannot be empty");
  }

  const blocksToReplace = editor.document.filter((b: any) => block_ids.includes(b.id));
  if (blocksToReplace.length === 0) {
    throw new Error("No blocks found with the specified IDs");
  }

  editor.replaceBlocks(blocksToReplace, new_blocks);
  return { success: true };
}

async function executeTool(
  editor: BlockNoteEditor,
  toolName: string,
  params: any,
): Promise<{ success: boolean; result: string }> {
  try {
    let result: any;
    switch (toolName) {
      case "get_runbook_document":
        result = await executeGetRunbookDocument(editor);
        break;
      case "insert_blocks":
        result = await executeInsertBlocks(editor, params);
        break;
      case "update_block":
        result = await executeUpdateBlock(editor, params);
        break;
      case "replace_blocks":
        result = await executeReplaceBlocks(editor, params);
        break;
      default:
        throw new Error(`Unknown tool: ${toolName}`);
    }
    return { success: true, result: JSON.stringify(result) };
  } catch (error) {
    const message = error instanceof Error ? error.message : "Unknown error";
    return { success: false, result: message };
  }
}

export default function AIAssistant({
  runbookId,
  editor,
  getContext: _getContext, // TODO: Use context in AI requests
  isOpen,
  onClose,
}: AIAssistantProps) {
  const [inputValue, setInputValue] = useState("");
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [isCreatingSession, setIsCreatingSession] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Create session on mount
  useEffect(() => {
    if (!isOpen) return;

    let mounted = true;
    setIsCreatingSession(true);

    createSession()
      .then((id) => {
        if (mounted) {
          setSessionId(id);
          setIsCreatingSession(false);
        }
      })
      .catch((err) => {
        console.error("Failed to create AI session:", err);
        if (mounted) {
          setIsCreatingSession(false);
        }
      });

    return () => {
      mounted = false;
      // Destroy session on unmount
      if (sessionId) {
        destroySession(sessionId).catch(console.error);
      }
    };
  }, [isOpen]);

  // Destroy old session when runbookId changes
  useEffect(() => {
    return () => {
      if (sessionId) {
        destroySession(sessionId).catch(console.error);
        setSessionId(null);
      }
    };
  }, [runbookId]);

  const chat = useAIChat(sessionId || "");
  const {
    messages,
    streamingContent,
    isStreaming,
    pendingToolCalls,
    error,
    sendMessage,
    addToolOutput,
  } = chat;

  // Auto-scroll to bottom when new messages arrive or streaming content updates
  // TODO: Only scroll if the user is at the bottom of the chat
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streamingContent]);

  // Focus input when opened
  useEffect(() => {
    if (isOpen && sessionId) {
      setTimeout(() => {
        textareaRef.current?.focus();
      }, 100);
    }
  }, [isOpen, sessionId]);

  const handleSend = useCallback(() => {
    if (!inputValue.trim() || isStreaming || !sessionId) return;
    // TODO: Allow buffering one message while streaming
    sendMessage(inputValue.trim());
    setInputValue("");
  }, [inputValue, isStreaming, sessionId, sendMessage]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    // TODO: send on ctrl/cmd+enter, newline on enter
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

      const { success, result } = await executeTool(editor, toolCall.name, toolCall.args);
      addToolOutput({
        toolCallId: toolCall.id,
        success,
        result,
      });
    },
    [editor, addToolOutput],
  );

  const handleDeny = useCallback(
    (toolCall: AIToolCall) => {
      addToolOutput({
        toolCallId: toolCall.id,
        success: false,
        result: "User denied tool execution",
      });
    },
    [addToolOutput],
  );

  const handleClear = useCallback(() => {
    // Destroy current session and create a new one
    // TODO: replace with tabs containing sessions;
    // only create new sessions or delete old ones
    // keep history in sql
    if (sessionId) {
      destroySession(sessionId).catch(console.error);
      setSessionId(null);
      setIsCreatingSession(true);
      createSession()
        .then((id) => {
          setSessionId(id);
          setIsCreatingSession(false);
        })
        .catch((err) => {
          console.error("Failed to create AI session:", err);
          setIsCreatingSession(false);
        });
    }
  }, [sessionId]);

  if (!isOpen) return null;

  const isConnected = sessionId !== null && !isCreatingSession;

  return (
    <div className="flex flex-col h-full min-h-0 bg-white dark:bg-gray-900 border-l border-gray-200 dark:border-gray-700">
      {/* Header */}
      <div className="flex-shrink-0 flex items-center justify-between p-3 border-b border-gray-200 dark:border-gray-700 bg-gradient-to-r from-purple-50 to-blue-50 dark:from-purple-950/20 dark:to-blue-950/20">
        <div className="flex items-center gap-2">
          <SparklesIcon className="h-5 w-5 text-purple-600 dark:text-purple-400" />
          <span className="font-medium text-purple-900 dark:text-purple-100">AI Assistant</span>
        </div>
        <div className="flex items-center gap-2">
          <div
            className={cn("h-2 w-2 rounded-full", isConnected ? "bg-green-500" : "bg-red-500")}
            title={isConnected ? "Connected" : "Connecting..."}
          />
          <Button
            size="sm"
            isIconOnly
            variant="light"
            onPress={onClose}
            className="text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
          >
            <XIcon className="h-4 w-4" />
          </Button>
        </div>
      </div>

      {/* Messages */}
      <ScrollShadow className="flex-1 min-h-0 overflow-y-auto p-3 space-y-2">
        {isCreatingSession && (
          <div className="flex items-center justify-center h-full">
            <Spinner size="lg" />
          </div>
        )}
        {!isCreatingSession && messages.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full text-center text-gray-500 dark:text-gray-400">
            <BotIcon className="h-12 w-12 mb-4 opacity-50" />
            <p className="text-sm">Ask me to help edit your runbook.</p>
            <p className="text-xs mt-2 opacity-75">
              I can read and modify blocks in your document.
            </p>
          </div>
        )}
        {error && (
          <div className="p-3 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 rounded-lg">
            <div className="flex items-center gap-2">
              <AlertCircleIcon className="h-4 w-4 text-red-600 dark:text-red-400" />
              <span className="text-sm text-red-800 dark:text-red-200">{error}</span>
            </div>
          </div>
        )}
        {messages.map((message, idx) => (
          <MessageBubble
            key={idx}
            message={message}
            pendingToolCalls={pendingToolCalls}
            onApprove={handleApprove}
            onDeny={handleDeny}
          />
        ))}
        {/* Show streaming content as it arrives */}
        {streamingContent !== null && (
          <div className="flex gap-2 py-2 px-3 rounded-lg bg-gray-50 dark:bg-gray-800/50">
            <div className="flex-shrink-0 mt-0.5">
              <BotIcon className="h-4 w-4 text-purple-600 dark:text-purple-400" />
            </div>
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2">
                <span className="text-xs font-medium text-purple-700 dark:text-purple-300">
                  Assistant
                </span>
                <Spinner size="sm" variant="wave" className="ml-2" />
              </div>
              {streamingContent ? (
                <div className="text-sm mt-1 whitespace-pre-wrap break-words text-gray-800 dark:text-gray-200">
                  <MarkdownContent content={streamingContent} isStreaming={true} />
                </div>
              ) : (
                <div className="text-sm mt-1 text-gray-500 dark:text-gray-400">Thinking...</div>
              )}
            </div>
          </div>
        )}
        <div ref={messagesEndRef} />
      </ScrollShadow>

      {/* Input */}
      <div className="flex-shrink-0 p-3 border-t border-gray-200 dark:border-gray-700">
        <div className="flex gap-2">
          <Textarea
            ref={textareaRef}
            value={inputValue}
            onValueChange={(t) => setInputValue(t)}
            onKeyDown={handleKeyDown}
            placeholder="Ask the AI to help..."
            minRows={1}
            maxRows={4}
            disabled={isStreaming || !isConnected}
            variant="bordered"
            classNames={{
              input: "text-sm",
              inputWrapper: "min-h-[40px]",
            }}
          />
          <Button
            isIconOnly
            color="primary"
            onPress={handleSend}
            disabled={!inputValue.trim() || isStreaming || !isConnected}
            className="bg-purple-600 text-white self-end"
          >
            {isStreaming ? <Spinner size="sm" /> : <SendIcon className="h-4 w-4" />}
          </Button>
        </div>
        <div className="flex items-center justify-between mt-2">
          <span className="text-xs text-gray-400">
            <kbd className="px-1 py-0.5 bg-gray-100 dark:bg-gray-800 rounded">Enter</kbd> to send
          </span>
          {messages.length > 0 && (
            <Button
              size="sm"
              variant="light"
              onPress={handleClear}
              className="text-xs text-gray-500 hover:text-gray-700"
            >
              Clear chat
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
