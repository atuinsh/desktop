import { useLayoutEffect, useState } from "react";

interface AIFocusOverlayProps {
  blockId: string;
  editor: any;
}

export function AIFocusOverlay({ blockId, editor }: AIFocusOverlayProps) {
  const [position, setPosition] = useState<{
    top: number;
    left: number;
    width: number;
    height: number;
  } | null>(null);

  useLayoutEffect(() => {
    const blockEl = editor?.domElement?.querySelector(`[data-id="${blockId}"]`);

    // Apply faded effect to the block
    if (blockEl) {
      blockEl.style.opacity = "0.6";
      blockEl.style.transition = "opacity 150ms ease-in-out";
    }

    const updatePosition = () => {
      if (!editor?.domElement) return;

      const blockEl = editor.domElement.querySelector(`[data-id="${blockId}"]`);
      if (!blockEl) return;

      const scrollContainer = editor.domElement.closest(".editor");
      if (!scrollContainer) return;

      const blockRect = blockEl.getBoundingClientRect();
      const containerRect = scrollContainer.getBoundingClientRect();

      setPosition({
        top: blockRect.top - containerRect.top + scrollContainer.scrollTop,
        left: blockRect.left - containerRect.left,
        width: blockRect.width,
        height: blockRect.height,
      });
    };

    updatePosition();

    const scrollContainer = editor?.domElement?.closest(".editor");
    scrollContainer?.addEventListener("scroll", updatePosition);
    window.addEventListener("resize", updatePosition);

    // Also update on any DOM changes (block content might change size)
    const observer = new MutationObserver(updatePosition);
    if (blockEl) {
      observer.observe(blockEl, { childList: true, subtree: true, attributes: true });
    }

    return () => {
      // Restore opacity when overlay is removed
      if (blockEl) {
        blockEl.style.opacity = "";
        blockEl.style.transition = "";
      }
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
      {/* Hint text at the bottom */}
      <div className="absolute -bottom-8 left-0 right-0 flex justify-center">
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
            <kbd className="font-mono text-[10px] bg-purple-100 dark:bg-purple-900/60 px-1.5 py-0.5 rounded border border-purple-200 dark:border-purple-700">Esc</kbd>
            <span className="ml-0.5">Dismiss</span>
          </span>
        </div>
      </div>
    </div>
  );
}
