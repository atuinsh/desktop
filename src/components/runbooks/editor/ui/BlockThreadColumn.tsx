import { useEffect, useRef, useState } from "react";
import { BlockNoteEditor } from "@blocknote/core";
import BlockThreadChat from "./BlockThreadChat";
import { cn } from "@/lib/utils";

interface ActiveThread {
  blockId: string;
  blockType: string;
}

interface BlockThreadColumnProps {
  editor: BlockNoteEditor | null;
  activeThread: ActiveThread | null;
  onClose: () => void;
  runbookId: string;
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
}

/**
 * A column that displays block-local AI threads, positioned vertically
 * to align with their target blocks (Google Docs comment style).
 */
export default function BlockThreadColumn({
  editor,
  activeThread,
  onClose,
  runbookId,
  scrollContainerRef,
}: BlockThreadColumnProps) {
  const [threadPosition, setThreadPosition] = useState<number>(0);
  const columnRef = useRef<HTMLDivElement>(null);

  // Calculate vertical position to align thread with its target block
  useEffect(() => {
    if (!activeThread || !editor || !scrollContainerRef.current) {
      return;
    }

    const updatePosition = () => {
      // Find the block element in the DOM
      const blockElement = document.querySelector(
        `[data-id="${activeThread.blockId}"]`
      );

      if (!blockElement || !scrollContainerRef.current) {
        return;
      }

      const scrollContainer = scrollContainerRef.current;
      const blockRect = blockElement.getBoundingClientRect();
      const containerRect = scrollContainer.getBoundingClientRect();

      // Position relative to the scroll container
      const relativeTop = blockRect.top - containerRect.top + scrollContainer.scrollTop;

      // Clamp to reasonable bounds (don't go above container or too far down)
      const clampedTop = Math.max(12, Math.min(relativeTop, scrollContainer.scrollHeight - 200));

      setThreadPosition(clampedTop);
    };

    updatePosition();

    // Update position on scroll
    const scrollContainer = scrollContainerRef.current;
    const handleScroll = () => {
      requestAnimationFrame(updatePosition);
    };

    scrollContainer.addEventListener("scroll", handleScroll, { passive: true });

    // Also update on window resize
    window.addEventListener("resize", updatePosition);

    return () => {
      scrollContainer.removeEventListener("scroll", handleScroll);
      window.removeEventListener("resize", updatePosition);
    };
  }, [activeThread, editor, scrollContainerRef]);

  if (!activeThread) {
    return null;
  }

  return (
    <div
      ref={columnRef}
      className={cn(
        "relative h-full flex-shrink-0 border-l border-default-200 dark:border-default-100",
        "bg-white dark:bg-gray-900"
      )}
      style={{ width: 280 }}
    >
      {/* Thread container - positioned to align with block */}
      <div
        className="absolute left-0 right-0 px-2"
        style={{
          top: threadPosition,
          maxHeight: "calc(100% - 24px)",
        }}
      >
        {/* Connector line to block */}
        <div
          className="absolute -left-3 top-4 w-3 h-px bg-purple-400 dark:bg-purple-500"
          aria-hidden="true"
        />

        <BlockThreadChat
          blockId={activeThread.blockId}
          blockType={activeThread.blockType}
          editor={editor}
          runbookId={runbookId}
          onClose={onClose}
        />
      </div>
    </div>
  );
}
