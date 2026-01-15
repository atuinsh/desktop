import { useCallback, useState } from "react";
import { ChevronRightIcon, ListIcon } from "lucide-react";

export interface HeadingItem {
  id: string;
  text: string;
  level: number;
}

interface TableOfContentsProps {
  editor: any;
  headings: HeadingItem[];
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
}

export default function TableOfContents({
  editor,
  headings,
  scrollContainerRef,
}: TableOfContentsProps) {
  const [isCollapsed, setIsCollapsed] = useState(false);

  const scrollToBlock = useCallback(
    (blockId: string) => {
      // Find the block element in the DOM
      const blockElement = scrollContainerRef.current?.querySelector(
        `[data-id="${blockId}"]`
      );

      if (blockElement && scrollContainerRef.current) {
        // Scroll the block into view within the container
        const containerRect = scrollContainerRef.current.getBoundingClientRect();
        const blockRect = blockElement.getBoundingClientRect();
        const scrollTop = scrollContainerRef.current.scrollTop;
        const offsetTop = blockRect.top - containerRect.top + scrollTop - 80; // 80px offset from top

        scrollContainerRef.current.scrollTo({
          top: offsetTop,
          behavior: "smooth",
        });
      }

      // Also set cursor position
      editor.setTextCursorPosition(blockId, "start");
    },
    [editor, scrollContainerRef],
  );

  if (headings.length === 0) {
    return null;
  }

  return (
    <div className="sticky top-3 right-0 max-h-[calc(100vh-120px)] z-10">
      {isCollapsed ? (
        <button
          onClick={() => setIsCollapsed(false)}
          className="p-2 rounded-lg bg-background/80 backdrop-blur-sm border border-zinc-200 dark:border-zinc-700 hover:bg-zinc-100 dark:hover:bg-zinc-800 transition-colors"
          title="Show contents"
        >
          <ListIcon className="w-4 h-4 text-zinc-500" />
        </button>
      ) : (
        <div className="w-48 bg-background/80 backdrop-blur-sm rounded-lg border border-zinc-200 dark:border-zinc-700 overflow-hidden">
          <button
            onClick={() => setIsCollapsed(true)}
            className="w-full flex items-center justify-between px-3 py-2 text-xs font-medium uppercase tracking-wider text-zinc-400 dark:text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800 transition-colors"
          >
            <span>Contents</span>
            <ChevronRightIcon className="w-3 h-3" />
          </button>
          <nav className="max-h-64 overflow-y-auto px-2 pb-2 space-y-0.5">
            {headings.map((heading) => (
              <button
                key={heading.id}
                onClick={() => scrollToBlock(heading.id)}
                className={`
                  block w-full text-left text-sm truncate
                  text-zinc-600 dark:text-zinc-400
                  hover:text-zinc-900 dark:hover:text-zinc-200
                  hover:bg-zinc-100 dark:hover:bg-zinc-800
                  rounded px-2 py-1 transition-colors
                  ${heading.level === 1 ? "pl-2 font-medium" : ""}
                  ${heading.level === 2 ? "pl-4" : ""}
                  ${heading.level === 3 ? "pl-6" : ""}
                `}
                title={heading.text}
              >
                {heading.text || "Untitled"}
              </button>
            ))}
          </nav>
        </div>
      )}
    </div>
  );
}
