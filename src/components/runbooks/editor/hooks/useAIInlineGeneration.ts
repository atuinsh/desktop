import { useState, useCallback, useRef } from "react";
import { addToast } from "@heroui/react";
import { BlockNoteEditor } from "@blocknote/core";
import { AIFeatureDisabledError, AIQuotaExceededError } from "@/lib/ai/block_generator";
import { AISingleBlockResponse } from "@/api/ai";
import { incrementAIHintUseCount } from "../ui/AIHint";
import track_event from "@/tracking";
import useDocumentBridge from "@/lib/hooks/useDocumentBridge";

export interface EditorContext {
  documentMarkdown?: string;
  currentBlockId: string;
  currentBlockIndex: number;
  runbookId?: string;
}

export interface UseAIInlineGenerationOptions {
  editor: BlockNoteEditor | null;
  documentBridge: ReturnType<typeof useDocumentBridge>;
  getEditorContext: () => Promise<EditorContext | undefined>;
}

export interface UseAIInlineGenerationReturn {
  // Generation state
  isGeneratingInline: boolean;
  generatingBlockId: string | null;
  loadingStatus: "loading" | "cancelled";

  // Post-generation state
  postGenerationBlockId: string | null;
  generatedBlockIds: string[];
  generatedBlockCount: number;

  // Edit mode state
  isEditingGenerated: boolean;
  editPrompt: string;
  setEditPrompt: (value: string) => void;

  // Actions
  handleInlineGenerate: (block: any) => Promise<void>;
  clearPostGenerationMode: () => void;
  handleEditSubmit: () => Promise<void>;
  startEditing: () => void;
  cancelEditing: () => void;

  // For onChange integration (ref-based, doesn't trigger re-renders)
  getIsProgrammaticEdit: () => boolean;

  // Helper
  getBlockText: (block: any) => string;
}

export function useAIInlineGeneration({
  editor,
  documentBridge,
  getEditorContext,
}: UseAIInlineGenerationOptions): UseAIInlineGenerationReturn {
  // Inline AI generation state
  const [isGeneratingInline, setIsGeneratingInline] = useState(false);
  const [generatingBlockId, setGeneratingBlockId] = useState<string | null>(null);
  const [loadingStatus, setLoadingStatus] = useState<"loading" | "cancelled">("loading");
  const generatingBlockIdRef = useRef<string | null>(null); // For cancellation check
  const originalPromptRef = useRef<string | null>(null); // For cancellation detection
  const errorToastShownRef = useRef(false); // Prevent duplicate error toasts

  // Post-generation mode state - after AI generates a block, user can Cmd+Enter to run or Tab to continue
  const [postGenerationBlockId, setPostGenerationBlockId] = useState<string | null>(null);
  const [generatedBlockIds, setGeneratedBlockIds] = useState<string[]>([]);
  const [generatedBlockCount, setGeneratedBlockCount] = useState(0);

  // Edit mode state for follow-up adjustments
  const [isEditingGenerated, setIsEditingGenerated] = useState(false);
  const [editPrompt, setEditPrompt] = useState("");
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

      // Set up generation state
      setIsGeneratingInline(true);
      setGeneratingBlockId(block.id);
      setLoadingStatus("loading");
      generatingBlockIdRef.current = block.id;
      originalPromptRef.current = prompt;
      errorToastShownRef.current = false;

      try {
        const context = await getEditorContext();
        const { generateBlocks } = await import("@/lib/ai/block_generator");
        const lastBlockContext = await documentBridge?.getLastBlockContext();
        console.log("lastBlockContext", lastBlockContext);

        const result = await generateBlocks({
          prompt,
          documentMarkdown: context?.documentMarkdown,
          insertAfterIndex: context?.currentBlockIndex,
          runbookId: context?.runbookId,
          context: {
            variables: Object.keys(lastBlockContext?.variables ?? {}),
            named_blocks: [], // TODO: Implement named blocks
            environment_variables: Object.keys(lastBlockContext?.envVars ?? {}),
            working_directory: lastBlockContext?.cwd || null,
            ssh_host: lastBlockContext?.sshHost || null,
          },
        });

        // Check if the block was edited during generation (cancellation)
        const currentBlock = editor.document.find((b: any) => b.id === block.id);
        const currentBlockText = getBlockText(currentBlock);
        if (currentBlockText !== prompt) {
          // Block was edited, show cancelled state briefly then hide
          setLoadingStatus("cancelled");
          track_event("runbooks.ai.inline_generate_cancelled", {
            reason: "block_edited",
          });
          // Show "Cancelled" for 1.5 seconds
          await new Promise((resolve) => setTimeout(resolve, 1500));
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

        // Enter post-generation mode - user can Cmd+Enter to run or Tab to continue
        if (blocksToInsert.length > 0) {
          setPostGenerationBlockId(lastInsertedId);
          setGeneratedBlockIds(insertedIds);
          setGeneratedBlockCount(blocksToInsert.length);
        }

        track_event("runbooks.ai.inline_generate_success", {
          prompt_length: prompt.length,
          blocks_generated: blocksToInsert.length,
        });

        // Increment usage count for AI hint dismissal
        incrementAIHintUseCount();
      } catch (error) {
        // Prevent duplicate error toasts
        if (errorToastShownRef.current) {
          console.log("[AI] Duplicate toast prevented", new Error().stack);
          return;
        }
        errorToastShownRef.current = true;
        console.log("[AI] Showing error toast", new Error().stack);

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

        track_event("runbooks.ai.inline_generate_error", {
          error: message,
        });
      } finally {
        setIsGeneratingInline(false);
        setGeneratingBlockId(null);
        generatingBlockIdRef.current = null;
        originalPromptRef.current = null;
      }
    },
    [editor, getEditorContext, getBlockText, documentBridge]
  );

  // Clear post-generation mode
  const clearPostGenerationMode = useCallback(() => {
    setPostGenerationBlockId(null);
    setGeneratedBlockIds([]);
    setGeneratedBlockCount(0);
    setIsEditingGenerated(false);
    setEditPrompt("");
  }, []);

  // Handle edit submission for follow-up adjustments
  const handleEditSubmit = useCallback(async () => {
    if (!editor || !postGenerationBlockId || !editPrompt.trim() || generatedBlockIds.length === 0)
      return;

    const blockToEditId = generatedBlockIds[0];

    // Use the same loading overlay as initial generation
    setIsEditingGenerated(false);
    setIsGeneratingInline(true);
    setGeneratingBlockId(blockToEditId);
    setLoadingStatus("loading");

    try {
      // Get the current block to edit
      const currentBlock = editor.document.find((b: any) => b.id === blockToEditId);
      if (!currentBlock) {
        throw new Error("Block not found");
      }

      const context = await getEditorContext();
      const { generateOrEditBlock } = await import("@/api/ai");

      const result = (await generateOrEditBlock({
        action: "edit",
        block: currentBlock,
        instruction: editPrompt,
        document_markdown: context?.documentMarkdown,
        runbook_id: context?.runbookId,
      })) as AISingleBlockResponse;

      // Replace the block with the edited version
      const newBlock = { ...result.block, id: currentBlock.id };
      isProgrammaticEditRef.current = true;
      editor.updateBlock(currentBlock.id, newBlock as any);
      // Reset after microtask to ensure onChange has fired
      queueMicrotask(() => {
        isProgrammaticEditRef.current = false;
      });

      // Reset edit prompt but stay in post-generation mode
      setEditPrompt("");

      track_event("runbooks.ai.post_generation_edit", {
        blockType: currentBlock.type,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : "Failed to edit block";
      addToast({
        title: "Edit failed",
        description: message,
        color: "danger",
      });
      track_event("runbooks.ai.post_generation_edit_error", { error: message });
    } finally {
      // Clear loading state - this will show the focus overlay again
      setIsGeneratingInline(false);
      setGeneratingBlockId(null);
    }
  }, [editor, postGenerationBlockId, editPrompt, generatedBlockIds, getEditorContext]);

  // Start editing mode
  const startEditing = useCallback(() => {
    setIsEditingGenerated(true);
  }, []);

  // Cancel editing mode
  const cancelEditing = useCallback(() => {
    setIsEditingGenerated(false);
  }, []);

  // Get whether current edit is programmatic (ref-based for onChange)
  const getIsProgrammaticEdit = useCallback(() => {
    return isProgrammaticEditRef.current;
  }, []);

  return {
    // Generation state
    isGeneratingInline,
    generatingBlockId,
    loadingStatus,

    // Post-generation state
    postGenerationBlockId,
    generatedBlockIds,
    generatedBlockCount,

    // Edit mode state
    isEditingGenerated,
    editPrompt,
    setEditPrompt,

    // Actions
    handleInlineGenerate,
    clearPostGenerationMode,
    handleEditSubmit,
    startEditing,
    cancelEditing,

    // For onChange integration
    getIsProgrammaticEdit,

    // Helper
    getBlockText,
  };
}
