import { useState, useRef, useEffect } from "react";
import { Button, Input, ScrollShadow } from "@heroui/react";
import { BotIcon, SendIcon, Loader2Icon, StopCircleIcon } from "lucide-react";
import { createReactBlockSpec } from "@blocknote/react";
import { useCurrentRunbookId } from "@/context/runbook_id_context";
import { getTemplateVar } from "@/state/templates";
import { streamText } from "ai";
import { createModel } from "@/lib/ai/provider";
import { Settings } from "@/state/settings";
import { tool } from "ai";
import { z } from "zod";
import { searchWorkspaceTool, getRunbookContentTool } from "@/lib/ai/tools";

interface Message {
  role: "user" | "assistant" | "system";
  content: string;
}

interface AgentProps {
  editor: any;
  isEditable: boolean;
}

const Agent = ({ editor, isEditable, blockId }: AgentProps & { blockId: string }) => {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const currentRunbookId = useCurrentRunbookId();
  const agentBlockIdRef = useRef<string>(blockId);
  const abortControllerRef = useRef<AbortController | null>(null);

  useEffect(() => {
    agentBlockIdRef.current = blockId;
  }, [blockId]);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages]);

  const handleStop = () => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
      setIsLoading(false);
    }
  };

  const handleSend = async () => {
    if (!input.trim() || isLoading) return;

    const userMessage: Message = { role: "user", content: input };
    setMessages((prev) => [...prev, userMessage]);
    setInput("");
    setIsLoading(true);

    // Create abort controller for this request
    abortControllerRef.current = new AbortController();

    try {
      const aiEnabled = await Settings.aiEnabled();
      if (!aiEnabled) {
        throw new Error("AI is not enabled. Please enable AI in settings.");
      }

      const apiKey = await Settings.aiApiKey();
      const apiEndpoint = await Settings.aiApiEndpoint();
      const modelName = await Settings.aiModel();

      if (!apiKey || apiKey.trim() === '') {
        throw new Error("No API key configured. Please set your API key in Settings.");
      }

      const model = createModel({
        apiKey,
        baseURL: apiEndpoint || undefined,
        model: modelName || undefined,
      });

      const currentDoc = editor.document;

      // Create tools for the agent
      const getTemplateVariableTool = tool({
        description: 'Get the value of a template variable from the current runbook',
        parameters: z.object({
          name: z.string().describe('The name of the template variable to retrieve'),
        }),
        execute: async ({ name }) => {
          if (!currentRunbookId) {
            return { success: false, error: 'No runbook context available' };
          }
          try {
            const value = await getTemplateVar(currentRunbookId, name);
            return { success: true, name, value: value || null };
          } catch (error) {
            return { success: false, error: String(error) };
          }
        },
      });

      const insertBlocksTool = tool({
        description: 'Insert one or more new blocks at a specific location in the document. Can insert before or after a block by its position (0-indexed).',
        parameters: z.object({
          blocks: z.array(z.any()).describe('Array of blocks to insert'),
          position: z.number().describe('0-indexed position where to insert. Use 0 for start, document.length for end'),
          placement: z.enum(['before', 'after']).describe('Insert before or after the block at position').default('after'),
        }),
        execute: async ({ blocks, position, placement }) => {
          try {
            const targetBlock = currentDoc[position];
            if (!targetBlock) {
              return { success: false, error: `No block at position ${position}` };
            }
            editor.insertBlocks(blocks, targetBlock.id, placement);
            return { success: true, blocksInserted: blocks.length, position };
          } catch (error) {
            return { success: false, error: String(error) };
          }
        },
      });

      const updateBlockTool = tool({
        description: 'Update a specific block by its position in the document (0-indexed).',
        parameters: z.object({
          position: z.number().describe('0-indexed position of the block to update'),
          props: z.any().describe('New props for the block'),
        }),
        execute: async ({ position, props }) => {
          try {
            const targetBlock = currentDoc[position];
            if (!targetBlock) {
              return { success: false, error: `No block at position ${position}` };
            }
            editor.updateBlock(targetBlock, { props });
            return { success: true, position };
          } catch (error) {
            return { success: false, error: String(error) };
          }
        },
      });

      const removeBlocksTool = tool({
        description: 'Remove one or more blocks by their positions (0-indexed). Cannot remove the agent block itself.',
        parameters: z.object({
          positions: z.array(z.number()).describe('Array of 0-indexed positions of blocks to remove'),
        }),
        execute: async ({ positions }) => {
          try {
            const blocksToRemove = positions.map(pos => currentDoc[pos]).filter(b => {
              if (!b) return false;
              if (b.id === agentBlockIdRef.current) return false; // Don't remove self
              return true;
            });
            if (blocksToRemove.length === 0) {
              return { success: false, error: 'No valid blocks to remove' };
            }
            editor.removeBlocks(blocksToRemove);
            return { success: true, blocksRemoved: blocksToRemove.length };
          } catch (error) {
            return { success: false, error: String(error) };
          }
        },
      });

      const systemPrompt = `You are an expert runbook editor AI agent. You can read variables and edit the document using these tools:
- get_template_variable: Read variable values from the current runbook
- insert_blocks: Add new blocks at a specific position (before/after)
- update_block: Modify a block's props by position
- remove_blocks: Delete blocks by positions (cannot delete yourself)

BLOCK STRUCTURE:
When creating blocks, use this exact JSON structure:
{
  "type": "block_type",
  "props": { /* block-specific props */ },
  "content": /* optional content array */
}

AVAILABLE BLOCK TYPES (with exact props):

1. run - Terminal command (PREFERRED for commands)
   {"type": "run", "props": {"code": "command here", "name": "Step name"}}

2. script - Multi-line shell script
   {"type": "script", "props": {"code": "script content", "name": "Script name", "lang": "bash"}}

3. postgres - PostgreSQL query
   {"type": "postgres", "props": {"query": "SELECT * FROM users", "name": "Query name", "uri": "postgresql://..."}}

4. sqlite - SQLite query
   {"type": "sqlite", "props": {"query": "SELECT * FROM users", "name": "Query name", "database": "/path/to/db"}}

5. clickhouse - ClickHouse query
   {"type": "clickhouse", "props": {"query": "SELECT * FROM events", "name": "Query name", "uri": "clickhouse://..."}}

6. mysql - MySQL query
   {"type": "mysql", "props": {"query": "SELECT * FROM users", "name": "Query name", "uri": "mysql://..."}}

7. http - HTTP request
   {"type": "http", "props": {"url": "https://api.example.com", "method": "GET", "name": "API call"}}

8. var - Template variable (synced)
   {"type": "var", "props": {"name": "variable_name", "value": "default value"}}

9. local-var - Local template variable (device-only)
   {"type": "local-var", "props": {"name": "variable_name", "value": "default value"}}

10. env - Environment variable
    {"type": "env", "props": {"name": "ENV_VAR", "value": "value"}}

11. directory - Change working directory
    {"type": "directory", "props": {"path": "/path/to/dir"}}

12. paragraph - Text content
    {"type": "paragraph", "content": [{"type": "text", "text": "Your text here"}]}

13. heading - Section heading
    {"type": "heading", "props": {"level": 1}, "content": [{"type": "text", "text": "Heading text"}]}

14. ssh-connect - SSH connection
    {"type": "ssh-connect", "props": {"userHost": "user@host:port"}}

15. editor - Multi-line code editor
    {"type": "editor", "props": {"code": "content", "language": "yaml", "variableName": "optional_var_name"}}

TEMPLATE VARIABLES:
- Use {{ var.variable_name }} syntax in code, queries, URLs, etc.
- Read variable values with get_template_variable tool
- Variables can store command output, API responses, user input

IMPORTANT:
- ALWAYS include required props for each block type
- Use exact prop names as shown above
- For postgres/mysql/clickhouse: use "query" not "code"
- For run/script: use "code" not "query"
- Blocks are 0-indexed positions in the document

Current document (${currentDoc.length} blocks):
${currentDoc.map((b: any, i: number) => `${i}: ${b.type} ${b.props?.name ? `"${b.props.name}"` : ''}`).join('\n')}

Full document structure:
${JSON.stringify(currentDoc, null, 2)}

Be precise with block structure and helpful in your responses.`;

      const result = await streamText({
        model,
        system: systemPrompt,
        messages: [
          ...messages.filter(m => m.role !== "system").map(m => ({ role: m.role, content: m.content })),
          { role: "user", content: userMessage.content },
        ],
        tools: {
          get_template_variable: getTemplateVariableTool,
          insert_blocks: insertBlocksTool,
          update_block: updateBlockTool,
          remove_blocks: removeBlocksTool,
          search_workspace: searchWorkspaceTool,
          get_runbook_content: getRunbookContentTool,
        },
        maxSteps: 10,
        abortSignal: abortControllerRef.current?.signal,
      });

      let currentAssistantMessage = "";
      let lastUpdateTime = Date.now();
      const UPDATE_INTERVAL = 100; // Update UI every 100ms to avoid jank
      let currentMessageIndex = -1; // Track which message we're updating

      // Stream the response
      for await (const chunk of result.fullStream) {
        if (chunk.type === "text-delta") {
          currentAssistantMessage += chunk.textDelta;
          
          // Throttle UI updates
          const now = Date.now();
          if (now - lastUpdateTime >= UPDATE_INTERVAL) {
            lastUpdateTime = now;
            setMessages((prev) => {
              const newMessages = [...prev];
              
              // If we don't have a current message index, create a new message
              if (currentMessageIndex === -1 || currentMessageIndex >= newMessages.length) {
                newMessages.push({ role: "assistant", content: currentAssistantMessage });
                currentMessageIndex = newMessages.length - 1;
              } else {
                // Update the existing message
                newMessages[currentMessageIndex] = {
                  role: "assistant",
                  content: currentAssistantMessage
                };
              }
              
              return newMessages;
            });
          }
        } else if (chunk.type === "tool-call") {
          // Finalize current text message before showing tool call
          if (currentAssistantMessage.trim()) {
            setMessages((prev) => {
              const newMessages = [...prev];
              if (currentMessageIndex === -1 || currentMessageIndex >= newMessages.length) {
                newMessages.push({ role: "assistant", content: currentAssistantMessage });
              } else {
                newMessages[currentMessageIndex] = {
                  role: "assistant",
                  content: currentAssistantMessage
                };
              }
              return newMessages;
            });
          }
          
          // Reset for next text chunk
          currentAssistantMessage = "";
          currentMessageIndex = -1;
          
          // Show tool call
          const toolName = chunk.toolName;
          const args = JSON.stringify(chunk.args);
          setMessages((prev) => [
            ...prev,
            { role: "system", content: `🔧 ${toolName}(${args})` }
          ]);
        } else if (chunk.type === "tool-result") {
          // Optional: show tool results
          // const result = JSON.stringify(chunk.result);
          // setMessages((prev) => [...prev, { role: "system", content: `✓ ${result}` }]);
        }
      }

      // Final update with complete message
      if (currentAssistantMessage.trim()) {
        setMessages((prev) => {
          const newMessages = [...prev];
          if (currentMessageIndex === -1 || currentMessageIndex >= newMessages.length) {
            newMessages.push({ role: "assistant", content: currentAssistantMessage });
          } else {
            newMessages[currentMessageIndex] = {
              role: "assistant",
              content: currentAssistantMessage
            };
          }
          return newMessages;
        });
      }

    } catch (error: any) {
      // Don't show error for user-initiated abort
      if (error?.name !== 'AbortError') {
        const errorMessage: Message = {
          role: "assistant",
          content: `Error: ${error}`,
        };
        setMessages((prev) => [...prev, errorMessage]);
      }
    } finally {
      setIsLoading(false);
      abortControllerRef.current = null;
    }
  };

  return (
    <div className="w-full bg-gradient-to-r from-purple-50 to-blue-50 dark:from-slate-800 dark:to-blue-950 rounded-lg border border-purple-200 dark:border-purple-900 shadow-sm">
      <div className="flex items-center gap-2 p-3 border-b border-purple-200 dark:border-purple-800">
        <Button isIconOnly variant="light" size="sm" className="bg-purple-100 dark:bg-purple-800 text-purple-600 dark:text-purple-300">
          <BotIcon className="h-4 w-4" />
        </Button>
        <span className="text-sm font-medium text-purple-700 dark:text-purple-300">Agent</span>
      </div>

      <ScrollShadow className="max-h-64 overflow-y-auto" ref={scrollRef}>
        <div className="p-3 space-y-2">
          {messages.length === 0 ? (
            <div className="text-sm text-gray-500 dark:text-gray-400 text-center py-4">
              Ask me to edit the document or read template variables
            </div>
          ) : (
            messages.map((msg, idx) => (
              <div
                key={idx}
                className={`text-sm p-2 rounded ${
                  msg.role === "user"
                    ? "bg-purple-100 dark:bg-purple-900 ml-8"
                    : msg.role === "system"
                    ? "bg-gray-100 dark:bg-gray-800 text-xs font-mono"
                    : "bg-white dark:bg-slate-700 mr-8"
                }`}
              >
                {msg.role !== "system" && (
                  <div className="font-medium text-xs mb-1 text-purple-600 dark:text-purple-400">
                    {msg.role === "user" ? "You" : "Agent"}
                  </div>
                )}
                <div className="text-gray-800 dark:text-gray-200 whitespace-pre-wrap">{msg.content}</div>
              </div>
            ))
          )}
          {isLoading && (
            <div className="flex items-center gap-2 text-sm text-gray-500 dark:text-gray-400 mr-8 p-2">
              <Loader2Icon className="h-3 w-3 animate-spin" />
              <span>Thinking...</span>
            </div>
          )}
        </div>
      </ScrollShadow>

      <div className="p-3 border-t border-purple-200 dark:border-purple-800">
        <div className="flex gap-2">
          <Input
            placeholder="Ask me to edit the document..."
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                if (isLoading) {
                  handleStop();
                } else {
                  handleSend();
                }
              }
            }}
            disabled={!isEditable}
            size="sm"
            className="flex-1"
          />
          {isLoading ? (
            <Button
              isIconOnly
              size="sm"
              color="danger"
              onClick={handleStop}
              disabled={!isEditable}
            >
              <StopCircleIcon className="h-4 w-4" />
            </Button>
          ) : (
            <Button
              isIconOnly
              size="sm"
              color="primary"
              onClick={handleSend}
              disabled={!isEditable || !input.trim()}
            >
              <SendIcon className="h-4 w-4" />
            </Button>
          )}
        </div>
      </div>
    </div>
  );
};

export default createReactBlockSpec(
  {
    type: "agent",
    propSchema: {},
    content: "none",
  },
  {
    render: ({ block, editor }) => {
      return (
        <Agent
          editor={editor}
          isEditable={editor.isEditable}
          blockId={block.id}
        />
      );
    },
  },
);
