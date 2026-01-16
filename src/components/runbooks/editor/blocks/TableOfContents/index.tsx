import { ListIcon, ChevronRightIcon, ChevronDownIcon } from "lucide-react";
import { createReactBlockSpec } from "@blocknote/react";
import { useCallback, useEffect, useState } from "react";
import track_event from "@/tracking";

interface HeadingItem {
  id: string;
  text: string;
  level: number;
}

interface TableOfContentsProps {
  editor: any;
  blockId: string;
}

const getBlockText = (block: any): string => {
  if (!block.content || !Array.isArray(block.content)) return "";
  return block.content
    .filter((item: any) => item.type === "text")
    .map((item: any) => item.text || "")
    .join("");
};

const TableOfContents = ({ editor, blockId }: TableOfContentsProps) => {
  const [headings, setHeadings] = useState<HeadingItem[]>([]);
  const [isCollapsed, setIsCollapsed] = useState(false);

  const extractHeadings = useCallback(() => {
    if (!editor) return;
    const blocks = editor.document;
    const headingItems: HeadingItem[] = blocks
      .filter((block: any) => block.type === "heading" && block.id !== blockId)
      .map((block: any) => ({
        id: block.id,
        text: getBlockText(block),
        level: block.props?.level || 1,
      }));
    setHeadings(headingItems);
  }, [editor, blockId]);

  // Extract headings on mount and subscribe to changes
  useEffect(() => {
    extractHeadings();

    // Subscribe to editor content changes
    const unsubscribe = editor.onEditorContentChange(() => {
      extractHeadings();
    });

    return () => {
      unsubscribe?.();
    };
  }, [editor, extractHeadings]);

  const scrollToBlock = useCallback(
    (targetBlockId: string) => {
      // Find the block element in the DOM
      const blockElement = document.querySelector(`[data-id="${targetBlockId}"]`);

      if (blockElement) {
        blockElement.scrollIntoView({
          behavior: "smooth",
          block: "center",
        });
      }

      // Also set cursor position
      editor.setTextCursorPosition(targetBlockId, "start");
    },
    [editor],
  );

  if (headings.length === 0) {
    return (
      <div className="w-full flex items-center gap-2 py-2 px-3 text-zinc-400 dark:text-zinc-500 text-sm italic border border-zinc-200 dark:border-zinc-800 rounded-md">
        <ListIcon className="w-4 h-4" />
        <span>No headings in document</span>
      </div>
    );
  }

  return (
    <div className="w-full py-2 border border-zinc-200 dark:border-zinc-800 rounded-md">
      <button
        onClick={() => setIsCollapsed(!isCollapsed)}
        className="flex items-center gap-2 px-3 py-1.5 w-full text-left text-xs font-medium uppercase tracking-wider text-zinc-400 dark:text-zinc-500 hover:text-zinc-600 dark:hover:text-zinc-400 transition-colors"
      >
        {isCollapsed ? (
          <ChevronRightIcon className="w-3 h-3" />
        ) : (
          <ChevronDownIcon className="w-3 h-3" />
        )}
        <span>Contents</span>
        <span className="text-zinc-300 dark:text-zinc-600">({headings.length})</span>
      </button>

      {!isCollapsed && (
        <nav className="mt-1 space-y-0.5 px-2">
          {headings.map((heading) => {
            // Indentation: 8px base + 12px per level (h1=8, h2=20, h3=32, h4=44, h5=56, h6=68)
            const paddingLeft = 8 + (heading.level - 1) * 12;
            return (
              <button
                key={heading.id}
                onClick={() => scrollToBlock(heading.id)}
                style={{ paddingLeft }}
                className={`
                  block w-full text-left text-sm truncate
                  text-zinc-500 dark:text-zinc-400
                  hover:text-zinc-800 dark:hover:text-zinc-200
                  hover:bg-zinc-100 dark:hover:bg-zinc-800
                  rounded pr-2 py-1 transition-colors
                  ${heading.level === 1 ? "font-medium text-zinc-700 dark:text-zinc-300" : ""}
                  ${heading.level >= 4 ? "text-xs" : ""}
                `}
                title={heading.text}
              >
                {heading.text || "Untitled"}
              </button>
            );
          })}
        </nav>
      )}
    </div>
  );
};

export default createReactBlockSpec(
  {
    type: "table_of_contents",
    propSchema: {},
    content: "none",
  },
  {
    toExternalHTML: () => {
      return <div>[Table of Contents]</div>;
    },
    render: ({ block, editor }) => {
      return <TableOfContents editor={editor} blockId={block.id} />;
    },
  },
);

// Component to insert this block from the editor menu
export const insertTableOfContents =
  (schema: any) => (editor: typeof schema.BlockNoteEditor) => ({
    title: "Contents",
    subtext: "Add a navigable list of all headings",
    onItemClick: async () => {
      track_event("runbooks.block.create", { type: "table_of_contents" });

      editor.insertBlocks(
        [
          {
            type: "table_of_contents",
            props: {},
          },
        ],
        editor.getTextCursorPosition().block.id,
        "before",
      );
    },
    icon: <ListIcon size={18} />,
    aliases: ["toc", "contents"],
    group: "Content",
  });
