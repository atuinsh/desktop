import { useState, useEffect, useRef } from "react";
import { Input } from "@heroui/react";
import { cn } from "@/lib/utils";
import RunbookIndexService from "@/state/runbooks/search";
import Runbook from "@/state/runbooks/runbook";
import { useStore } from "@/state/store";
import { LinkIcon } from "lucide-react";

// Create a global search index instance
const searchIndex = new RunbookIndexService();

interface RunbookLinkPopupProps {
  isVisible: boolean;
  position: { x: number; y: number };
  onSelect: (runbookId: string, runbookName: string) => void;
  onClose: () => void;
}

export function RunbookLinkPopup({
  isVisible,
  position,
  onSelect,
  onClose,
}: RunbookLinkPopupProps) {
  const [query, setQuery] = useState("");
  const [runbooks, setRunbooks] = useState<Runbook[]>([]);
  const [filteredRunbooks, setFilteredRunbooks] = useState<Runbook[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Load runbooks when popup opens
  useEffect(() => {
    if (isVisible) {
      const loadRunbooks = async () => {
        const { selectedOrg } = useStore.getState();
        // TODO: support offline runbooks
        const allRunbooks = selectedOrg
          ? await Runbook.allFromOrg(selectedOrg)
          : await Runbook.allFromOrg(null);

        setRunbooks(allRunbooks);
        searchIndex.bulkUpdateRunbooks(allRunbooks);

        // Show recent runbooks initially
        const recentRunbooks = allRunbooks
          .slice()
          .sort((a: Runbook, b: Runbook) => b.updated.getTime() - a.updated.getTime())
          .slice(0, 10);
        setFilteredRunbooks(recentRunbooks);
        setSelectedIndex(0);
      };

      loadRunbooks();
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
      const recentRunbooks = runbooks
        .slice()
        .sort((a: Runbook, b: Runbook) => b.updated.getTime() - a.updated.getTime())
        .slice(0, 10);
      setFilteredRunbooks(recentRunbooks);
      setSelectedIndex(0);
      return;
    }

    // Search runbooks
    searchIndex.searchRunbooks(query).then((resultIds) => {
      const searchResults = resultIds
        .map((id) => runbooks.find((rb: Runbook) => rb.id === id))
        .filter((rb): rb is Runbook => rb !== undefined)
        .slice(0, 10);
      setFilteredRunbooks(searchResults);
      setSelectedIndex(0);
    });
  }, [query, runbooks]);

  // Handle keyboard navigation
  useEffect(() => {
    if (!isVisible) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setSelectedIndex((prev) => Math.min(prev + 1, filteredRunbooks.length - 1));
          break;
        case "ArrowUp":
          e.preventDefault();
          setSelectedIndex((prev) => Math.max(prev - 1, 0));
          break;
        case "Enter":
          e.preventDefault();
          if (filteredRunbooks[selectedIndex]) {
            const runbook = filteredRunbooks[selectedIndex];
            onSelect(runbook.id, runbook.name || "Untitled Runbook");
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
  }, [isVisible, filteredRunbooks, selectedIndex, onSelect, onClose]);

  if (!isVisible) return null;

  // Check if popup would go off-screen at the bottom
  const popupHeight = 300; // approximate popup height
  const windowHeight = window.innerHeight;
  const shouldPositionAbove = position.y + popupHeight + 50 > windowHeight;

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
          placeholder="Search runbooks..."
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          classNames={{
            input: "text-sm",
            inputWrapper: "h-8",
          }}
        />
      </div>

      <div className="max-h-60 overflow-y-auto">
        {filteredRunbooks.length === 0 ? (
          <div className="p-3 text-sm text-gray-500 dark:text-gray-400 text-center">
            No runbooks found
          </div>
        ) : (
          filteredRunbooks.map((runbook, index) => (
            <div
              key={runbook.id}
              className={cn(
                "flex items-center gap-2 px-3 py-2 cursor-pointer text-sm border-b border-gray-100 dark:border-gray-700 last:border-b-0",
                index === selectedIndex
                  ? "bg-blue-50 dark:bg-blue-900/20 text-blue-600 dark:text-blue-400"
                  : "hover:bg-gray-50 dark:hover:bg-gray-700",
              )}
              onClick={() => onSelect(runbook.id, runbook.name || "Untitled Runbook")}
            >
              <LinkIcon size={14} />
              <span className="truncate">{runbook.name || "Untitled Runbook"}</span>
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
