import React, { useState, useRef, useEffect, useCallback } from "react";
import {
  Button,
  Textarea,
  Spinner,
  ScrollShadow,
  Card,
  CardBody,
  Tooltip,
  Chip,
} from "@heroui/react";
import {
  BotIcon,
  AlertCircleIcon,
  TrashIcon,
  Sparkles,
} from "lucide-react";
import { streamChatResponse, isAIChatEnabled, type ChatMessage, type ToolCallEvent } from "@/lib/agent";
import { uuidv7 } from "uuidv7";
import track_event from "@/tracking";
import MarkdownContent from "./MarkdownContent";
import "./markdown-styles.css";

interface AgentChatProps {
  className?: string;
}

const SYSTEM_PROMPT: ChatMessage = {
  id: "system",
  role: "system",
  content: `You are an AI assistant helping SREs and DevOps engineers with their runbooks and operational tasks. 

TOOLS:
- getCurrentDocument: Fetch runbook blocks with IDs, types, content
- addBlocks: Add blocks to end (max 3)
- insertBlocksAfter: Insert blocks after a specific block ID (max 3)
- updateBlock: Modify block properties/content by ID
- deleteBlock/deleteBlocks: Remove blocks by ID
- replaceBlock: Replace a block with new blocks

BLOCK TYPES & WHEN TO USE THEM:

Terminal Execution:
- run: Single command or interactive/long-running processes (props: code, name)
- script: Multi-line shell scripts (props: code, name, lang)

Variables & Templating:
- var: Define template variables (props: name, value) - Access as {{ var.name }}
- var_display: Display a variable's value (props: name)
- env: Environment variables (props: name, value)
- local-var: Local variables (props: name, value)
- dropdown: User selection with variable output (props: name, options, optionsType, value, interpreter)

SSH & Remote Execution:
- ssh-connect: Connect to remote host (props: userHost as "user@host:port")
  * ALL subsequent run/script blocks execute on this host automatically
  * Use host-select with hosts="localhost" to switch back to local
  * Example workflow: ssh-connect → run commands on remote → host-select localhost

Data & Queries:
- postgres: PostgreSQL query (props: query, host, port, database, user)
- sqlite: SQLite query (props: query, database)
- clickhouse: ClickHouse query (props: query, host, port, database)
- prometheus: Prometheus query (props: query, url)

HTTP & APIs:
- http: HTTP request (props: url, method, headers, body)

Content Editing:
- editor: Multi-line text/code/config editor (props: code, language, variableName)
  * Use for YAML, JSON, scripts, configs instead of heredocs
  * Set variableName to store content, access as {{ var.variableName }}
- paragraph: Text paragraphs with structured content (NOT markdown!)
  * Content format: [{type: "text", text: "hello", styles: {bold: true, italic: true}}]
  * Available styles: bold, italic, underline, strikethrough, code
  * Example: {type: "paragraph", content: [{type: "text", text: "Bold text", styles: {bold: true}}]}
- heading: Section headings (props: {level: 1-3})
  * Content format same as paragraph: [{type: "text", text: "Title"}]

Directory:
- directory: Change working directory (props: path)

MINIJINJA TEMPLATING (critical!):

Variables use MiniJinja syntax:
- Reference: {{ var.variable_name }}
- Conditionals: {% if var.foo %}...{% else %}...{% endif %}
- Loops: {% for item in var.list %}{{ item }}{% endfor %}
- Filters: {{ var.text | shellquote }} (for safe shell use)
- Named blocks: {{ doc.named["block_name"].content }}

BEST PRACTICES:

1. VARIABLE ORDERING (CRITICAL):
   ⚠️ Variables MUST be defined BEFORE they are used!
   ⚠️ Put var/dropdown/script blocks that SET variables at the TOP
   ⚠️ Put blocks that USE variables (with {{ var.x }}) AFTER
   
   Example order:
   1. var block: {type: "var", props: {name: "API_KEY", value: "xxx"}}
   2. dropdown: {type: "dropdown", props: {name: "environment", options: "dev,prod"}}
   3. run block using them: {type: "run", props: {code: "curl {{ var.API_KEY }}"}}
   
   Wrong order will cause template errors!

2. USE TEMPLATE VARIABLES:
   - Store reusable values in var blocks at the top
   - Use {{ var.name }} to reference them
   - Use {{ var.text | shellquote }} for safe shell injection
   - Name blocks and access via {{ doc.named["name"].content }}
   - Script blocks can output to variables (set variableName prop)

3. SSH WORKFLOW:
   - ssh-connect establishes persistent connection
   - All run/script blocks after ssh-connect run on remote host
   - No need to repeat ssh in each command
   - Switch back: {type: "host-select", props: {hosts: "localhost"}}

4. TERMINAL ISOLATION:
   - Each run/script block is independent (new PTY)
   - Use 'directory' blocks to set working directory
   - Use 'env' blocks for environment variables
   - Use template variables to pass data between blocks

5. PREFER STRUCTURED DATA:
   - Use editor blocks for configs (YAML/JSON)
   - Use dropdown for user selections
   - Store outputs in variables for reuse

ITERATION:
1. Call getCurrentDocument to see block IDs
2. Use updateBlock for modifications (efficient)
3. Use insertBlocksAfter to add blocks in specific positions
4. You CAN and SHOULD iterate on your own blocks

Be concise, technical, and actionable.`,
  timestamp: Date.now(),
};

export default function AgentChat({ className = "" }: AgentChatProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [isStreaming, setIsStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isAIEnabled, setIsAIEnabled] = useState(true);
  const [currentToolActivity, setCurrentToolActivity] = useState<string>("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const streamingMessageRef = useRef<string>("");

  // Check if AI is enabled on mount
  useEffect(() => {
    isAIChatEnabled().then(setIsAIEnabled);
  }, []);

  // Auto-scroll to bottom when messages change
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, isStreaming]);

  const handleSendMessage = useCallback(async () => {
    if (!input.trim() || isLoading) return;

    const userMessage: ChatMessage = {
      id: uuidv7(),
      role: "user",
      content: input.trim(),
      timestamp: Date.now(),
    };

    setMessages((prev) => [...prev, userMessage]);
    setInput("");
    setIsLoading(true);
    setIsStreaming(true);
    setError(null);
    streamingMessageRef.current = "";

    track_event("agent.message_sent", {
      messageLength: userMessage.content.length,
    });

    // Create a temporary assistant message for streaming
    const assistantMessageId = uuidv7();
    const assistantMessage: ChatMessage = {
      id: assistantMessageId,
      role: "assistant",
      content: "",
      timestamp: Date.now(),
    };

    setMessages((prev) => [...prev, assistantMessage]);

    try {
      setCurrentToolActivity("");
      await streamChatResponse({
        messages: [SYSTEM_PROMPT, ...messages, userMessage],
        onChunk: (chunk) => {
          streamingMessageRef.current += chunk;
          setMessages((prev) => {
            const updated = [...prev];
            const lastMessage = updated[updated.length - 1];
            if (lastMessage && lastMessage.id === assistantMessageId) {
              lastMessage.content = streamingMessageRef.current;
            }
            return updated;
          });
        },
        onToolCall: (toolCall) => {
          console.log("[UI] Tool call:", toolCall);
          if (toolCall.result) {
            // Tool completed
            setCurrentToolActivity(`✓ ${toolCall.toolName}`);
            setTimeout(() => setCurrentToolActivity(""), 1000);
          } else {
            // Tool starting
            setCurrentToolActivity(`${toolCall.toolName}...`);
          }
        },
        onComplete: (fullText) => {
          setIsLoading(false);
          setIsStreaming(false);
          setCurrentToolActivity("");
          track_event("agent.message_received", {
            responseLength: fullText.length,
          });
        },
        onError: (err) => {
          setIsLoading(false);
          setIsStreaming(false);
          setCurrentToolActivity("");
          setError(err.message);
          setMessages((prev) => prev.slice(0, -1));
          track_event("agent.error", {
            error: err.message,
          });
        },
      });
    } catch (err) {
      setIsLoading(false);
      setIsStreaming(false);
      setError((err as Error).message);
      setMessages((prev) => prev.slice(0, -1));
    }
  }, [input, messages, isLoading]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSendMessage();
      }
    },
    [handleSendMessage]
  );

  const handleClearChat = useCallback(() => {
    setMessages([]);
    setError(null);
    track_event("agent.chat_cleared");
  }, []);

  if (!isAIEnabled) {
    return (
      <div className={`flex flex-col h-full p-2 ${className}`}>
        <Card className="bg-warning-50 border-warning-200">
          <CardBody className="gap-2 p-2">
            <div className="flex items-center gap-2">
              <AlertCircleIcon size={20} className="text-warning-600" />
              <span className="font-semibold text-warning-700">AI Not Configured</span>
            </div>
            <p className="text-sm text-warning-700">
              Enable AI and configure your API key in Settings to use the agent chat.
            </p>
          </CardBody>
        </Card>
      </div>
    );
  }

  return (
    <div className={`flex flex-col h-full ${className}`}>
      {/* Header */}
      <div className="flex items-center justify-between p-2 border-b">
        <div className="flex items-center gap-2">
          <Sparkles size={18} className="text-primary" />
          <h3 className="font-semibold text-sm">AI Agent</h3>
          <Chip size="sm" variant="flat" color="primary">
            Beta
          </Chip>
        </div>
        {messages.length > 0 && (
          <Tooltip content="Clear chat" placement="left">
            <Button
              isIconOnly
              size="sm"
              variant="light"
              onPress={handleClearChat}
              isDisabled={isLoading}
            >
              <TrashIcon size={16} />
            </Button>
          </Tooltip>
        )}
      </div>

      {/* Messages */}
      <ScrollShadow
        ref={scrollRef}
        className="flex-1 overflow-y-auto p-2 space-y-2"
        hideScrollBar={false}
      >
        {messages.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full text-center px-2">
            <BotIcon size={48} className="text-default-300 mb-4" />
            <h4 className="font-semibold text-default-700 mb-2">
              AI Agent Ready
            </h4>
            <p className="text-sm text-default-500 max-w-xs">
              Ask me about commands, scripts, debugging, or anything related to your runbooks.
            </p>
          </div>
        )}

        {messages.map((message) => (
          <MessageBubble key={message.id} message={message} isStreaming={isStreaming && message === messages[messages.length - 1]} />
        ))}

        {/* Tool activity - subtle indicator */}
        {currentToolActivity && (
          <div className="text-xs text-default-400 px-1 italic">
            {currentToolActivity}
          </div>
        )}

        {error && (
          <Card className="bg-danger-50 border-danger-200">
            <CardBody className="gap-2 p-2">
              <div className="flex items-center gap-2">
                <AlertCircleIcon size={16} className="text-danger-600" />
                <span className="font-semibold text-sm text-danger-700">Error</span>
              </div>
              <p className="text-sm text-danger-700">{error}</p>
            </CardBody>
          </Card>
        )}
      </ScrollShadow>

      {/* Input */}
      <div className="p-2 border-t">
        <Textarea
          ref={textareaRef}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Ask me anything..."
          minRows={2}
          maxRows={8}
          variant="flat"
          classNames={{
            input: "text-sm",
            inputWrapper: "shadow-none border-1 border-default-200 data-[hover=true]:border-default-300 group-data-[focus=true]:border-primary",
          }}
          isDisabled={isLoading}
          endContent={
            isLoading && (
              <div className="flex items-center">
                <Spinner size="sm" />
              </div>
            )
          }
        />
        <p className="text-xs text-default-400 mt-1">
          ⌘+Enter to send
        </p>
      </div>
    </div>
  );
}

interface MessageBubbleProps {
  message: ChatMessage;
  isStreaming?: boolean;
}

function MessageBubble({ message, isStreaming = false }: MessageBubbleProps) {
  const isUser = message.role === "user";

  return (
    <div className="w-full">
      <Card
        className={`${
          isUser
            ? "bg-primary-50 border-primary-200"
            : "bg-default-50 border-default-200"
        }`}
      >
        <CardBody className="p-2">
          {isUser ? (
            <div className="text-sm whitespace-pre-wrap break-words text-primary-900">
              {message.content}
            </div>
          ) : (
            <div className="text-sm text-default-900">
              <MarkdownContent content={message.content} />
              {isStreaming && (
                <span className="inline-block w-2 h-4 ml-1 bg-current animate-pulse" />
              )}
            </div>
          )}
        </CardBody>
      </Card>
      <div className={`text-xs text-default-400 mt-0.5 px-1 ${isUser ? "text-right" : "text-left"}`}>
        {new Date(message.timestamp).toLocaleTimeString()}
      </div>
    </div>
  );
}

