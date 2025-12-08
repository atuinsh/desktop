import { useEffect, useCallback } from "react";
import { addToast } from "@heroui/react";
import { executeBlock } from "@/lib/runtime";
import { calculateAIPopupPosition } from "../utils/popupPositioning";
import track_event from "@/tracking";

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

interface UseAIKeyboardShortcutsProps {
  editor: any;
  runbookId: string | undefined;
  // Post-generation state
  postGenerationBlockId: string | null;
  generatedBlockIds: string[];
  generatedBlockCount: number;
  isEditingGenerated: boolean;
  isGeneratingInline: boolean;
  // Callbacks
  onShowAIPopup: (position: { x: number; y: number }) => void;
  onShowEditPopup: (position: { x: number; y: number }, block: any) => void;
  onInlineGenerate: (block: any) => void;
  onClearPostGeneration: () => void;
  onStartEditing: () => void;
}

export function useAIKeyboardShortcuts({
  editor,
  runbookId,
  postGenerationBlockId,
  generatedBlockIds,
  generatedBlockCount,
  isEditingGenerated,
  isGeneratingInline,
  onShowAIPopup,
  onShowEditPopup,
  onInlineGenerate,
  onClearPostGeneration,
  onStartEditing,
}: UseAIKeyboardShortcutsProps) {
  const handlePostGenerationShortcuts = useCallback(
    (e: KeyboardEvent): boolean => {
      if (!postGenerationBlockId || !editor) return false;

      // Don't handle shortcuts while editing (except escape which is handled in the input)
      if (isEditingGenerated) return false;

      // Escape - dismiss and delete generated blocks
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        if (generatedBlockIds.length > 0) {
          editor.removeBlocks(generatedBlockIds);
        }
        onClearPostGeneration();
        track_event("runbooks.ai.post_generation_dismiss");
        return true;
      }

      // E - enter edit mode for follow-up adjustments
      if (e.key === "e" || e.key === "E") {
        e.preventDefault();
        e.stopPropagation();
        onStartEditing();
        track_event("runbooks.ai.post_generation_edit_start");
        return true;
      }

      // Tab - insert paragraph after generated block and continue writing
      if (e.key === "Tab" && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        e.stopPropagation();
        const newParagraph = editor.insertBlocks(
          [{ type: "paragraph", content: "" }],
          postGenerationBlockId,
          "after"
        );
        if (newParagraph?.[0]?.id) {
          editor.setTextCursorPosition(newParagraph[0].id, "start");
        }
        onClearPostGeneration();
        track_event("runbooks.ai.post_generation_continue");
        return true;
      }

      // Cmd+Enter - accept and run the generated block
      if (e.metaKey && e.key === "Enter") {
        e.preventDefault();
        e.stopPropagation();

        // Check if multiple blocks were generated
        if (generatedBlockCount > 1) {
          addToast({
            title: "Multiple blocks generated",
            description:
              "Running multiple blocks in series is not yet supported. Please run them individually.",
            color: "warning",
          });
          onClearPostGeneration();
          return true;
        }

        // Check if the block is executable
        const block = editor.document.find(
          (b: any) => b.id === postGenerationBlockId
        );
        if (block && EXECUTABLE_BLOCK_TYPES.includes(block.type)) {
          if (runbookId) {
            executeBlock(runbookId, postGenerationBlockId);
            track_event("runbooks.ai.post_generation_run", {
              blockType: block.type,
            });
          }
        } else {
          addToast({
            title: "Cannot run this block",
            description: `Block type "${block?.type || "unknown"}" is not executable.`,
            color: "warning",
          });
        }

        // Move cursor after the block and insert a new paragraph
        const newParagraph = editor.insertBlocks(
          [{ type: "paragraph", content: "" }],
          postGenerationBlockId,
          "after"
        );
        if (newParagraph?.[0]?.id) {
          editor.setTextCursorPosition(newParagraph[0].id, "start");
        }

        onClearPostGeneration();
        return true;
      }

      return false;
    },
    [
      editor,
      postGenerationBlockId,
      generatedBlockIds,
      generatedBlockCount,
      isEditingGenerated,
      runbookId,
      onClearPostGeneration,
      onStartEditing,
    ]
  );

  const handleAIShortcuts = useCallback(
    (e: KeyboardEvent): boolean => {
      if (!editor) return false;

      // Cmd+K or Cmd+Enter for AI operations
      if (!e.metaKey || (e.key !== "k" && e.key !== "Enter")) return false;

      try {
        const cursorPosition = editor.getTextCursorPosition();
        const currentBlock = cursorPosition.block;

        const isTextBlock = TEXT_BLOCK_TYPES.includes(currentBlock.type);
        const hasContent =
          currentBlock.content &&
          Array.isArray(currentBlock.content) &&
          currentBlock.content.length > 0;

        if (isTextBlock && hasContent) {
          // Text block with content → inline generation (Cmd+K or Cmd+Enter)
          e.preventDefault();
          if (!isGeneratingInline) {
            onClearPostGeneration();
            track_event("runbooks.ai.inline_generate_trigger", {
              shortcut: e.key === "k" ? "cmd-k" : "cmd-enter",
              blockType: currentBlock.type,
            });
            onInlineGenerate(currentBlock);
          }
          return true;
        } else if (e.key === "k" && isTextBlock && !hasContent) {
          // Empty text block + Cmd+K = show generate popup
          e.preventDefault();
          onClearPostGeneration();
          track_event("runbooks.ai.keyboard_shortcut");
          const position = calculateAIPopupPosition(editor, currentBlock.id);
          onShowAIPopup(position);
          return true;
        } else if (e.key === "k" && !isTextBlock) {
          // Non-text block + Cmd+K = edit popup
          e.preventDefault();
          onClearPostGeneration();
          track_event("runbooks.ai.edit_block", { blockType: currentBlock.type });
          const position = calculateAIPopupPosition(editor, currentBlock.id);
          onShowEditPopup(position, currentBlock);
          return true;
        }
      } catch (error) {
        console.warn("Could not get cursor position:", error);
      }

      return false;
    },
    [
      editor,
      isGeneratingInline,
      onClearPostGeneration,
      onInlineGenerate,
      onShowAIPopup,
      onShowEditPopup,
    ]
  );

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Try post-generation shortcuts first
      if (handlePostGenerationShortcuts(e)) return;

      // Then try AI shortcuts
      handleAIShortcuts(e);
    };

    // Use capture phase to intercept before BlockNote handles the event
    document.addEventListener("keydown", handleKeyDown, { capture: true });

    return () => {
      document.removeEventListener("keydown", handleKeyDown, { capture: true });
    };
  }, [handlePostGenerationShortcuts, handleAIShortcuts]);
}
