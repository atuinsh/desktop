import { useLayoutEffect, useState, useRef, useEffect } from "react";

interface AIFocusOverlayProps {
  blockIds: string[];
  editor: any;
  isEditing?: boolean;
  editValue?: string;
  showRunHint: boolean;
  hideAllHints?: boolean;
  onEditChange?: (value: string) => void;
  onEditSubmit?: () => void;
  onEditCancel?: () => void;
}

export function AIFocusOverlay({
  blockIds,
  editor,
  isEditing = false,
  editValue = "",
  showRunHint,
  hideAllHints = false,
  onEditChange,
  onEditSubmit,
  onEditCancel,
}: AIFocusOverlayProps) {
  const [position, setPosition] = useState<{
    top: number;
    left: number;
    width: number;
    height: number;
  } | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Focus input when editing starts
  useEffect(() => {
    if (isEditing && inputRef.current) {
      inputRef.current.focus();
    }
  }, [isEditing]);

  useLayoutEffect(() => {
    if (!editor?.domElement || blockIds.length === 0) return;

    const scrollContainer = editor.domElement.closest(".editor") as HTMLElement | null;

    // Track block elements for cleanup
    let blockEls: HTMLElement[] = [];

    // Apply faded effect to all blocks
    const applyPendingClass = () => {
      blockEls = blockIds
        .map((id) => editor.domElement?.querySelector(`[data-id="${id}"]`) as HTMLElement | null)
        .filter((el): el is HTMLElement => el !== null);

      blockEls.forEach((el) => el.classList.add("ai-generated-pending"));
    };

    applyPendingClass();

    const updatePosition = () => {
      if (!scrollContainer) return;

      // Re-query all blocks in case they were re-rendered
      const currentBlockEls = blockIds
        .map((id) => editor.domElement?.querySelector(`[data-id="${id}"]`) as HTMLElement | null)
        .filter((el): el is HTMLElement => el !== null);

      if (currentBlockEls.length === 0) return;

      // Update CSS classes if elements changed
      const currentSet = new Set(currentBlockEls);
      const previousSet = new Set(blockEls);

      // Remove class from elements no longer in the list
      blockEls.forEach((el) => {
        if (!currentSet.has(el)) {
          el.classList.remove("ai-generated-pending");
        }
      });

      // Add class to new elements
      currentBlockEls.forEach((el) => {
        if (!previousSet.has(el)) {
          el.classList.add("ai-generated-pending");
        }
      });

      blockEls = currentBlockEls;

      // Calculate combined bounding box
      const rects = currentBlockEls.map((el) => el.getBoundingClientRect());
      const containerRect = scrollContainer.getBoundingClientRect();

      const minTop = Math.min(...rects.map((r) => r.top));
      const minLeft = Math.min(...rects.map((r) => r.left));
      const maxBottom = Math.max(...rects.map((r) => r.bottom));
      const maxRight = Math.max(...rects.map((r) => r.right));

      setPosition({
        top: minTop - containerRect.top + scrollContainer.scrollTop,
        left: minLeft - containerRect.left,
        width: maxRight - minLeft,
        height: maxBottom - minTop,
      });
    };

    updatePosition();

    scrollContainer?.addEventListener("scroll", updatePosition);
    window.addEventListener("resize", updatePosition);

    // Observe all blocks for size changes
    const observer = new MutationObserver(updatePosition);
    blockEls.forEach((el) => {
      observer.observe(el, { childList: true, subtree: true, attributes: true });
    });

    return () => {
      blockEls.forEach((el) => el.classList.remove("ai-generated-pending"));
      scrollContainer?.removeEventListener("scroll", updatePosition);
      window.removeEventListener("resize", updatePosition);
      observer.disconnect();
    };
  }, [blockIds, editor]);

  if (!position) return null;

  return (
    <div
      className="absolute pointer-events-none z-10 rounded-lg border-2 border-purple-500/50 transition-all duration-150"
      style={{
        top: position.top - 4,
        left: position.left - 4,
        width: position.width + 8,
        height: position.height + 8,
      }}
    >
      {/* Edit input or hint text at the bottom */}
      {!hideAllHints && <div className="absolute -bottom-10 left-0 right-0 flex justify-center pointer-events-auto">
        {isEditing ? (
          <div className="flex items-center gap-2 bg-white dark:bg-zinc-900 px-3 py-2 rounded-lg shadow-md border border-purple-300 dark:border-purple-700">
            <input
              ref={inputRef}
              type="text"
              value={editValue}
              onChange={(e) => onEditChange?.(e.target.value)}
              onKeyDown={(e) => {
                e.stopPropagation();
                if (e.key === "Enter") {
                  onEditSubmit?.();
                  editor?.focus();
                } else if (e.key === "Escape") {
                  onEditCancel?.();
                  editor?.focus();
                }
              }}
              placeholder="Describe changes..."
              className="text-sm bg-transparent border-none outline-none w-72 text-zinc-800 dark:text-zinc-200 placeholder:text-zinc-400 dark:placeholder:text-zinc-500"
            />
            <span className="text-[10px] text-zinc-400 dark:text-zinc-500 flex items-center gap-1.5">
              <kbd className="font-mono bg-zinc-100 dark:bg-zinc-800 px-1.5 py-0.5 rounded border border-zinc-200 dark:border-zinc-700">↵</kbd>
              <span>send</span>
            </span>
          </div>
        ) : (
          <div className="text-[11px] bg-white dark:bg-zinc-900 px-3 py-1.5 rounded-lg shadow-md border border-purple-300 dark:border-purple-700 flex items-center gap-3">
            {showRunHint && (
              <>
                <span className="flex items-center gap-1.5 text-purple-700 dark:text-purple-300">
                  <kbd className="font-mono text-[10px] bg-purple-100 dark:bg-purple-900/60 px-1.5 py-0.5 rounded border border-purple-200 dark:border-purple-700">⌘</kbd>
                  <kbd className="font-mono text-[10px] bg-purple-100 dark:bg-purple-900/60 px-1.5 py-0.5 rounded border border-purple-200 dark:border-purple-700">Enter</kbd>
                  <span className="ml-0.5">Run</span>
                </span>
                <span className="text-purple-300 dark:text-purple-600">│</span>
              </>)}
            <span className="flex items-center gap-1.5 text-purple-700 dark:text-purple-300">
              <kbd className="font-mono text-[10px] bg-purple-100 dark:bg-purple-900/60 px-1.5 py-0.5 rounded border border-purple-200 dark:border-purple-700">Tab</kbd>
              <span className="ml-0.5">Accept</span>
            </span>
            <span className="text-purple-300 dark:text-purple-600">│</span>
            <span className="flex items-center gap-1.5 text-purple-700 dark:text-purple-300">
              <kbd className="font-mono text-[10px] bg-purple-100 dark:bg-purple-900/60 px-1.5 py-0.5 rounded border border-purple-200 dark:border-purple-700">E</kbd>
              <span className="ml-0.5">Edit</span>
            </span>
            <span className="text-purple-300 dark:text-purple-600">│</span>
            <span className="flex items-center gap-1.5 text-purple-700 dark:text-purple-300">
              <kbd className="font-mono text-[10px] bg-purple-100 dark:bg-purple-900/60 px-1.5 py-0.5 rounded border border-purple-200 dark:border-purple-700">Esc</kbd>
              <span className="ml-0.5">Dismiss</span>
            </span>
          </div>
        )}
      </div>}
    </div>
  );
}
