import { useReducer, useCallback, useRef } from "react";
import { addToast } from "@heroui/react";
import { BlockNoteEditor } from "@blocknote/core";
import { AIFeatureDisabledError, AIQuotaExceededError } from "@/lib/ai/block_generator";
import { AIMultiBlockResponse } from "@/api/ai";
import { incrementAIHintUseCount } from "../ui/AIHint";
import track_event from "@/tracking";
import useDocumentBridge from "@/lib/hooks/useDocumentBridge";

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
  | { status: "generating"; promptBlockId: string; originalPrompt: string }
  | { status: "cancelled" }
  | { status: "postGeneration"; generatedBlockIds: string[] }
  | { status: "editing"; generatedBlockIds: string[]; editPrompt: string }
  | { status: "submittingEdit"; generatedBlockIds: string[]; editPrompt: string };

// All possible actions
type Action =
  | { type: "START_GENERATE"; promptBlockId: string; originalPrompt: string }
  | { type: "GENERATION_CANCELLED" }
  | { type: "GENERATION_SUCCESS"; generatedBlockIds: string[] }
  | { type: "GENERATION_ERROR" }
  | { type: "FINISH_CANCELLED_DISPLAY" }
  | { type: "START_EDITING" }
  | { type: "UPDATE_EDIT_PROMPT"; editPrompt: string }
  | { type: "CANCEL_EDITING" }
  | { type: "SUBMIT_EDIT" }
  | { type: "EDIT_SUCCESS"; generatedBlockIds: string[] }
  | { type: "EDIT_ERROR" }
  | { type: "CLEAR" };

// =============================================================================
// Reducer
// =============================================================================

const initialState: InlineGenerationState = { status: "idle" };

function reducer(state: InlineGenerationState, action: Action): InlineGenerationState {
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
      };

    case "GENERATION_CANCELLED":
      if (state.status !== "generating") {
        console.warn(`[AIInlineGeneration] Cannot GENERATION_CANCELLED from state: ${state.status}`);
        return state;
      }
      return { status: "cancelled" };

    case "GENERATION_SUCCESS":
      if (state.status !== "generating") {
        console.warn(`[AIInlineGeneration] Cannot GENERATION_SUCCESS from state: ${state.status}`);
        return state;
      }
      return {
        status: "postGeneration",
        generatedBlockIds: action.generatedBlockIds,
      };

    case "GENERATION_ERROR":
      if (state.status !== "generating") {
        console.warn(`[AIInlineGeneration] Cannot GENERATION_ERROR from state: ${state.status}`);
        return state;
      }
      return { status: "idle" };

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
      return {
        status: "postGeneration",
        generatedBlockIds: state.generatedBlockIds,
      };

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
      };

    case "EDIT_SUCCESS":
      if (state.status !== "submittingEdit") {
        console.warn(`[AIInlineGeneration] Cannot EDIT_SUCCESS from state: ${state.status}`);
        return state;
      }
      return {
        status: "postGeneration",
        generatedBlockIds: action.generatedBlockIds,
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
      };

    case "CLEAR":
      // Can clear from postGeneration or editing
      if (state.status !== "postGeneration" && state.status !== "editing") {
        console.warn(`[AIInlineGeneration] Cannot CLEAR from state: ${state.status}`);
        return state;
      }
      return { status: "idle" };

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

// =============================================================================
// Hook interface
// =============================================================================

export interface UseAIInlineGenerationOptions {
  editor: BlockNoteEditor | null;
  documentBridge: ReturnType<typeof useDocumentBridge>;
  getEditorContext: () => Promise<EditorContext | undefined>;
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

  // Actions
  handleInlineGenerate: (block: any) => Promise<void>;
  clearPostGenerationMode: () => void;
  handleEditSubmit: () => Promise<void>;
  startEditing: () => void;
  cancelEditing: () => void;
  setEditPrompt: (value: string) => void;

  // For onChange integration (ref-based, doesn't trigger re-renders)
  getIsProgrammaticEdit: () => boolean;

  // Helper
  getBlockText: (block: any) => string;
}

// =============================================================================
// Hook implementation
// =============================================================================

export function useAIInlineGeneration({
  editor,
  documentBridge,
  getEditorContext,
}: UseAIInlineGenerationOptions): UseAIInlineGenerationReturn {
  const [state, dispatch] = useReducer(reducer, initialState);

  // Refs for async operation tracking
  const errorToastShownRef = useRef(false);
  const isProgrammaticEditRef = useRef(false);

  // Extract plain text from a BlockNote block's content
  const getBlockText = useCallback((block: any): string => {
    if (!block.content || !Array.isArray(block.content)) return "";
    return block.content
      .filter((item: any) => item.type === "text")
      .map((item: any) => item.text || "")
      .join("");
  }, []);

  // Handle inline AI generation from a paragraph block
  const handleInlineGenerate = useCallback(
    async (block: any) => {
      const prompt = getBlockText(block);
      if (!prompt.trim() || !editor) return;

      // Start generation
      dispatch({ type: "START_GENERATE", promptBlockId: block.id, originalPrompt: prompt });
      errorToastShownRef.current = false;

      try {
        const context = await getEditorContext();
        const { generateBlocks } = await import("@/lib/ai/block_generator");
        const lastBlockContext = await documentBridge?.getLastBlockContext();

        const result = await generateBlocks({
          prompt,
          documentMarkdown: context?.documentMarkdown,
          insertAfterIndex: context?.currentBlockIndex,
          runbookId: context?.runbookId,
          context: {
            variables: Object.keys(lastBlockContext?.variables ?? {}),
            named_blocks: [],
            environment_variables: Object.keys(lastBlockContext?.envVars ?? {}),
            working_directory: lastBlockContext?.cwd || null,
            ssh_host: lastBlockContext?.sshHost || null,
          },
        });

        // Check if the block was edited during generation (cancellation)
        const currentBlock = editor.document.find((b: any) => b.id === block.id);
        const currentBlockText = getBlockText(currentBlock);
        if (currentBlockText !== prompt) {
          dispatch({ type: "GENERATION_CANCELLED" });
          track_event("runbooks.ai.inline_generate_cancelled", { reason: "block_edited" });
          // Show "Cancelled" for 1.5 seconds, then return to idle
          setTimeout(() => dispatch({ type: "FINISH_CANCELLED_DISPLAY" }), 1500);
          return;
        }

        // Cap at 3 blocks
        const blocksToInsert = result.blocks.slice(0, 3);

        let lastInsertedId = block.id;
        const insertedIds: string[] = [];
        for (const newBlock of blocksToInsert) {
          const inserted = editor.insertBlocks([newBlock as any], lastInsertedId, "after");
          if (inserted?.[0]?.id) {
            lastInsertedId = inserted[0].id;
            insertedIds.push(inserted[0].id);
          }
        }

        // Move cursor to after the last inserted block
        if (lastInsertedId !== block.id) {
          editor.setTextCursorPosition(lastInsertedId, "end");
        }

        if (insertedIds.length > 0) {
          dispatch({ type: "GENERATION_SUCCESS", generatedBlockIds: insertedIds });
        } else {
          dispatch({ type: "GENERATION_ERROR" });
        }

        track_event("runbooks.ai.inline_generate_success", {
          prompt_length: prompt.length,
          blocks_generated: blocksToInsert.length,
        });

        incrementAIHintUseCount();
      } catch (error) {
        dispatch({ type: "GENERATION_ERROR" });

        // Prevent duplicate error toasts
        if (errorToastShownRef.current) return;
        errorToastShownRef.current = true;

        const message =
          error instanceof AIFeatureDisabledError
            ? "AI feature is not enabled for your account"
            : error instanceof AIQuotaExceededError
              ? "AI quota exceeded"
              : error instanceof Error
                ? error.message
                : "Failed to generate blocks";

        addToast({
          title: error instanceof AIQuotaExceededError ? "Quota exceeded" : "Generation failed",
          description: message,
          color: "danger",
        });

        track_event("runbooks.ai.inline_generate_error", { error: message });
      }
    },
    [editor, getEditorContext, getBlockText, documentBridge]
  );

  // Handle edit submission for follow-up adjustments
  const handleEditSubmit = useCallback(async () => {
    if (!editor || state.status !== "editing" || !state.editPrompt.trim()) return;

    const { generatedBlockIds, editPrompt } = state;

    dispatch({ type: "SUBMIT_EDIT" });

    try {
      const currentBlocks = editor.document.filter((b: any) => generatedBlockIds.includes(b.id));
      if (currentBlocks.length !== generatedBlockIds.length) {
        throw new Error("Blocks not found");
      }

      const context = await getEditorContext();
      const { generateOrEditBlock } = await import("@/api/ai");

      const result = (await generateOrEditBlock({
        action: "edit",
        blocks: currentBlocks,
        instruction: editPrompt,
        document_markdown: context?.documentMarkdown,
        runbook_id: context?.runbookId,
      })) as AIMultiBlockResponse;

      isProgrammaticEditRef.current = true;
      editor.replaceBlocks(generatedBlockIds, result.blocks as any[]);
      queueMicrotask(() => {
        isProgrammaticEditRef.current = false;
      });

      dispatch({
        type: "EDIT_SUCCESS",
        generatedBlockIds: result.blocks.map((b: any) => b.id),
      });
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
  }, [editor, state, getEditorContext]);

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

    handleInlineGenerate,
    clearPostGenerationMode,
    handleEditSubmit,
    startEditing,
    cancelEditing,
    setEditPrompt,

    getIsProgrammaticEdit,
    getBlockText,
  };
}
