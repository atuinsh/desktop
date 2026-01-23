import React, {
  useState,
  useRef,
  useEffect,
  useLayoutEffect,
  useCallback,
  memo,
  Component,
  ReactNode,
} from "react";
import { Streamdown } from "streamdown";
import {
  Button,
  Textarea,
  Spinner,
  ScrollShadow,
  ButtonGroup,
  Dropdown,
  DropdownTrigger,
  DropdownMenu,
  DropdownItem,
  Selection,
} from "@heroui/react";
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
  ArrowDownToLineIcon,
  StopCircleIcon,
} from "lucide-react";
import { BlockNoteEditor } from "@blocknote/core";
import { cn } from "@/lib/utils";
import { AIMessage } from "@/rs-bindings/AIMessage";
import { AIToolCall } from "@/rs-bindings/AIToolCall";
import useAIChat from "@/lib/ai/useAIChat";
import { changeChargeTarget, changeUser, createSession, destroySession } from "@/lib/ai/commands";
import AIBlockRegistry from "@/lib/ai/block_registry";
import { Settings } from "@/state/settings";
import { useStore } from "@/state/store";
import { ChargeTarget } from "@/rs-bindings/ChargeTarget";
import AtuinEnv from "@/atuin_env";
import { getModelSelection } from "@/state/settings_ai";
import { DialogBuilder } from "@/components/Dialogs/dialog";
import { ModelSelection } from "@/rs-bindings/ModelSelection";

const ALL_TOOL_NAMES = [
  "get_runbook_document",
  "get_block_docs",
  "get_default_shell",
  "insert_blocks",
  "update_block",
  "replace_blocks",
];
const AUTO_APPROVE_TOOLS = new Set(["get_block_docs"]);

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
  chargeTarget: ChargeTarget;
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

type ApprovalAction = "approve" | "approve-always";

function ToolCallUI({
  toolCall,
  isPending,
  isRejected,
  onApprove,
  onAlwaysApprove,
  onDeny,
}: {
  toolCall: AIToolCall;
  isPending: boolean;
  isRejected: boolean;
  onApprove: (toolCall: AIToolCall) => void;
  onAlwaysApprove: (toolCall: AIToolCall) => void;
  onDeny: (toolCall: AIToolCall) => void;
}) {
  const [selectedAction, setSelectedAction] = useState<ApprovalAction>("approve");
  const isApproved = !isPending && !isRejected;

  const handleButtonPress = () => {
    if (selectedAction === "approve") {
      onApprove(toolCall);
    } else {
      onAlwaysApprove(toolCall);
    }
  };

  const handleSelectionChange = (keys: Selection) => {
    const selected = Array.from(keys)[0] as ApprovalAction;
    if (selected) {
      setSelectedAction(selected);
    }
  };

  const buttonLabel = selectedAction === "approve" ? "Allow" : "Always Allow";

  return (
    <div className="mt-2 p-2 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-800 rounded overflow-x-auto">
      <div className="flex items-center gap-2">
        <WrenchIcon className="h-4 w-4 text-amber-600 dark:text-amber-400" />
        <span className="text-sm font-medium text-amber-700 dark:text-amber-300">
          Tool: {toolCall.name}
        </span>
        {isPending && <Spinner size="sm" variant="dots" />}
        {isApproved && <CheckIcon className="h-3 w-3 text-green-500" />}
        {isRejected && <XIcon className="h-3 w-3 text-red-500" />}
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
          <ButtonGroup>
            <Button
              size="sm"
              color="success"
              onPress={handleButtonPress}
              className="bg-green-600 text-white"
            >
              {buttonLabel}
            </Button>
            <Dropdown placement="bottom-end">
              <DropdownTrigger>
                <Button size="sm" isIconOnly color="success" className="bg-green-600 text-white">
                  <ChevronDownIcon className="h-3 w-3" />
                </Button>
              </DropdownTrigger>
              <DropdownMenu
                disallowEmptySelection
                aria-label="Tool approval options"
                className="max-w-[300px]"
                selectedKeys={new Set([selectedAction])}
                selectionMode="single"
                onSelectionChange={handleSelectionChange}
              >
                <DropdownItem key="approve">Allow this tool usage</DropdownItem>
                <DropdownItem key="approve-always">
                  Allow this and future uses of this tool
                </DropdownItem>
              </DropdownMenu>
            </Dropdown>
          </ButtonGroup>
        </div>
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
  rejectedToolCalls,
  onApprove,
  onAlwaysApprove,
  onDeny,
}: {
  message: AIMessage;
  pendingToolCalls: AIToolCall[];
  rejectedToolCalls: string[];
  onApprove: (toolCall: AIToolCall) => void;
  onAlwaysApprove: (toolCall: AIToolCall) => void;
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
        {toolCalls.map((toolCall) => (
          <ToolCallUI
            key={toolCall.id}
            toolCall={toolCall}
            isPending={pendingToolCalls.some((tc) => tc.id === toolCall.id)}
            isRejected={rejectedToolCalls.includes(toolCall.id)}
            onApprove={onApprove}
            onAlwaysApprove={onAlwaysApprove}
            onDeny={onDeny}
          />
        ))}
      </div>
    </div>
  );
}

// Queued message display - compact with expand/collapse
function QueuedMessageItem({ message }: { message: AIMessage }) {
  const [expanded, setExpanded] = useState(false);
  const text = getMessageText(message);

  return (
    <div
      className="flex items-start gap-1.5 px-2 py-1.5 bg-gray-100 dark:bg-gray-800 rounded text-sm text-gray-600 dark:text-gray-400 cursor-pointer hover:bg-gray-200 dark:hover:bg-gray-700"
      onClick={() => setExpanded(!expanded)}
    >
      {expanded ? (
        <ChevronDownIcon className="h-3.5 w-3.5 flex-shrink-0 mt-0.5" />
      ) : (
        <ChevronRightIcon className="h-3.5 w-3.5 flex-shrink-0 mt-0.5" />
      )}
      <span className={expanded ? "whitespace-pre-wrap" : "truncate"}>{text}</span>
    </div>
  );
}

// Tool execution functions
async function executeGetRunbookDocument(editor: BlockNoteEditor): Promise<any> {
  return { blocks: editor.document };
}

async function executeGetBlockDocs(
  _editor: BlockNoteEditor,
  params: { block_types: string[] },
): Promise<string> {
  return params.block_types.reduce((acc, blockType) => {
    acc += AIBlockRegistry.getInstance().getBlockDocs(blockType);
    return acc;
  }, "");
}

async function executeGetDefaultShell(): Promise<string> {
  return await Settings.getSystemDefaultShell();
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
      case "get_block_docs":
        result = await executeGetBlockDocs(editor, params);
        break;
      case "get_default_shell":
        result = await executeGetDefaultShell();
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
  chargeTarget,
  onClose,
}: AIAssistantProps) {
  const [inputValue, setInputValue] = useState("");
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [lockedToBottom, setLockedToBottom] = useState(true);
  const [isCreatingSession, setIsCreatingSession] = useState(false);
  const [rejectedToolCalls, setRejectedToolCalls] = useState<string[]>([]);
  const scrollShadowRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const autoApproveToolsRef = useRef<string[]>([...AUTO_APPROVE_TOOLS]);
  const user = useStore((state) => state.user);

  // Create session on mount
  useEffect(() => {
    if (!isOpen) return;
    const blockRegistry = AIBlockRegistry.getInstance();

    let mounted = true;
    setIsCreatingSession(true);

    createSession(
      runbookId,
      None,
      blockRegistry.getBlockInfos(),
      user.username,
      chargeTarget,
      AtuinEnv.url("/api/ai/proxy/"),
      true, // restore previous session if available
    )
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
  } = chat;

  useEffect(() => {
    if (!sessionId) return;
    changeChargeTarget(sessionId, chargeTarget);
  }, [sessionId, chargeTarget]);

  useEffect(() => {
    if (!sessionId) return;
    changeUser(sessionId, user.username);
  }, [sessionId, user.username]);

  // Can cancel when streaming or waiting for tool calls
  const canCancel = state !== "idle";

  // Detect user scroll interactions (wheel/touch) to unlock from bottom
  useEffect(() => {
    if (!scrollShadowRef.current) return;
    const el = scrollShadowRef.current;

    function checkIfAtBottom() {
      const { scrollTop, clientHeight, scrollHeight } = el;
      const isAtBottom = scrollTop + clientHeight >= scrollHeight - 5;
      setLockedToBottom(isAtBottom);
    }

    // These events only fire on user interaction, not programmatic scrolls
    function handleUserScroll() {
      // Check position after the scroll has been applied
      requestAnimationFrame(checkIfAtBottom);
    }

    el.addEventListener("wheel", handleUserScroll, { passive: true });
    el.addEventListener("touchmove", handleUserScroll, { passive: true });

    return () => {
      el.removeEventListener("wheel", handleUserScroll);
      el.removeEventListener("touchmove", handleUserScroll);
    };
  }, []);

  // Keep a ref to lockedToBottom for use in ResizeObserver callback
  const lockedToBottomRef = useRef(lockedToBottom);
  useLayoutEffect(() => {
    lockedToBottomRef.current = lockedToBottom;
  }, [lockedToBottom]);

  // Auto-scroll to bottom when content changes (messages added/removed, streaming updates)
  // MutationObserver catches all DOM changes including text updates during streaming
  useLayoutEffect(() => {
    if (!scrollShadowRef.current) return;
    const el = scrollShadowRef.current;

    const observer = new MutationObserver(() => {
      if (lockedToBottomRef.current) {
        el.scrollTop = el.scrollHeight;
      }
    });

    observer.observe(el, {
      childList: true,
      subtree: true,
      characterData: true,
    });

    return () => observer.disconnect();
  }, []);

  // Focus input when opened
  useEffect(() => {
    if (isOpen && sessionId) {
      setTimeout(() => {
        textareaRef.current?.focus();
      }, 100);
    }
  }, [isOpen, sessionId]);

  const handleSend = useCallback(async () => {
    if (!inputValue.trim() || isStreaming || !sessionId) return;

    const input = inputValue.trim();
    setInputValue("");

    let modelSelection: Result<ModelSelection, string> | null = null;
    try {
      const aiProvider = await Settings.aiAgentProvider();
      modelSelection = await getModelSelection(aiProvider);

      if (modelSelection.isErr()) {
        const err = modelSelection.unwrapErr();
        await new DialogBuilder()
          .title("AI Provider Error")
          .icon("error")
          .message("There was an error setting up your selected AI provider: " + err)
          .action({ label: "OK", value: undefined, variant: "flat" })
          .build();
        return;
      }
    } catch (err) {
      console.error("Failed to get model selection:", err);
      await new DialogBuilder()
        .title("AI Provider Error")
        .icon("error")
        .message("There was an error setting up your selected AI provider: " + err)
        .action({ label: "OK", value: undefined, variant: "flat" })
        .build();
      return;
    }

    // TODO: Allow buffering one message while streaming
    await changeModel(modelSelection.unwrap());
    sendMessage(input);
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

  const handleAlwaysApprove = useCallback(
    (toolCall: AIToolCall) => {
      autoApproveToolsRef.current.push(toolCall.name);
      handleApprove(toolCall);
    },
    [handleApprove],
  );

  const handleDeny = useCallback(
    (toolCall: AIToolCall, reason?: string) => {
      setRejectedToolCalls((prev) => [...prev, toolCall.id]);
      addToolOutput({
        toolCallId: toolCall.id,
        success: false,
        result: reason || "User denied tool execution",
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
      const blockRegistry = AIBlockRegistry.getInstance();
      destroySession(sessionId).catch(console.error);
      setSessionId(null);
      setIsCreatingSession(true);
      createSession(
        runbookId,
        None,
        blockRegistry.getBlockInfos(),
        user.username,
        chargeTarget,
        AtuinEnv.url("/api/ai/proxy/"),
        false, // don't restore - create fresh session
      )
        .then((id) => {
          setSessionId(id);
          setIsCreatingSession(false);
        })
        .catch((err) => {
          console.error("Failed to create AI session:", err);
          setIsCreatingSession(false);
        });
    }
  }, [sessionId, runbookId, user.username, chargeTarget]);

  const autoApprovedRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    if (!editor) return;

    for (const toolCall of pendingToolCalls) {
      if (!ALL_TOOL_NAMES.includes(toolCall.name)) {
        handleDeny(toolCall, `Unknown tool: ${toolCall.name}`);
        continue;
      }

      if (
        autoApproveToolsRef.current.includes(toolCall.name) &&
        !autoApprovedRef.current.has(toolCall.id)
      ) {
        autoApprovedRef.current.add(toolCall.id);
        handleApprove(toolCall);
      }
    }
  }, [pendingToolCalls, editor, handleApprove]);

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
      <div className="flex-1 min-h-0 relative">
        <ScrollShadow className="h-full overflow-y-auto p-3 space-y-2" ref={scrollShadowRef}>
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
                <span className="text-sm text-red-800 dark:text-red-200 whitespace-pre-wrap break-words">
                  {error}
                </span>
              </div>
            </div>
          )}
          {messages.map((message, idx) => (
            <MessageBubble
              key={idx}
              message={message}
              pendingToolCalls={pendingToolCalls}
              rejectedToolCalls={rejectedToolCalls}
              onApprove={handleApprove}
              onAlwaysApprove={handleAlwaysApprove}
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
                  <Spinner size="sm" variant="dots" className="ml-2" />
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
          {state === "sending" ||
            (streamingContent === null && state === "streaming" && (
              <Spinner size="sm" variant="wave" className="ml-2" />
            ))}
        </ScrollShadow>

        {/* Scroll to bottom button */}
        {!lockedToBottom && (messages.length > 0 || streamingContent !== null) && (
          <Button
            isIconOnly
            size="sm"
            variant="flat"
            onPress={() => {
              if (scrollShadowRef.current) {
                scrollShadowRef.current.scrollTop = scrollShadowRef.current.scrollHeight;
              }
              setLockedToBottom(true);
            }}
            className="absolute bottom-2 right-2 bg-white/80 dark:bg-gray-800/80 backdrop-blur-sm shadow-md"
            title="Scroll to bottom"
          >
            <ArrowDownToLineIcon className="h-4 w-4" />
          </Button>
        )}
      </div>

      {/* Queued messages */}
      {queuedMessages.length > 0 && (
        <div className="flex-shrink-0 px-3 pt-2 space-y-1 border-t border-gray-200 dark:border-gray-700">
          <span className="text-xs text-gray-500 dark:text-gray-400">Queued messages</span>
          {queuedMessages.map((msg, idx) => (
            <QueuedMessageItem key={idx} message={msg} />
          ))}
        </div>
      )}

      {/* Input */}
      <div
        className={cn(
          "flex-shrink-0 p-3 border-gray-200 dark:border-gray-700",
          queuedMessages.length === 0 && "border-t",
        )}
      >
        <div className="flex gap-2">
          <Textarea
            ref={textareaRef}
            value={inputValue}
            onValueChange={(t) => setInputValue(t)}
            onKeyDown={handleKeyDown}
            placeholder={
              canCancel
                ? "Message will be sent after current operation..."
                : "Ask the AI to help..."
            }
            minRows={1}
            maxRows={4}
            disabled={!isConnected}
            variant="bordered"
            classNames={{
              input: "text-sm",
              inputWrapper: "min-h-[40px]",
            }}
          />
          {canCancel ? (
            <Button
              isIconOnly
              color="danger"
              onPress={cancel}
              className="self-end"
              title="Cancel current operation"
            >
              <StopCircleIcon className="h-4 w-4" />
            </Button>
          ) : (
            <Button
              isIconOnly
              color="primary"
              onPress={handleSend}
              disabled={!inputValue.trim() || !isConnected}
              className="bg-purple-600 text-white self-end"
            >
              <SendIcon className="h-4 w-4" />
            </Button>
          )}
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
