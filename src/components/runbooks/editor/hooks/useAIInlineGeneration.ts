import React, { useCallback, useRef, useEffect } from "react";
import { addToast } from "@heroui/react";
import { BlockNoteEditor } from "@blocknote/core";
import { None } from "@binarymuse/ts-stdlib";
import { incrementAIHintUseCount } from "../ui/AIHint";
import track_event from "@/tracking";
import useDocumentBridge from "@/lib/hooks/useDocumentBridge";
import { executeBlock } from "@/lib/runtime";
import useReducerWithEffects, { UseReducerWithEffectsReducerReturn } from "@/lib/hooks/useReducerWithEffects";
import { ChargeTarget } from "@/rs-bindings/ChargeTarget";
import {
  createGeneratorSession,
  subscribeSession,
  sendMessage,
  sendEditRequest,
  sendToolResult,
  destroySession,
} from "@/lib/ai/commands";
import AIBlockRegistry from "@/lib/ai/block_registry";
import AtuinEnv from "@/atuin_env";
import { SessionEvent } from "@/rs-bindings/SessionEvent";
import { AIToolRunner, DEFAULT_AUTO_APPROVE_TOOLS } from "@/lib/ai/tools";

// =============================================================================
// Types
// =============================================================================

export interface EditorContext {
  documentMarkdown?: string;
  currentBlockId: string;
  currentBlockIndex: number;
  runbookId?: string;
}

// Discriminated union for all possible states
export type InlineGenerationState =
  | { status: "idle" }
  | { status: "generating"; promptBlockId: string; originalPrompt: string; sessionId: string; replacePromptBlock: boolean }
  | { status: "cancelled" }
  | { status: "postGeneration"; generatedBlockIds: string[]; sessionId: string; toolCallId: string }
  | { status: "editing"; generatedBlockIds: string[]; editPrompt: string; sessionId: string; toolCallId: string }
  | { status: "submittingEdit"; generatedBlockIds: string[]; editPrompt: string; sessionId: string; toolCallId: string };

// All possible actions
type Action =
  | { type: "START_GENERATE"; promptBlockId: string; originalPrompt: string; sessionId: string; replacePromptBlock: boolean }
  | { type: "GENERATION_CANCELLED" }
  | { type: "GENERATION_SUCCESS"; generatedBlockIds: string[]; toolCallId: string }
  | { type: "GENERATION_ERROR" }
  | { type: "FINISH_CANCELLED_DISPLAY" }
  | { type: "START_EDITING" }
  | { type: "UPDATE_EDIT_PROMPT"; editPrompt: string }
  | { type: "CANCEL_EDITING" }
  | { type: "SUBMIT_EDIT" }
  | { type: "EDIT_SUCCESS"; generatedBlockIds: string[]; toolCallId: string }
  | { type: "EDIT_ERROR" }
  | { type: "CLEAR" };

type Effects =
  | { type: "focusEditor" }
  | { type: "destroySession"; sessionId: string }

// Block types that have inline text content (can be used as prompts)
const TEXT_BLOCK_TYPES = [
  "paragraph",
  "heading",
  "bulletListItem",
  "numberedListItem",
  "checkListItem",
];

// Block types that can be executed
const EXECUTABLE_BLOCK_TYPES = [
  "run",
  "script",
  "postgres",
  "sqlite",
  "mysql",
  "clickhouse",
  "http",
  "prometheus",
  "kubernetes-get",
];

// =============================================================================
// Reducer
// =============================================================================

const initialState: InlineGenerationState = { status: "idle" };

function reducer(state: InlineGenerationState, action: Action): UseReducerWithEffectsReducerReturn<InlineGenerationState, Effects> {
  switch (action.type) {
    case "START_GENERATE":
      // Can only start generating from idle
      if (state.status !== "idle") {
        console.warn(`[AIInlineGeneration] Cannot START_GENERATE from state: ${state.status}`);
        return state;
      }
      return {
        status: "generating",
        promptBlockId: action.promptBlockId,
        originalPrompt: action.originalPrompt,
        sessionId: action.sessionId,
        replacePromptBlock: action.replacePromptBlock,
      };

    case "GENERATION_CANCELLED":
      if (state.status !== "generating") {
        console.warn(`[AIInlineGeneration] Cannot GENERATION_CANCELLED from state: ${state.status}`);
        return state;
      }
      return [
        { status: "cancelled" },
        [{ type: "destroySession", sessionId: state.sessionId }],
      ];

    case "GENERATION_SUCCESS":
      if (state.status !== "generating") {
        console.warn(`[AIInlineGeneration] Cannot GENERATION_SUCCESS from state: ${state.status}`);
        return state;
      }
      return {
        status: "postGeneration",
        generatedBlockIds: action.generatedBlockIds,
        sessionId: state.sessionId,
        toolCallId: action.toolCallId,
      };

    case "GENERATION_ERROR":
      if (state.status !== "generating") {
        console.warn(`[AIInlineGeneration] Cannot GENERATION_ERROR from state: ${state.status}`);
        return state;
      }
      return [
        { status: "idle" },
        [{ type: "destroySession", sessionId: state.sessionId }],
      ];

    case "FINISH_CANCELLED_DISPLAY":
      if (state.status !== "cancelled") {
        console.warn(`[AIInlineGeneration] Cannot FINISH_CANCELLED_DISPLAY from state: ${state.status}`);
        return state;
      }
      return { status: "idle" };

    case "START_EDITING":
      if (state.status !== "postGeneration") {
        console.warn(`[AIInlineGeneration] Cannot START_EDITING from state: ${state.status}`);
        return state;
      }
      return {
        status: "editing",
        generatedBlockIds: state.generatedBlockIds,
        editPrompt: "",
        sessionId: state.sessionId,
        toolCallId: state.toolCallId,
      };

    case "UPDATE_EDIT_PROMPT":
      if (state.status !== "editing") {
        console.warn(`[AIInlineGeneration] Cannot UPDATE_EDIT_PROMPT from state: ${state.status}`);
        return state;
      }
      return {
        ...state,
        editPrompt: action.editPrompt,
      };

    case "CANCEL_EDITING":
      if (state.status !== "editing") {
        console.warn(`[AIInlineGeneration] Cannot CANCEL_EDITING from state: ${state.status}`);
        return state;
      }
      return [{
        status: "postGeneration",
        generatedBlockIds: state.generatedBlockIds,
        sessionId: state.sessionId,
        toolCallId: state.toolCallId,
      }, [{ type: "focusEditor" }]];

    case "SUBMIT_EDIT":
      if (state.status !== "editing") {
        console.warn(`[AIInlineGeneration] Cannot SUBMIT_EDIT from state: ${state.status}`);
        return state;
      }
      if (!state.editPrompt.trim()) {
        console.warn(`[AIInlineGeneration] Cannot SUBMIT_EDIT with empty prompt`);
        return state;
      }
      return {
        status: "submittingEdit",
        generatedBlockIds: state.generatedBlockIds,
        editPrompt: state.editPrompt,
        sessionId: state.sessionId,
        toolCallId: state.toolCallId,
      };

    case "EDIT_SUCCESS":
      if (state.status !== "submittingEdit") {
        console.warn(`[AIInlineGeneration] Cannot EDIT_SUCCESS from state: ${state.status}`);
        return state;
      }
      return {
        status: "postGeneration",
        generatedBlockIds: action.generatedBlockIds,
        sessionId: state.sessionId,
        toolCallId: action.toolCallId,
      };

    case "EDIT_ERROR":
      if (state.status !== "submittingEdit") {
        console.warn(`[AIInlineGeneration] Cannot EDIT_ERROR from state: ${state.status}`);
        return state;
      }
      // Return to editing state with prompt preserved
      return {
        status: "editing",
        generatedBlockIds: state.generatedBlockIds,
        editPrompt: state.editPrompt,
        sessionId: state.sessionId,
        toolCallId: state.toolCallId,
      };

    case "CLEAR": {
      // Can clear from postGeneration or editing - destroy session
      if (state.status === "postGeneration" || state.status === "editing") {
        return [
          { status: "idle" },
          [{ type: "destroySession", sessionId: state.sessionId }],
        ];
      }
      // Silent - this can happen legitimately when blocks are deleted
      return state;
    }

    default:
      return state;
  }
}

// =============================================================================
// Derived state helpers
// =============================================================================

function getGeneratedBlockIds(state: InlineGenerationState): string[] {
  switch (state.status) {
    case "postGeneration":
    case "editing":
    case "submittingEdit":
      return state.generatedBlockIds;
    default:
      return [];
  }
}

function getGeneratingBlockIds(state: InlineGenerationState): string[] | null {
  switch (state.status) {
    case "generating":
      return [state.promptBlockId];
    case "submittingEdit":
      return state.generatedBlockIds;
    default:
      return null;
  }
}

function getEditPrompt(state: InlineGenerationState): string {
  switch (state.status) {
    case "editing":
    case "submittingEdit":
      return state.editPrompt;
    default:
      return "";
  }
}

function getSessionId(state: InlineGenerationState): string | null {
  switch (state.status) {
    case "generating":
    case "postGeneration":
    case "editing":
    case "submittingEdit":
      return state.sessionId;
    default:
      return null;
  }
}

// =============================================================================
// Hook interface
// =============================================================================

export interface UseAIInlineGenerationOptions {
  editor: BlockNoteEditor | null;
  runbookId: string | undefined;
  documentBridge: ReturnType<typeof useDocumentBridge>;
  getEditorContext: () => Promise<EditorContext | undefined>;
  username: string;
  chargeTarget: ChargeTarget;
}

export interface UseAIInlineGenerationReturn {
  // State (derived from state machine)
  state: InlineGenerationState;
  isGenerating: boolean;
  generatingBlockIds: string[] | null;
  generatedBlockIds: string[];
  isEditing: boolean;
  editPrompt: string;
  loadingStatus: "loading" | "cancelled";

  // Actions (exposed for UI components like AIFocusOverlay)
  clearPostGenerationMode: () => void;
  handleEditSubmit: () => Promise<void>;
  startEditing: () => void;
  cancelEditing: () => void;
  setEditPrompt: (value: string) => void;

  // Start generation with a prompt string (for Cmd+K popup flow)
  // replacePromptBlock: if true, deletes the insertAfterBlockId after inserting generated blocks
  startGenerationWithPrompt: (prompt: string, insertAfterBlockId: string, replacePromptBlock?: boolean) => Promise<void>;

  // For onChange integration (ref-based, doesn't trigger re-renders)
  getIsProgrammaticEdit: () => boolean;
  hasGeneratedBlocks: () => boolean;

  // Keyboard handler to be called from BlockNoteView's onKeyDownCapture
  handleKeyDown: (e: React.KeyboardEvent) => void;

  // Helper
  getBlockText: (block: any) => string;
}

// =============================================================================
// Hook implementation
// =============================================================================

export function useAIInlineGeneration({
  editor,
  runbookId,
  documentBridge: _documentBridge,
  getEditorContext: _getEditorContext,
  username,
  chargeTarget,
}: UseAIInlineGenerationOptions): UseAIInlineGenerationReturn {
  const runEffect = useCallback((effect: Effects) => {
    console.log("[AIInlineGeneration] Running effect:", effect);
    if (effect.type === "focusEditor") {
      editor?.focus();
    } else if (effect.type === "destroySession") {
      destroySession(effect.sessionId).catch((err) => {
        console.error("[AIInlineGeneration] Failed to destroy session:", err);
      });
    }
  }, [editor]);

  const [state, dispatch] = useReducerWithEffects(reducer, initialState, runEffect);

  // Tool runner for executing AI tools - auto-approve all read-only tools for inline generation
  const toolRunnerRef = useRef<AIToolRunner>(new AIToolRunner(DEFAULT_AUTO_APPROVE_TOOLS.concat(["get_runbook_document"])));

  // Keep state and handlers in refs so keyboard handlers always have current values
  const stateRef = useRef(state);
  stateRef.current = state;

  const editorRef = useRef(editor);
  editorRef.current = editor;

  // Keep tool runner's editor reference up to date
  useEffect(() => {
    toolRunnerRef.current.setEditor(editor);
  }, [editor]);

  const handleInlineGenerateRef = useRef<(block: any) => Promise<void>>(null as any);

  // Refs for async operation tracking
  const errorToastShownRef = useRef(false);
  const isProgrammaticEditRef = useRef(false);

  // Track the prompt block ID so we can check for cancellation
  const promptBlockIdRef = useRef<string | null>(null);
  const originalPromptRef = useRef<string | null>(null);
  const replacePromptBlockRef = useRef<boolean>(false);
  // Track if we need to insert at the beginning of the document (when prompt block was first block)
  const insertAtBeginningRef = useRef<boolean>(false);

  // Extract plain text from a BlockNote block's content
  const getBlockText = useCallback((block: any): string => {
    if (!block.content || !Array.isArray(block.content)) return "";
    return block.content
      .filter((item: any) => item.type === "text")
      .map((item: any) => item.text || "")
      .join("");
  }, []);

  // Handle session events
  const handleSessionEvent = useCallback(
    (event: SessionEvent) => {
      console.log("[AIInlineGeneration] Session event:", event.type, event);
      const currentEditor = editorRef.current;
      const currentState = stateRef.current;

      switch (event.type) {
        case "blocksGenerated": {
          if (currentState.status !== "generating" && currentState.status !== "submittingEdit") {
            console.warn("[AIInlineGeneration] Received blocksGenerated in unexpected state:", currentState.status);
            return;
          }

          // Check if the block was edited during generation (cancellation)
          // Skip this check for Cmd+K flow (replacePromptBlock=true) since the prompt came from the popup, not the block
          if (currentState.status === "generating" && !currentState.replacePromptBlock && promptBlockIdRef.current && currentEditor) {
            const currentBlock = currentEditor.document.find((b: any) => b.id === promptBlockIdRef.current);
            const currentBlockText = currentBlock ? getBlockText(currentBlock) : null;
            if (currentBlockText !== originalPromptRef.current) {
              dispatch({ type: "GENERATION_CANCELLED" });
              track_event("runbooks.ai.inline_generate_cancelled", { reason: "block_edited" });
              // Show "Cancelled" for 1.5 seconds, then return to idle
              setTimeout(() => dispatch({ type: "FINISH_CANCELLED_DISPLAY" }), 1500);
              return;
            }
          }

          const blocks = event.blocks as Array<{ type: string; props?: Record<string, unknown> }>;
          const toolCallId = event.toolCallId;

          if (!currentEditor) {
            console.error("[AIInlineGeneration] No editor available for block insertion");
            dispatch({ type: "GENERATION_ERROR" });
            return;
          }

          // For edits, remove old blocks first
          if (currentState.status === "submittingEdit") {
            isProgrammaticEditRef.current = true;
            currentEditor.removeBlocks(currentState.generatedBlockIds);
          }

          // Cap at 3 blocks
          const blocksToInsert = blocks.slice(0, 3);

          // Determine insertion point and method
          let insertAfterBlockId: string | null;
          let insertAtBeginning = false;

          if (currentState.status === "generating") {
            insertAfterBlockId = currentState.promptBlockId;
          } else {
            // For edits, insert after the block before the first removed block
            // If insertAtBeginningRef is true, we need to insert before the first document block
            if (insertAtBeginningRef.current) {
              insertAtBeginning = true;
              insertAfterBlockId = currentEditor.document[0]?.id || null;
            } else {
              insertAfterBlockId = promptBlockIdRef.current || currentEditor.document[0]?.id;
            }
          }

          let lastInsertedId: string | null = insertAfterBlockId;
          const insertedIds: string[] = [];
          for (let i = 0; i < blocksToInsert.length; i++) {
            const newBlock = blocksToInsert[i];
            // For the first block when inserting at beginning, use "before"; otherwise "after"
            const position = (insertAtBeginning && i === 0) ? "before" : "after";
            const referenceId = lastInsertedId || currentEditor.document[0]?.id;
            if (!referenceId) {
              console.error("[AIInlineGeneration] No reference block for insertion");
              break;
            }
            const inserted = currentEditor.insertBlocks([newBlock as any], referenceId, position);
            if (inserted?.[0]?.id) {
              lastInsertedId = inserted[0].id;
              insertedIds.push(inserted[0].id);
            }
          }

          if (currentState.status === "submittingEdit") {
            queueMicrotask(() => {
              isProgrammaticEditRef.current = false;
            });
          }

          // If replacePromptBlock is set (Cmd+K flow), delete the original empty block
          // and update promptBlockIdRef to point to the block before it (for subsequent edits)
          if (currentState.status === "generating" && replacePromptBlockRef.current && promptBlockIdRef.current) {
            const promptBlockIndex = currentEditor.document.findIndex((b: any) => b.id === promptBlockIdRef.current);
            const blockBeforePrompt = promptBlockIndex > 0 ? currentEditor.document[promptBlockIndex - 1] : null;

            isProgrammaticEditRef.current = true;
            currentEditor.removeBlocks([promptBlockIdRef.current]);
            queueMicrotask(() => {
              isProgrammaticEditRef.current = false;
            });

            // Update refs so subsequent edits insert in the right place
            if (blockBeforePrompt) {
              promptBlockIdRef.current = blockBeforePrompt.id;
              insertAtBeginningRef.current = false;
            } else {
              // Prompt block was the first block - need to insert at beginning for edits
              promptBlockIdRef.current = null;
              insertAtBeginningRef.current = true;
            }
          }

          // Move cursor to after the last inserted block
          if (lastInsertedId && lastInsertedId !== insertAfterBlockId) {
            currentEditor.setTextCursorPosition(lastInsertedId, "end");
          }

          if (insertedIds.length > 0) {
            if (currentState.status === "generating") {
              dispatch({ type: "GENERATION_SUCCESS", generatedBlockIds: insertedIds, toolCallId });
              track_event("runbooks.ai.inline_generate_success", {
                prompt_length: originalPromptRef.current?.length || 0,
                blocks_generated: blocksToInsert.length,
              });
              incrementAIHintUseCount();
            } else {
              dispatch({ type: "EDIT_SUCCESS", generatedBlockIds: insertedIds, toolCallId });
              track_event("runbooks.ai.post_generation_edit_success", {
                blocks_generated: blocksToInsert.length,
              });
            }
          } else {
            if (currentState.status === "generating") {
              dispatch({ type: "GENERATION_ERROR" });
            } else {
              dispatch({ type: "EDIT_ERROR" });
            }
          }
          break;
        }

        case "error": {
          const message = event.message || "Unknown error";
          console.error("[AIInlineGeneration] Session error:", message);

          if (currentState.status === "generating") {
            dispatch({ type: "GENERATION_ERROR" });
          } else if (currentState.status === "submittingEdit") {
            dispatch({ type: "EDIT_ERROR" });
          }

          // Prevent duplicate error toasts
          if (!errorToastShownRef.current) {
            errorToastShownRef.current = true;
            addToast({
              title: "Generation failed",
              description: message,
              color: "danger",
            });
          }

          track_event("runbooks.ai.inline_generate_error", { error: message });
          break;
        }

        case "cancelled": {
          if (currentState.status === "generating") {
            dispatch({ type: "GENERATION_CANCELLED" });
            track_event("runbooks.ai.inline_generate_cancelled", { reason: "session_cancelled" });
            setTimeout(() => dispatch({ type: "FINISH_CANCELLED_DISPLAY" }), 1500);
          }
          break;
        }

        case "toolsRequested": {
          // Auto-execute tools for inline generation using the tool runner
          const toolCalls = event.calls;
          const sessionId = getSessionId(currentState);
          const toolRunner = toolRunnerRef.current;

          if (!sessionId) {
            console.error("[AIInlineGeneration] No session for tool execution");
            return;
          }

          // Execute each tool and send result
          for (const toolCall of toolCalls) {
            if (toolRunner.isAutoApprovable(toolCall.name)) {
              console.log(`[AIInlineGeneration] Auto-executing tool: ${toolCall.name}`);
              toolRunner
                .executeToolCall(toolCall)
                .then((result) => {
                  sendToolResult(sessionId, toolCall.id, result.success, result.result).catch(
                    (err) => console.error("[AIInlineGeneration] Failed to send tool result:", err)
                  );
                })
                .catch((err) => {
                  console.error(`[AIInlineGeneration] Failed to execute tool ${toolCall.name}:`, err);
                  sendToolResult(sessionId, toolCall.id, false, err.message).catch(
                    (err2) => console.error("[AIInlineGeneration] Failed to send error result:", err2)
                  );
                });
            } else {
              console.warn(`[AIInlineGeneration] Tool ${toolCall.name} is not auto-approvable, sending error`);
              sendToolResult(
                sessionId,
                toolCall.id,
                false,
                `Tool ${toolCall.name} is not available for inline generation`
              ).catch((err) => console.error("[AIInlineGeneration] Failed to send error result:", err));
            }
          }
          break;
        }

        // Other events we don't need to handle for inline generation
        default:
          break;
      }
    },
    [getBlockText]
  );

  // Core generation logic shared between inline (Cmd+Enter) and popup (Cmd+K) flows
  const startGeneration = useCallback(
    async (prompt: string, blockId: string, replacePromptBlock: boolean) => {
      if (!prompt.trim() || !editor || !runbookId) return;

      // Cancel any existing session
      const existingSessionId = getSessionId(stateRef.current);
      if (existingSessionId) {
        await destroySession(existingSessionId).catch(console.error);
      }

      errorToastShownRef.current = false;
      promptBlockIdRef.current = blockId;
      originalPromptRef.current = prompt;
      replacePromptBlockRef.current = replacePromptBlock;

      try {
        const blockRegistry = AIBlockRegistry.getInstance();
        const hubEndpoint = AtuinEnv.url("/api/ai/proxy/");

        // Create generator session
        const sessionId = await createGeneratorSession(
          runbookId,
          None, // Use default model
          blockRegistry.getBlockInfos(),
          editor.document,
          blockId,
          username,
          chargeTarget,
          hubEndpoint,
        );

        // Update state with session ID
        dispatch({ type: "START_GENERATE", promptBlockId: blockId, originalPrompt: prompt, sessionId, replacePromptBlock });

        // Subscribe to session events
        await subscribeSession(sessionId, handleSessionEvent);

        // Send the prompt as user message to start generation
        await sendMessage(sessionId, prompt);

      } catch (error) {
        console.error("[AIInlineGeneration] Failed to create session:", error);
        dispatch({ type: "GENERATION_ERROR" });

        const message = error instanceof Error ? error.message : "Failed to start generation";
        addToast({
          title: "Generation failed",
          description: message,
          color: "danger",
        });

        track_event("runbooks.ai.inline_generate_error", { error: message });
      }
    },
    [editor, runbookId, username, chargeTarget, handleSessionEvent]
  );

  // Handle inline AI generation from a paragraph block (Cmd+Enter flow)
  const handleInlineGenerate = useCallback(
    async (block: any) => {
      const prompt = getBlockText(block);
      await startGeneration(prompt, block.id, false);
    },
    [getBlockText, startGeneration]
  );

  // Start generation with a prompt string (Cmd+K popup flow)
  const startGenerationWithPrompt = useCallback(
    async (prompt: string, insertAfterBlockId: string, replacePromptBlock: boolean = false) => {
      await startGeneration(prompt, insertAfterBlockId, replacePromptBlock);
    },
    [startGeneration]
  );

  // Update ref so keyboard handler always has current function
  handleInlineGenerateRef.current = handleInlineGenerate;

  // Handle edit submission for follow-up adjustments
  const handleEditSubmit = useCallback(async () => {
    const currentState = stateRef.current;
    if (!editor || currentState.status !== "editing" || !currentState.editPrompt.trim()) return;

    const { sessionId, toolCallId, editPrompt } = currentState;

    dispatch({ type: "SUBMIT_EDIT" });

    try {
      // Send edit request to session - this will update system prompt and continue conversation
      await sendEditRequest(sessionId, editPrompt, toolCallId);
      // The session will emit BlocksGenerated event when done
    } catch (error) {
      dispatch({ type: "EDIT_ERROR" });

      const message = error instanceof Error ? error.message : "Failed to edit block";
      addToast({
        title: "Edit failed",
        description: message,
        color: "danger",
      });
      track_event("runbooks.ai.post_generation_edit_error", { error: message });
    }
  }, [editor]);

  // Simple action dispatchers
  const clearPostGenerationMode = useCallback(() => {
    dispatch({ type: "CLEAR" });
  }, []);

  const startEditing = useCallback(() => {
    dispatch({ type: "START_EDITING" });
  }, []);

  const cancelEditing = useCallback(() => {
    dispatch({ type: "CANCEL_EDITING" });
  }, []);

  const setEditPrompt = useCallback((value: string) => {
    dispatch({ type: "UPDATE_EDIT_PROMPT", editPrompt: value });
  }, []);

  const getIsProgrammaticEdit = useCallback(() => {
    return isProgrammaticEditRef.current;
  }, []);

  const hasGeneratedBlocks = useCallback(() => {
    return getGeneratedBlockIds(stateRef.current).length > 0;
  }, []);

  // =============================================================================
  // Keyboard handling - called from BlockNoteView's onKeyDownCapture
  // =============================================================================

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!editor) return;

      const currentState = stateRef.current;

      // Handle post-generation shortcuts
      if (currentState.status === "postGeneration") {
        const { generatedBlockIds } = currentState;

        // Check if blocks still exist
        const blocksExist =
          generatedBlockIds.length > 0 &&
          generatedBlockIds.every((id) => editor.document.some((b: any) => b.id === id));

        if (!blocksExist) {
          dispatch({ type: "CLEAR" });
          // Don't return - let the event fall through to generation handling
        } else {
          // Escape - dismiss and delete generated blocks
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            editor.removeBlocks(generatedBlockIds);
            dispatch({ type: "CLEAR" });
            track_event("runbooks.ai.post_generation_dismiss");
            return;
          }

          // E - enter edit mode
          if (e.key === "e" || e.key === "E") {
            e.preventDefault();
            e.stopPropagation();
            dispatch({ type: "START_EDITING" });
            track_event("runbooks.ai.post_generation_edit_start");
            return;
          }

          // Tab - accept and continue
          if (e.key === "Tab" && !e.metaKey && !e.ctrlKey && !e.altKey) {
            e.preventDefault();
            e.stopPropagation();
            const lastBlockId = generatedBlockIds[generatedBlockIds.length - 1];
            const newParagraph = editor.insertBlocks(
              [{ type: "paragraph", content: "" }],
              lastBlockId,
              "after"
            );
            if (newParagraph?.[0]?.id) {
              editor.setTextCursorPosition(newParagraph[0].id, "start");
            }
            dispatch({ type: "CLEAR" });
            track_event("runbooks.ai.post_generation_continue");
            return;
          }

          // Cmd+Enter - run the generated block
          if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
            e.preventDefault();
            e.stopPropagation();

            if (generatedBlockIds.length > 1) {
              addToast({
                title: "Multiple blocks generated",
                description:
                  "Running multiple blocks in series is not yet supported. Please run them individually.",
                color: "warning",
              });
              dispatch({ type: "CLEAR" });
              return;
            }

            const blockId = generatedBlockIds[0];
            const block = editor.document.find((b: any) => b.id === blockId);

            if (block && EXECUTABLE_BLOCK_TYPES.includes(block.type)) {
              if (runbookId) {
                executeBlock(runbookId, blockId);
                track_event("runbooks.ai.post_generation_run", { blockType: block.type });
              }
            } else {
              addToast({
                title: "Cannot run this block",
                description: `Block type "${block?.type || "unknown"}" is not executable.`,
                color: "warning",
              });
            }

            // Insert paragraph after and clear
            const newParagraph = editor.insertBlocks(
              [{ type: "paragraph", content: "" }],
              blockId,
              "after"
            );
            if (newParagraph?.[0]?.id) {
              editor.setTextCursorPosition(newParagraph[0].id, "start");
            }
            dispatch({ type: "CLEAR" });
            return;
          }
        }
      }

      // Handle editing state - only Escape to cancel (Enter is handled by input)
      if (currentState.status === "editing") {
        // Don't intercept keyboard events while editing - let the input handle them
        return;
      }

      // Handle Cmd+Enter to start generation (only when idle)
      if (currentState.status === "idle" && (e.metaKey || e.ctrlKey) && e.key === "Enter") {
        try {
          const cursorPosition = editor.getTextCursorPosition();
          const currentBlock = cursorPosition.block;

          const isTextBlock = TEXT_BLOCK_TYPES.includes(currentBlock.type);
          const hasContent =
            currentBlock.content &&
            Array.isArray(currentBlock.content) &&
            currentBlock.content.length > 0;

          if (isTextBlock && hasContent) {
            e.preventDefault();
            e.stopPropagation();
            track_event("runbooks.ai.inline_generate_trigger", {
              shortcut: "cmd-enter",
              blockType: currentBlock.type,
            });
            handleInlineGenerateRef.current(currentBlock);
            return;
          }
        } catch (error) {
          console.warn("Could not get cursor position:", error);
        }
      }
    },
    [editor, runbookId]
  );

  // Cleanup session on unmount
  useEffect(() => {
    return () => {
      const sessionId = getSessionId(stateRef.current);
      if (sessionId) {
        destroySession(sessionId).catch(console.error);
      }
    };
  }, []);

  // Derive values from state
  const isGenerating = state.status === "generating" || state.status === "submittingEdit";
  const generatingBlockIds = getGeneratingBlockIds(state);
  const generatedBlockIds = getGeneratedBlockIds(state);
  const isEditing = state.status === "editing";
  const editPrompt = getEditPrompt(state);
  const loadingStatus: "loading" | "cancelled" = state.status === "cancelled" ? "cancelled" : "loading";

  return {
    state,
    isGenerating,
    generatingBlockIds,
    generatedBlockIds,
    isEditing,
    editPrompt,
    loadingStatus,

    clearPostGenerationMode,
    handleEditSubmit,
    startEditing,
    cancelEditing,
    setEditPrompt,
    startGenerationWithPrompt,

    getIsProgrammaticEdit,
    hasGeneratedBlocks,
    handleKeyDown,
    getBlockText,
  };
}
