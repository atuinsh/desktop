import React, { useCallback, useRef } from "react";
import { calculateAIPopupPosition } from "../utils/popupPositioning";
import track_event from "@/tracking";

// Block types that have inline text content
const TEXT_BLOCK_TYPES = [
  "paragraph",
  "heading",
  "bulletListItem",
  "numberedListItem",
  "checkListItem",
];

interface UseAIKeyboardShortcutsProps {
  editor: any;
  // Callbacks for showing popups
  onShowAIPopup: (position: { x: number; y: number }) => void;
  onShowEditPopup: (position: { x: number; y: number }, block: any) => void;
}

interface UseAIKeyboardShortcutsReturn {
  handleKeyDown: (e: React.KeyboardEvent) => void;
}

/**
 * Hook for handling Cmd+K keyboard shortcuts for AI popups.
 *
 * Note: Cmd+Enter for inline generation and post-generation shortcuts
 * are handled by useAIInlineGeneration.
 */
export function useAIKeyboardShortcuts({
  editor,
  onShowAIPopup,
  onShowEditPopup,
}: UseAIKeyboardShortcutsProps): UseAIKeyboardShortcutsReturn {
  // Use refs to avoid recreating the handler when callbacks change
  const callbacksRef = useRef({ onShowAIPopup, onShowEditPopup });
  callbacksRef.current = { onShowAIPopup, onShowEditPopup };

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!editor) return;

      // Only handle Cmd+K
      if (!e.metaKey || e.key !== "k") return;

      try {
        const cursorPosition = editor.getTextCursorPosition();
        const currentBlock = cursorPosition.block;

        const isTextBlock = TEXT_BLOCK_TYPES.includes(currentBlock.type);
        const hasContent =
          currentBlock.content &&
          Array.isArray(currentBlock.content) &&
          currentBlock.content.length > 0;

        if (isTextBlock && !hasContent) {
          // Empty text block + Cmd+K = show generate popup
          e.preventDefault();
          e.stopPropagation();
          track_event("runbooks.ai.keyboard_shortcut");
          const position = calculateAIPopupPosition(editor, currentBlock.id);
          callbacksRef.current.onShowAIPopup(position);
        } else if (!isTextBlock) {
          // Non-text block + Cmd+K = show edit popup
          e.preventDefault();
          e.stopPropagation();
          track_event("runbooks.ai.edit_block", { blockType: currentBlock.type });
          const position = calculateAIPopupPosition(editor, currentBlock.id);
          callbacksRef.current.onShowEditPopup(position, currentBlock);
        }
        // Note: Cmd+K on text block WITH content does nothing here
        // Cmd+Enter for generation is handled by useAIInlineGeneration
      } catch (error) {
        console.warn("Could not get cursor position:", error);
      }
    },
    [editor]
  );

  return { handleKeyDown };
}
