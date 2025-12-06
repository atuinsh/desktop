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
    const blockEl = editor?.domElement?.querySelector(`[data-id="${blockId}"]`);
    if (blockEl) {
      observer.observe(blockEl, { childList: true, subtree: true, attributes: true });
    }

    return () => {
      scrollContainer?.removeEventListener("scroll", updatePosition);
      window.removeEventListener("resize", updatePosition);
      observer.disconnect();
    };
  }, [blockId, editor]);

  if (!position) return null;

  return (
    <div
      className="absolute pointer-events-none z-10 rounded-lg border-2 border-purple-500/50 bg-purple-500/5 transition-all duration-150"
      style={{
        top: position.top - 4,
        left: position.left - 4,
        width: position.width + 8,
        height: position.height + 8,
      }}
    >
      {/* Hint text at the bottom */}
      <div className="absolute -bottom-6 left-0 right-0 flex justify-center">
        <div className="text-xs text-purple-600 dark:text-purple-400 bg-white dark:bg-gray-900 px-2 py-0.5 rounded shadow-sm border border-purple-200 dark:border-purple-800">
          <kbd className="font-mono">⌘↵</kbd> run • <kbd className="font-mono">Tab</kbd> continue
        </div>
      </div>
    </div>
  );
}
