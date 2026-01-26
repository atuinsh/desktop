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
  // Callback for showing generate popup with position and block ID
  onShowAIPopup: (position: { x: number; y: number }, blockId: string) => void;
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
}: UseAIKeyboardShortcutsProps): UseAIKeyboardShortcutsReturn {
  // Use refs to avoid recreating the handler when callbacks change
  const callbacksRef = useRef({ onShowAIPopup });
  callbacksRef.current = { onShowAIPopup };

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!editor) return;

      // Only handle Cmd+K
      if ((!e.metaKey && !e.ctrlKey) || e.key !== "k") return;

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
          callbacksRef.current.onShowAIPopup(position, currentBlock.id);
        }
        // Note: Cmd+K on text block WITH content does nothing
        // Cmd+K on non-text blocks does nothing (use AI sidebar instead)
        // Cmd+Enter for generation is handled by useAIInlineGeneration
      } catch (error) {
        console.warn("Could not get cursor position:", error);
      }
    },
    [editor]
  );

  return { handleKeyDown };
}
