import { useLayoutEffect, useState, useRef, useEffect } from "react";

interface AIFocusOverlayProps {
  blockId: string;
  editor: any;
  isEditing?: boolean;
  editValue?: string;
  onEditChange?: (value: string) => void;
  onEditSubmit?: () => void;
  onEditCancel?: () => void;
}

export function AIFocusOverlay({
  blockId,
  editor,
  isEditing = false,
  editValue = "",
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
    if (!editor?.domElement) return;

    const scrollContainer = editor.domElement.closest(".editor") as HTMLElement | null;
    let blockEl = editor.domElement.querySelector(`[data-id="${blockId}"]`) as HTMLElement | null;

    // Apply faded effect via CSS class
    if (blockEl) {
      blockEl.classList.add("ai-generated-pending");
    }

    const updatePosition = () => {
      // Re-query in case block was re-rendered
      const currentBlockEl = editor.domElement?.querySelector(`[data-id="${blockId}"]`) as HTMLElement | null;
      if (!currentBlockEl || !scrollContainer) return;

      // Update blockEl reference if changed
      if (currentBlockEl !== blockEl) {
        blockEl?.classList.remove("ai-generated-pending");
        currentBlockEl.classList.add("ai-generated-pending");
        blockEl = currentBlockEl;
      }

      const blockRect = currentBlockEl.getBoundingClientRect();
      const containerRect = scrollContainer.getBoundingClientRect();

      setPosition({
        top: blockRect.top - containerRect.top + scrollContainer.scrollTop,
        left: blockRect.left - containerRect.left,
        width: blockRect.width,
        height: blockRect.height,
      });
    };

    updatePosition();

    scrollContainer?.addEventListener("scroll", updatePosition);
    window.addEventListener("resize", updatePosition);

    // Observe only the specific block for size changes, not the whole editor
    const observer = new MutationObserver(updatePosition);
    if (blockEl) {
      observer.observe(blockEl, { childList: true, subtree: true, attributes: true });
    }

    return () => {
      blockEl?.classList.remove("ai-generated-pending");
      scrollContainer?.removeEventListener("scroll", updatePosition);
      window.removeEventListener("resize", updatePosition);
      observer.disconnect();
    };
  }, [blockId, editor]);

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
      <div className="absolute -bottom-10 left-0 right-0 flex justify-center pointer-events-auto">
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
                } else if (e.key === "Escape") {
                  onEditCancel?.();
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
            <span className="flex items-center gap-1.5 text-purple-700 dark:text-purple-300">
              <kbd className="font-mono text-[10px] bg-purple-100 dark:bg-purple-900/60 px-1.5 py-0.5 rounded border border-purple-200 dark:border-purple-700">⌘</kbd>
              <kbd className="font-mono text-[10px] bg-purple-100 dark:bg-purple-900/60 px-1.5 py-0.5 rounded border border-purple-200 dark:border-purple-700">Enter</kbd>
              <span className="ml-0.5">Run</span>
            </span>
            <span className="text-purple-300 dark:text-purple-600">│</span>
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
      </div>
    </div>
  );
}
