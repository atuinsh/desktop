import { useEffect, useState, useRef, useCallback } from "react";

const STORAGE_KEY_DISMISSED = "ai-hint-dismissed";
const STORAGE_KEY_USE_COUNT = "ai-hint-use-count";
const USE_COUNT_THRESHOLD = 3;

interface AIHintProps {
  editor: any;
  isGenerating: boolean;
  aiEnabled: boolean;
}

function shouldShowHint(): boolean {
  const dismissed = localStorage.getItem(STORAGE_KEY_DISMISSED) === "true";
  const useCount = parseInt(localStorage.getItem(STORAGE_KEY_USE_COUNT) || "0", 10);
  return !dismissed && useCount < USE_COUNT_THRESHOLD;
}

export function incrementAIHintUseCount(): void {
  const current = parseInt(localStorage.getItem(STORAGE_KEY_USE_COUNT) || "0", 10);
  localStorage.setItem(STORAGE_KEY_USE_COUNT, String(current + 1));
}

export function AIHint({ editor, isGenerating, aiEnabled }: AIHintProps) {
  const [visible, setVisible] = useState(false);
  const [position, setPosition] = useState<{ top: number; left: number } | null>(null);
  const [permanentlyHidden, setPermanentlyHidden] = useState(!shouldShowHint());
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastContentRef = useRef<string>("");

  const dismissPermanently = useCallback(() => {
    localStorage.setItem(STORAGE_KEY_DISMISSED, "true");
    setPermanentlyHidden(true);
    setVisible(false);
  }, []);

  useEffect(() => {
    if (!editor || !aiEnabled || permanentlyHidden) return;

    const checkAndShowHint = () => {
      // Don't show while generating or if permanently hidden
      if (isGenerating || permanentlyHidden) {
        setVisible(false);
        return;
      }

      try {
        const cursorPosition = editor.getTextCursorPosition();
        const block = cursorPosition?.block;

        // Only show for paragraph blocks with content
        if (
          block?.type === "paragraph" &&
          block.content &&
          Array.isArray(block.content) &&
          block.content.length > 0
        ) {
          const text = block.content
            .filter((item: any) => item.type === "text")
            .map((item: any) => item.text || "")
            .join("");

          // Need at least a few characters
          if (text.trim().length < 3) {
            setVisible(false);
            return;
          }

          // Check if content changed (user is typing)
          if (text !== lastContentRef.current) {
            lastContentRef.current = text;
            setVisible(false);

            // Clear existing timeout
            if (timeoutRef.current) {
              clearTimeout(timeoutRef.current);
            }

            // Show hint after delay
            timeoutRef.current = setTimeout(() => {
              updatePosition();
              setVisible(true);
            }, 1500); // Show after 1.5s of no typing
          }
        } else {
          setVisible(false);
          lastContentRef.current = "";
        }
      } catch {
        setVisible(false);
      }
    };

    const updatePosition = () => {
      try {
        const cursorPosition = editor.getTextCursorPosition();
        const blockId = cursorPosition?.block?.id;
        if (!blockId || !editor.domElement) return;

        const blockEl = editor.domElement.querySelector(`[data-id="${blockId}"]`);
        if (!blockEl) return;

        const scrollContainer = editor.domElement.closest(".editor");
        if (!scrollContainer) return;

        const blockRect = blockEl.getBoundingClientRect();
        const containerRect = scrollContainer.getBoundingClientRect();

        setPosition({
          top: blockRect.bottom - containerRect.top + scrollContainer.scrollTop + 4,
          left: blockRect.left - containerRect.left,
        });
      } catch {
        setPosition(null);
      }
    };

    // Listen for editor changes
    const unsubscribe = editor.onSelectionChange(checkAndShowHint);

    // Also check on content changes
    const contentUnsubscribe = editor.onChange(checkAndShowHint);

    // Initial check
    checkAndShowHint();

    return () => {
      unsubscribe?.();
      contentUnsubscribe?.();
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, [editor, isGenerating, aiEnabled, permanentlyHidden]);

  // Hide on scroll
  useEffect(() => {
    if (!editor?.domElement) return;

    const scrollContainer = editor.domElement.closest(".editor");
    if (!scrollContainer) return;

    const handleScroll = () => setVisible(false);
    scrollContainer.addEventListener("scroll", handleScroll);

    return () => scrollContainer.removeEventListener("scroll", handleScroll);
  }, [editor]);

  // Dismiss on Esc key
  useEffect(() => {
    if (!visible) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        dismissPermanently();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [visible, dismissPermanently]);

  if (!visible || !position || isGenerating) return null;

  return (
    <div
      className="absolute z-10 transition-opacity duration-200"
      style={{
        top: position.top,
        left: position.left,
        opacity: visible ? 1 : 0,
      }}
    >
      <div className="text-[10px] text-zinc-400 dark:text-zinc-500 flex items-center gap-1">
        <kbd className="font-mono bg-zinc-100 dark:bg-zinc-800 px-1 py-0.5 rounded text-zinc-500 dark:text-zinc-400">⌘</kbd>
        <kbd className="font-mono bg-zinc-100 dark:bg-zinc-800 px-1 py-0.5 rounded text-zinc-500 dark:text-zinc-400">↵</kbd>
        <span className="ml-0.5">to generate</span>
        <button
          onClick={dismissPermanently}
          className="ml-1.5 text-zinc-300 dark:text-zinc-600 hover:text-zinc-500 dark:hover:text-zinc-400 transition-colors cursor-pointer bg-transparent border-none p-0 leading-none"
          title="Dismiss forever"
        >
          ×
        </button>
      </div>
    </div>
  );
}
