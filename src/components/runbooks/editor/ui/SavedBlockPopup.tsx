import { useState, useEffect, useRef } from "react";
import { Input } from "@heroui/react";
import { cn } from "@/lib/utils";
import { BlocksIcon, TrashIcon } from "lucide-react";
import SavedBlock from "@/state/runbooks/saved_block";
import { DialogBuilder } from "@/components/Dialogs/dialog";
import { useQuery } from "@tanstack/react-query";
import { savedBlocks } from "@/lib/queries/saved_blocks";

interface SavedBlockPopupProps {
  isVisible: boolean;
  position: { x: number; y: number };
  onSelect: (savedBlockId: string, block: any) => void;
  onClose: () => void;
}

export function SavedBlockPopup({ isVisible, position, onSelect, onClose }: SavedBlockPopupProps) {
  const [query, setQuery] = useState("");
  const [filteredBlocks, setFilteredBlocks] = useState<SavedBlock[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const { data: blocks } = useQuery(savedBlocks());

  // Load runbooks when popup opens
  useEffect(() => {
    if (isVisible) {
      setQuery("");

      // Focus input after a short delay
      setTimeout(() => {
        inputRef.current?.focus();
      }, 100);
    }
  }, [isVisible]);

  // Handle search
  useEffect(() => {
    if (!query.trim()) {
      // Show recent runbooks when no query
      const recentBlocks = (blocks || [])
        .slice()
        .sort(
          (a: SavedBlock, b: SavedBlock) =>
            b.get("updated")!.getTime() - a.get("updated")!.getTime(),
        )
        .slice(0, 10);
      setFilteredBlocks(recentBlocks);
      setSelectedIndex(0);
      return;
    }

    // filter blocks by name
    const filteredBlocks = (blocks || []).filter((block) => block.get("name")!.includes(query));
    setFilteredBlocks(filteredBlocks);
    setSelectedIndex(0);
  }, [query, blocks]);

  // Handle keyboard navigation
  useEffect(() => {
    if (!isVisible) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setSelectedIndex((prev) => Math.min(prev + 1, filteredBlocks.length - 1));
          break;
        case "ArrowUp":
          e.preventDefault();
          setSelectedIndex((prev) => Math.max(prev - 1, 0));
          break;
        case "Enter":
          e.preventDefault();
          if (filteredBlocks[selectedIndex]) {
            const block = filteredBlocks[selectedIndex];
            onSelect(block.get("id")!, block.get("content")!);
          }
          break;
        case "Escape":
          e.preventDefault();
          onClose();
          break;
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [isVisible, filteredBlocks, selectedIndex, onSelect, onClose]);

  if (!isVisible) return null;

  // Check if popup would go off-screen at the bottom
  const popupHeight = 300; // approximate popup height
  const windowHeight = window.innerHeight;
  const shouldPositionAbove = position.y + popupHeight + 50 > windowHeight;

  async function handleDeleteBlock(e: React.MouseEvent<SVGSVGElement>, blockId: string) {
    e.stopPropagation();

    const block = await SavedBlock.get(blockId);

    if (!block) return;

    const answer = await new DialogBuilder<"yes" | "no">()
      .title(`Delete Saved Block ${block.get("name")!}`)
      .icon("error")
      .message("Are you sure you want to delete this saved block?")
      .action({ label: "OK", value: "yes", variant: "flat", color: "danger" })
      .action({ label: "Cancel", value: "no", variant: "flat" })
      .build();

    if (answer === "yes") {
      await block.del();
    }
  }

  return (
    <div
      className="absolute z-50 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg min-w-80 max-w-96"
      style={{
        left: position.x,
        top: shouldPositionAbove ? position.y - 10 : position.y + 50,
        transform: shouldPositionAbove ? "translateY(-100%)" : "none",
      }}
    >
      <div className="p-3">
        <Input
          ref={inputRef}
          placeholder="Search saved blocks..."
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          classNames={{
            input: "text-sm",
            inputWrapper: "h-8",
          }}
        />
      </div>

      <div className="max-h-60 overflow-y-auto">
        {filteredBlocks.length === 0 ? (
          <div className="p-3 text-sm text-gray-500 dark:text-gray-400 text-center">
            No saved blocks found
          </div>
        ) : (
          filteredBlocks.map((block, index) => (
            <div
              key={block.get("id")!}
              className={cn(
                "flex items-center justify-between px-3 py-2 cursor-pointer text-sm border-b border-gray-100 dark:border-gray-700 last:border-b-0",
                index === selectedIndex
                  ? "bg-blue-50 dark:bg-blue-900/20 text-blue-600 dark:text-blue-400"
                  : "hover:bg-gray-50 dark:hover:bg-gray-700",
              )}
              onClick={() => onSelect(block.get("id")!, block.get("content")!)}
            >
              <div className="flex items-center gap-2">
                <BlocksIcon size={14} />
                <span className="truncate">{block.get("name") || "Untitled Block"}</span>
              </div>
              <TrashIcon
                size={14}
                className="text-danger"
                onClick={(e) => handleDeleteBlock(e, block.get("id")!)}
              />
            </div>
          ))
        )}
      </div>

      <div className="p-2 text-xs text-gray-500 dark:text-gray-400 border-t border-gray-100 dark:border-gray-700">
        ↑↓ to navigate • Enter to select • Esc to cancel
      </div>
    </div>
  );
}
