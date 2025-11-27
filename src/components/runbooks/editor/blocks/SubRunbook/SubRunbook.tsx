import { useState, useEffect, useRef } from "react";
import { BookOpenIcon, CheckCircleIcon, XCircleIcon, AlertTriangleIcon, ChevronDownIcon } from "lucide-react";
import { Button, Input, Tooltip } from "@heroui/react";
import { cn, exportPropMatter } from "@/lib/utils";
import { createReactBlockSpec } from "@blocknote/react";
import { useBlockExecution, useBlockState } from "@/lib/hooks/useDocumentBridge";
import track_event from "@/tracking";
import RunbookIndexService from "@/state/runbooks/search";
import Runbook from "@/state/runbooks/runbook";
import { useStore } from "@/state/store";
import PlayButton from "@/lib/blocks/common/PlayButton";

// Create a search index instance
const searchIndex = new RunbookIndexService();

// SubRunbook state from backend
interface SubRunbookState {
  totalBlocks: number;
  completedBlocks: number;
  currentBlockName: string | null;
  status: SubRunbookStatus;
}

type SubRunbookStatus =
  | "idle"
  | "loading"
  | "running"
  | "success"
  | { failed: { error: string } }
  | "cancelled"
  | "notFound"
  | "recursionDetected";

function getStatusLabel(status: SubRunbookStatus): string {
  if (status === "idle") return "Ready";
  if (status === "loading") return "Loading...";
  if (status === "running") return "Running...";
  if (status === "success") return "Completed";
  if (status === "cancelled") return "Cancelled";
  if (status === "notFound") return "Not Found";
  if (status === "recursionDetected") return "Recursion Detected";
  if (typeof status === "object" && "failed" in status) return `Failed: ${status.failed.error}`;
  return "Unknown";
}

function isErrorStatus(status: SubRunbookStatus): boolean {
  return (
    status === "notFound" ||
    status === "recursionDetected" ||
    (typeof status === "object" && "failed" in status)
  );
}

interface SubRunbookProps {
  id: string;
  runbookId: string;
  runbookName: string;
  isEditable: boolean;
  onRunbookSelect: (runbookId: string, runbookName: string) => void;
}

function RunbookSelector({
  isVisible,
  position,
  onSelect,
  onClose,
  anchorRef,
}: {
  isVisible: boolean;
  position: { x: number; y: number };
  onSelect: (runbookId: string, runbookName: string) => void;
  onClose: () => void;
  anchorRef: React.RefObject<HTMLDivElement | null>;
}) {
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

      setTimeout(() => {
        inputRef.current?.focus();
      }, 100);
    }
  }, [isVisible]);

  // Handle search
  useEffect(() => {
    if (!query.trim()) {
      const recentRunbooks = runbooks
        .slice()
        .sort((a: Runbook, b: Runbook) => b.updated.getTime() - a.updated.getTime())
        .slice(0, 10);
      setFilteredRunbooks(recentRunbooks);
      setSelectedIndex(0);
      return;
    }

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

  // Close on click outside
  useEffect(() => {
    if (!isVisible) return;

    const handleClickOutside = (e: MouseEvent) => {
      if (anchorRef.current && !anchorRef.current.contains(e.target as Node)) {
        onClose();
      }
    };

    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [isVisible, onClose, anchorRef]);

  if (!isVisible) return null;

  return (
    <div
      className="absolute z-50 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg min-w-80 max-w-96"
      style={{
        left: position.x,
        top: position.y + 10,
      }}
    >
      <div className="p-3">
        <Input
          ref={inputRef}
          placeholder="Search runbooks..."
          value={query}
          onValueChange={setQuery}
          size="sm"
          classNames={{
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
                  ? "bg-purple-50 dark:bg-purple-900/20 text-purple-600 dark:text-purple-400"
                  : "hover:bg-gray-50 dark:hover:bg-gray-700",
              )}
              onClick={() => onSelect(runbook.id, runbook.name || "Untitled Runbook")}
            >
              <BookOpenIcon size={14} />
              <span className="truncate">{runbook.name || "Untitled Runbook"}</span>
            </div>
          ))
        )}
      </div>

      <div className="p-2 text-xs text-gray-500 dark:text-gray-400 border-t border-gray-100 dark:border-gray-700">
        Press Enter to select
      </div>
    </div>
  );
}

const SubRunbook = ({
  id,
  runbookId,
  runbookName,
  isEditable,
  onRunbookSelect,
}: SubRunbookProps) => {
  const [selectorVisible, setSelectorVisible] = useState(false);
  const [selectorPosition, setSelectorPosition] = useState({ x: 0, y: 0 });
  const selectButtonRef = useRef<HTMLButtonElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const execution = useBlockExecution(id);
  const state = useBlockState<SubRunbookState>(id);

  const status = state?.status || "idle";
  const progress = state ? `${state.completedBlocks}/${state.totalBlocks}` : "0/0";
  const currentBlock = state?.currentBlockName;

  const handleSelectClick = () => {
    if (!isEditable) return;

    if (selectButtonRef.current) {
      const rect = selectButtonRef.current.getBoundingClientRect();
      const containerRect = containerRef.current?.getBoundingClientRect();
      setSelectorPosition({
        x: rect.left - (containerRect?.left || 0),
        y: rect.bottom - (containerRect?.top || 0),
      });
    }
    setSelectorVisible(true);
  };

  const handleRunbookSelected = (selectedId: string, selectedName: string) => {
    onRunbookSelect(selectedId, selectedName);
    setSelectorVisible(false);
  };

  const handleExecute = async () => {
    if (!runbookId) return;
    await execution.execute();
  };

  const [hasRun, setHasRun] = useState(false);

  // Track when execution starts
  useEffect(() => {
    if (execution.isRunning) {
      setHasRun(true);
    }
  }, [execution.isRunning]);

  const getStatusIcon = () => {
    if (status === "success" && hasRun) {
      return <CheckCircleIcon className="h-4 w-4 text-green-500" />;
    }
    if (isErrorStatus(status)) {
      return <XCircleIcon className="h-4 w-4 text-red-500" />;
    }
    if (status === "cancelled") {
      return <AlertTriangleIcon className="h-4 w-4 text-yellow-500" />;
    }
    return null;
  };

  return (
    <div ref={containerRef} className="relative w-full">
      <Tooltip
        content="Execute another runbook as part of this one"
        delay={1000}
      >
        <div className="flex flex-col w-full bg-white dark:bg-slate-800 rounded-lg p-3 border border-gray-200 dark:border-gray-700 shadow-sm hover:shadow-md transition-all duration-200">
          <span className="text-[10px] font-mono text-gray-400 dark:text-gray-500 mb-2">subrunbook</span>
          <div className="flex flex-row items-center space-x-3">
          {/* Play button */}
          <PlayButton
            eventName="runbooks.block.execute"
            eventProps={{ type: "sub-runbook" }}
            onPlay={handleExecute}
            onStop={execution.cancel}
            isRunning={execution.isRunning}
            cancellable={true}
            disabled={!runbookId}
            tooltip={!runbookId ? "Select a runbook first" : undefined}
          />

          {/* Progress indicator */}
          {(status === "running" || status === "loading") && (
            <div className="text-xs text-gray-500 dark:text-gray-400 whitespace-nowrap">
              {status === "loading" ? "Loading..." : progress}
              {currentBlock && (
                <span className="ml-1 text-gray-400 dark:text-gray-500 truncate max-w-[100px] inline-block align-bottom">
                  ({currentBlock})
                </span>
              )}
            </div>
          )}

          {/* Status icon (when not running) */}
          {!execution.isRunning && getStatusIcon() && (
            <div className="flex items-center">
              {getStatusIcon()}
            </div>
          )}

          {/* Runbook selector button */}
          <div className="flex-1 min-w-0">
            <Button
              ref={selectButtonRef}
              variant="flat"
              className="w-full justify-between bg-default-100"
              onPress={handleSelectClick}
              isDisabled={!isEditable}
              endContent={<ChevronDownIcon className="h-4 w-4 shrink-0" />}
            >
              <span className="truncate text-sm">{runbookId ? runbookName : "Select Runbook"}</span>
            </Button>
          </div>
          </div>
        </div>
      </Tooltip>

      {/* Error message */}
      {isErrorStatus(status) && (
        <div className="mt-2 text-sm text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-900/20 rounded p-2">
          {getStatusLabel(status)}
        </div>
      )}

      {/* Runbook selector popup */}
      <RunbookSelector
        isVisible={selectorVisible}
        position={selectorPosition}
        onSelect={handleRunbookSelected}
        onClose={() => setSelectorVisible(false)}
        anchorRef={containerRef}
      />
    </div>
  );
};

export default createReactBlockSpec(
  {
    type: "sub-runbook",
    propSchema: {
      name: { default: "" },
      // Runbook reference - at least one should be set
      runbookId: { default: "" },    // UUID (set by desktop app)
      runbookUri: { default: "" },   // Hub URI: "user/runbook" or "user/runbook:tag"
      runbookPath: { default: "" },  // File path for CLI use
      // Display name
      runbookName: { default: "" },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      const propMatter = exportPropMatter("sub-runbook", block.props, ["name", "runbookId", "runbookUri", "runbookPath", "runbookName"]);
      return (
        <div>
          <pre lang="sub-runbook">{propMatter}</pre>
        </div>
      );
    },
    // @ts-ignore
    render: ({ block, editor }) => {
      const onRunbookSelect = (runbookId: string, runbookName: string): void => {
        // Desktop app sets ID - this is the primary reference
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, runbookId, runbookName },
        });
      };

      return (
        <SubRunbook
          id={block.id}
          runbookId={block.props.runbookId}
          runbookName={block.props.runbookName}
          isEditable={editor.isEditable}
          onRunbookSelect={onRunbookSelect}
        />
      );
    },
  },
);

// Component to insert this block from the editor menu
export const insertSubRunbook = (editor: any) => ({
  title: "Sub-Runbook",
  subtext: "Embed and execute another runbook",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "sub-runbook" });

    editor.insertBlocks(
      [
        {
          type: "sub-runbook",
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <BookOpenIcon size={18} />,
  aliases: ["sub", "runbook", "embed", "include", "nested"],
  group: "Execute",
});
