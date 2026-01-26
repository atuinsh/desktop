import React, { useCallback, useState, useImperativeHandle, forwardRef } from "react";
import { BlockNoteEditor } from "@blocknote/core";
import { SparklesIcon } from "lucide-react";
import { AIGeneratePopup } from "./AIGeneratePopup";
import { AILoadingOverlay } from "./ui/AILoadingBlock";
import { AIFocusOverlay } from "./ui/AIFocusOverlay";
import { AIHint } from "./ui/AIHint";
import { useAIKeyboardShortcuts } from "./hooks/useAIKeyboardShortcuts";
import { useAIInlineGeneration } from "./hooks/useAIInlineGeneration";
import { calculateAIPopupPosition } from "./utils/popupPositioning";
import useDocumentBridge from "@/lib/hooks/useDocumentBridge";
import { ChargeTarget } from "@/rs-bindings/ChargeTarget";
import track_event from "@/tracking";

// =============================================================================
// Types
// =============================================================================

export interface EditorContext {
  documentMarkdown?: string;
  currentBlockId: string;
  currentBlockIndex: number;
  runbookId?: string;
}

export interface EditorAIFeaturesProps {
  editor: BlockNoteEditor | null;
  runbookId: string | undefined;
  documentBridge: ReturnType<typeof useDocumentBridge>;
  aiShareContext: boolean;
  username: string;
  chargeTarget: ChargeTarget;
  showHint: boolean;
}

export interface EditorAIFeaturesHandle {
  /** Keyboard handler for BlockNoteView's onKeyDownCapture */
  handleKeyDown: (e: React.KeyboardEvent) => void;
  /** Check if there are generated blocks (for clearing on user interaction) */
  hasGeneratedBlocks: () => boolean;
  /** Check if current edit is programmatic (to avoid clearing on AI edits) */
  getIsProgrammaticEdit: () => boolean;
  /** Clear the post-generation mode */
  clearPostGenerationMode: () => void;
  /** Show the AI popup at a position for a block (for slash menu) */
  showAIPopup: (position: { x: number; y: number }, blockId: string) => void;
}

// =============================================================================
// Component
// =============================================================================

export const EditorAIFeatures = forwardRef<EditorAIFeaturesHandle, EditorAIFeaturesProps>(
  function EditorAIFeatures(
    {
      editor,
      runbookId,
      documentBridge,
      aiShareContext,
      username,
      chargeTarget,
      showHint,
    },
    ref,
  ) {
    // =========================================================================
    // Popup State
    // =========================================================================

    const [aiPopupVisible, setAiPopupVisible] = useState(false);
    const [aiPopupPosition, setAiPopupPosition] = useState({ x: 0, y: 0 });
    const [aiPopupBlockId, setAiPopupBlockId] = useState<string | null>(null);

    const showAIPopup = useCallback((position: { x: number; y: number }, blockId: string) => {
      setAiPopupPosition(position);
      setAiPopupBlockId(blockId);
      setAiPopupVisible(true);
    }, []);

    const closeAIPopup = useCallback(() => {
      setAiPopupVisible(false);
      setAiPopupBlockId(null);
    }, []);

    // =========================================================================
    // Editor Context
    // =========================================================================

    const getEditorContext = useCallback(async (): Promise<EditorContext | undefined> => {
      if (!editor) return undefined;

      try {
        const cursorPosition = editor.getTextCursorPosition();
        const blocks = editor.document;
        const currentBlockId = cursorPosition.block.id;
        const currentBlockIndex = blocks.findIndex((b: any) => b.id === currentBlockId);

        // Export document as markdown to save tokens (only if sharing context is enabled)
        const documentMarkdown = aiShareContext ? await editor.blocksToMarkdownLossy() : undefined;

        return {
          documentMarkdown,
          currentBlockId,
          currentBlockIndex: currentBlockIndex >= 0 ? currentBlockIndex : 0,
          runbookId,
        };
      } catch (error) {
        console.warn("Failed to get editor context:", error);
        return undefined;
      }
    }, [editor, aiShareContext, runbookId]);

    // =========================================================================
    // Inline Generation Hook
    // =========================================================================

    const {
      isGenerating,
      generatingBlockIds,
      generatedBlockIds,
      isEditing,
      editPrompt,
      loadingStatus,
      clearPostGenerationMode,
      handleEditSubmit,
      cancelEditing,
      setEditPrompt,
      startGenerationWithPrompt,
      getIsProgrammaticEdit,
      hasGeneratedBlocks,
      handleKeyDown: handleInlineGenerationKeyDown,
    } = useAIInlineGeneration({
      editor,
      runbookId,
      documentBridge,
      getEditorContext,
      username,
      chargeTarget,
    });

    // =========================================================================
    // Popup Submit Handler
    // =========================================================================

    const handlePopupSubmit = useCallback(
      (prompt: string) => {
        if (!aiPopupBlockId) return;
        // Close popup immediately, generation UI is handled by the hook
        closeAIPopup();
        // Start generation with replacePromptBlock=true to replace the empty block
        startGenerationWithPrompt(prompt, aiPopupBlockId, true);
        editor?.focus();
      },
      [aiPopupBlockId, closeAIPopup, startGenerationWithPrompt, editor],
    );

    // =========================================================================
    // Keyboard Shortcuts
    // =========================================================================

    const { handleKeyDown: handleAIShortcutsKeyDown } = useAIKeyboardShortcuts({
      editor,
      onShowAIPopup: showAIPopup,
    });

    // Combined keyboard handler
    const handleKeyDown = useCallback(
      (e: React.KeyboardEvent) => {
        handleInlineGenerationKeyDown(e);
        handleAIShortcutsKeyDown(e);
      },
      [handleInlineGenerationKeyDown, handleAIShortcutsKeyDown],
    );

    // =========================================================================
    // Imperative Handle
    // =========================================================================

    useImperativeHandle(
      ref,
      () => ({
        handleKeyDown,
        hasGeneratedBlocks,
        getIsProgrammaticEdit,
        clearPostGenerationMode,
        showAIPopup,
      }),
      [handleKeyDown, hasGeneratedBlocks, getIsProgrammaticEdit, clearPostGenerationMode, showAIPopup],
    );

    // =========================================================================
    // Render
    // =========================================================================

    return (
      <>
        {/* AI Generate Popup */}
        <AIGeneratePopup
          isVisible={aiPopupVisible}
          position={aiPopupPosition}
          onSubmit={handlePopupSubmit}
          onClose={closeAIPopup}
        />

        {/* Subtle hint for AI generation */}
        {showHint && generatedBlockIds.length === 0 && (
          <AIHint editor={editor} isGenerating={isGenerating} aiEnabled={true} />
        )}

        {/* Inline generation loading overlay */}
        {isGenerating && generatingBlockIds && (
          <AILoadingOverlay blockIds={generatingBlockIds} editor={editor} status={loadingStatus} />
        )}

        {/* Post-generation focus overlay - shows after AI generates blocks */}
        {generatedBlockIds.length > 0 && (
          <AIFocusOverlay
            hideAllHints={isGenerating}
            showRunHint={generatedBlockIds.length === 1}
            blockIds={generatedBlockIds}
            editor={editor}
            isEditing={isEditing}
            editValue={editPrompt}
            onEditChange={setEditPrompt}
            onEditSubmit={handleEditSubmit}
            onEditCancel={cancelEditing}
          />
        )}
      </>
    );
  },
);

// =============================================================================
// Helper for slash menu
// =============================================================================

/**
 * Creates an "AI Generate" slash menu item.
 * Call this with the editor and the AI features ref's showAIPopup function.
 */
export function createAIGenerateMenuItem(
  editor: any,
  showAIPopup: (position: { x: number; y: number }, blockId: string) => void,
) {
  return {
    title: "AI Generate",
    subtext: "Generate blocks from a natural language prompt (or press ⌘K)",
    onItemClick: () => {
      track_event("runbooks.ai.slash_menu_popup");
      const cursorPosition = editor.getTextCursorPosition();
      const position = calculateAIPopupPosition(editor);
      showAIPopup(position, cursorPosition.block.id);
    },
    icon: <SparklesIcon size={18} />,
    aliases: ["ai", "generate", "prompt"],
    group: "AI",
  };
}
