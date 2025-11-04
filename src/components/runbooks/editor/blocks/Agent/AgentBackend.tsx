import { useState, useRef, useEffect } from "react";
import { Button, Input, ScrollShadow, Textarea } from "@heroui/react";
import { BotIcon, SendIcon, Loader2Icon, StopCircleIcon } from "lucide-react";
import { useCurrentRunbookId } from "@/context/runbook_id_context";
import PlayButton from "@/lib/blocks/common/PlayButton";
import { useBlockExecution, useBlockOutput } from "@/lib/hooks/useDocumentBridge";

interface Message {
  role: "user" | "assistant" | "system";
  content: string;
}

interface AgentProps {
  id: string;
  editor: any;
  isEditable: boolean;
  blockId: string;
}

export const AgentBackend = ({ id, isEditable, blockId }: AgentProps) => {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [initialPrompt, setInitialPrompt] = useState("Create a hello world script");
  const [isLoading, setIsLoading] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const currentRunbookId = useCurrentRunbookId();
  const execution = useBlockExecution(id);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages]);

  useBlockOutput(id, (output) => {
    if (!currentRunbookId) return;

    if (output.object) {
      const agentEvent: any = output.object;

      switch (agentEvent.type) {
        case "assistantDelta":
          // Update the last assistant message or create new one
          setMessages((prev) => {
            const lastMsg = prev[prev.length - 1];
            if (lastMsg && lastMsg.role === "assistant") {
              // Append to existing assistant message
              return [
                ...prev.slice(0, -1),
                { ...lastMsg, content: lastMsg.content + agentEvent.data.text }
              ];
            } else {
              // Create new assistant message
              return [...prev, { role: "assistant", content: agentEvent.data.text }];
            }
          });
          break;

        case "assistantMessage":
          // Final complete message
          setMessages((prev) => {
            const lastMsg = prev[prev.length - 1];
            if (lastMsg && lastMsg.role === "assistant") {
              // Replace with final version
              return [
                ...prev.slice(0, -1),
                { role: "assistant", content: agentEvent.data.text }
              ];
            } else {
              return [...prev, { role: "assistant", content: agentEvent.data.text }];
            }
          });
          setIsLoading(false);
          break;

        case "toolCall":
          setMessages((prev) => [
            ...prev,
            { role: "system", content: `🔧 ${agentEvent.data.name}(${agentEvent.data.argsJson})` }
          ]);
          break;
      }
    }
  });

  const handleStop = async () => {
    await execution.cancel();
    setIsLoading(false);
  };

  const handleRun = async () => {
    if (execution.isRunning || isLoading) return;

    const message = initialPrompt.trim() || "Hello";

    // Start the session
    const userMessage: Message = { role: "user", content: message };
    setMessages([userMessage]);
    setIsLoading(true);

    try {
      await execution.execute();
      await execution.sendInput(message);
    } catch (error: any) {
      const errorMessage: Message = {
        role: "assistant",
        content: `Error: ${error}`,
      };
      setMessages([userMessage, errorMessage]);
      setIsLoading(false);
    }
  };

  const handleSend = async () => {
    if (!input.trim() || isLoading || !execution.isRunning) return;

    const userMessage: Message = { role: "user", content: input };
    setMessages((prev) => [...prev, userMessage]);
    setInput("");
    setIsLoading(true);

    try {
      await execution.sendInput(input);
    } catch (error: any) {
      const errorMessage: Message = {
        role: "assistant",
        content: `Error: ${error}`,
      };
      setMessages((prev) => [...prev, errorMessage]);
      setIsLoading(false);
    }
  };

  return (
    <div className="w-full bg-gradient-to-r from-purple-50 to-blue-50 dark:from-slate-800 dark:to-blue-950 rounded-lg border border-purple-200 dark:border-purple-900 shadow-sm">
      <div className="flex items-center gap-2 p-3 border-b border-purple-200 dark:border-purple-800">
        <PlayButton
          onPlay={handleRun}
          onStop={handleStop}
          isRunning={execution.isRunning && isLoading}
          isLoading={isLoading}
          cancellable={true}
          disabled={!isEditable}
        />
        <Button isIconOnly variant="light" size="sm" className="bg-purple-100 dark:bg-purple-800 text-purple-600 dark:text-purple-300">
          <BotIcon className="h-4 w-4" />
        </Button>
        <div className="flex-1">
          <Textarea
            placeholder="Initial prompt for the agent..."
            value={initialPrompt}
            onChange={(e) => setInitialPrompt(e.target.value)}
            disabled={!isEditable || execution.isRunning}
            minRows={1}
            maxRows={3}
            size="sm"
            className="text-sm"
          />
        </div>
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
                className={`text-sm p-2 rounded ${msg.role === "user"
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
            classNames={{
              inputWrapper: "!ring-0 !ring-offset-0 focus-within:!ring-0"
            }}
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
