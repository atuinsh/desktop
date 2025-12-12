import { useState, useEffect, useRef } from "react";
import { BookOpenIcon, ChevronDownIcon, GlobeIcon, FileIcon, AlertCircleIcon, SettingsIcon } from "lucide-react";
import { Button, Input, Tooltip, Select, SelectItem, Spinner, Switch, Modal, ModalContent, ModalHeader, ModalBody } from "@heroui/react";
import { cn, exportPropMatter } from "@/lib/utils";
import { createReactBlockSpec } from "@blocknote/react";
import useDocumentBridge, { useBlockExecution, useBlockState } from "@/lib/hooks/useDocumentBridge";
import track_event from "@/tracking";
import RunbookIndexService from "@/state/runbooks/search";
import Runbook, { OnlineRunbook } from "@/state/runbooks/runbook";
import { useStore } from "@/state/store";
import PlayButton from "@/lib/blocks/common/PlayButton";
import WorkspaceManager from "@/lib/workspaces/manager";
import { RemoteRunbook } from "@/state/models";
import { resolveRunbookByNwo, ResolvedRunbook } from "@/api/runbooks";

const searchIndex = new RunbookIndexService();

function getRelativePath(fromPath: string, toPath: string): string {
  const fromParts = fromPath.split('/').filter(Boolean);
  const toParts = toPath.split('/').filter(Boolean);

  fromParts.pop();

  let commonLength = 0;
  while (
    commonLength < fromParts.length &&
    commonLength < toParts.length &&
    fromParts[commonLength] === toParts[commonLength]
  ) {
    commonLength++;
  }

  const upCount = fromParts.length - commonLength;
  const relativeParts = Array(upCount).fill('..');

  relativeParts.push(...toParts.slice(commonLength));

  if (relativeParts.length === 0) {
    return './' + toParts[toParts.length - 1];
  }

  return relativeParts.join('/');
}

function getRunbookPathFromWorkspace(runbookId: string): string | null {
  const manager = WorkspaceManager.getInstance();
  const workspaces = manager.getWorkspaces();

  for (const workspace of workspaces) {
    const runbook = workspace.runbooks[runbookId];
    if (runbook) {
      return runbook.path;
    }
  }
  return null;
}

function getHubInfoFromRunbook(runbook: Runbook): { uri: string | null; tags: string[] } {
  if (!runbook.isOnline()) {
    return { uri: null, tags: [] };
  }

  const onlineRunbook = runbook as OnlineRunbook;
  if (!onlineRunbook.remoteInfo) {
    return { uri: null, tags: [] };
  }

  try {
    const remoteInfo: RemoteRunbook = JSON.parse(onlineRunbook.remoteInfo);
    const uri = remoteInfo.nwo || null;
    const tags = remoteInfo.snapshots?.map((s) => s.tag) || [];
    return { uri, tags };
  } catch {
    return { uri: null, tags: [] };
  }
}

function getTagFromUri(uri: string): string {
  const colonIndex = uri.lastIndexOf(':');
  if (colonIndex === -1) return 'latest';
  return uri.substring(colonIndex + 1) || 'latest';
}

function getBaseUri(uri: string): string {
  const colonIndex = uri.lastIndexOf(':');
  if (colonIndex === -1) return uri;
  return uri.substring(0, colonIndex);
}

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

interface RunbookSelection {
  runbookId: string;
  runbookName: string;
  runbookPath: string | null;
  runbookUri: string | null;
}

interface SubRunbookProps {
  id: string;
  runbookId: string;
  runbookName: string;
  runbookPath: string;
  runbookUri: string;
  exportEnv: boolean;
  isEditable: boolean;
  onRunbookSelect: (selection: RunbookSelection) => void;
  onTagChange: (tag: string) => void;
  onExportEnvChange: (exportEnv: boolean) => void;
  currentRunbookId: string | null;
}

function isValidHubUri(uri: string): boolean {
  const cleanUri = uri
    .trim()
    .replace(/^https?:\/\//, '')
    .replace(/^hub\.atuin\.sh\//, '');

  const pattern = /^[a-zA-Z0-9_-]+\/[a-zA-Z0-9_-]+(?::[a-zA-Z0-9._-]+)?$/;
  return pattern.test(cleanUri);
}

type HubLookupState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "found"; data: ResolvedRunbook }
  | { status: "error"; message: string };

function RunbookSelector({
  isVisible,
  position,
  onSelect,
  onClose,
  anchorRef,
  currentRunbookId,
}: {
  isVisible: boolean;
  position: { x: number; y: number };
  onSelect: (selection: RunbookSelection) => void;
  onClose: () => void;
  anchorRef: React.RefObject<HTMLDivElement | null>;
  currentRunbookId: string | null;
}) {
  const [query, setQuery] = useState("");
  const [runbooks, setRunbooks] = useState<Runbook[]>([]);
  const [filteredRunbooks, setFilteredRunbooks] = useState<Runbook[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [hubLookup, setHubLookup] = useState<HubLookupState>({ status: "idle" });
  const inputRef = useRef<HTMLInputElement>(null);

  const queryLooksLikeHubUri = isValidHubUri(query);

  useEffect(() => {
    if (!queryLooksLikeHubUri) {
      setHubLookup({ status: "idle" });
      return;
    }

    const parsed = parseHubUri(query);
    if (!parsed) {
      setHubLookup({ status: "idle" });
      return;
    }

    setHubLookup({ status: "loading" });

    // Track if this effect is still current (not stale from a newer query)
    let isCancelled = false;

    const timeoutId = setTimeout(async () => {
      try {
        const result = await resolveRunbookByNwo(parsed.nwo, parsed.tag || undefined);
        // Only update state if this request is still relevant
        if (!isCancelled) {
          setHubLookup({ status: "found", data: result });
        }
      } catch (err: any) {
        // Only update state if this request is still relevant
        if (!isCancelled) {
          const message = err?.code === 404
            ? "Runbook not found"
            : err?.message || "Failed to fetch";
          setHubLookup({ status: "error", message });
        }
      }
    }, 300);

    return () => {
      isCancelled = true;
      clearTimeout(timeoutId);
    };
  }, [query, queryLooksLikeHubUri]);

  function parseHubUri(uri: string): { nwo: string; tag: string | null } | null {
    const cleanUri = uri
      .trim()
      .replace(/^https?:\/\//, '')
      .replace(/^hub\.atuin\.sh\//, '');

    const colonIndex = cleanUri.lastIndexOf(':');
    if (colonIndex === -1) {
      return { nwo: cleanUri, tag: null };
    }
    return {
      nwo: cleanUri.substring(0, colonIndex),
      tag: cleanUri.substring(colonIndex + 1) || null,
    };
  }

  const selectRunbook = (runbook: Runbook) => {
    const runbookName = runbook.name || "Untitled Runbook";
    let relativePath: string | null = null;
    let runbookUri: string | null = null;

    if (runbook.isOnline()) {
      const hubInfo = getHubInfoFromRunbook(runbook);
      runbookUri = hubInfo.uri ? `${hubInfo.uri}:latest` : null;
    } else {
      const selectedPath = getRunbookPathFromWorkspace(runbook.id);
      if (selectedPath && currentRunbookId) {
        const currentPath = getRunbookPathFromWorkspace(currentRunbookId);
        if (currentPath) {
          relativePath = getRelativePath(currentPath, selectedPath);
        }
      }
    }

    onSelect({
      runbookId: runbook.id,
      runbookName,
      runbookPath: relativePath,
      runbookUri,
    });
  };

  const selectHubRunbook = () => {
    if (hubLookup.status !== "found") return;

    const { runbook, snapshot } = hubLookup.data;
    const tag = snapshot?.tag || "latest";
    const uri = `${runbook.nwo}:${tag}`;

    onSelect({
      runbookId: runbook.id,
      runbookName: runbook.name,
      runbookPath: null,
      runbookUri: uri,
    });
  };

  const showHubResult = queryLooksLikeHubUri && hubLookup.status !== "idle";
  const hubResultSelectable = hubLookup.status === "found";
  const totalItems = (showHubResult && hubResultSelectable ? 1 : 0) + filteredRunbooks.length;

  useEffect(() => {
    if (isVisible) {
      const loadRunbooks = async () => {
        const { selectedOrg } = useStore.getState();
        const allRunbooks = selectedOrg
          ? await Runbook.allFromOrg(selectedOrg)
          : await Runbook.allFromOrg(null);

        setRunbooks(allRunbooks);
        searchIndex.bulkUpdateRunbooks(allRunbooks);

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

  useEffect(() => {
    if (!isVisible) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setSelectedIndex((prev) => Math.min(prev + 1, Math.max(0, totalItems - 1)));
          break;
        case "ArrowUp":
          e.preventDefault();
          setSelectedIndex((prev) => Math.max(prev - 1, 0));
          break;
        case "Enter":
          e.preventDefault();
          if (hubResultSelectable && selectedIndex === 0) {
            selectHubRunbook();
          } else {
            const runbookIndex = hubResultSelectable ? selectedIndex - 1 : selectedIndex;
            if (filteredRunbooks[runbookIndex]) {
              selectRunbook(filteredRunbooks[runbookIndex]);
            }
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
  }, [isVisible, filteredRunbooks, selectedIndex, onSelect, onClose, hubResultSelectable, totalItems, hubLookup]);

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
          placeholder="Search or enter hub URI (user/runbook)"
          value={query}
          onValueChange={setQuery}
          size="sm"
          classNames={{
            inputWrapper: "h-8",
          }}
        />
      </div>

      <div className="max-h-60 overflow-y-auto">
        {showHubResult && (
          <>
            {hubLookup.status === "loading" && (
              <div className="flex items-center gap-2 px-3 py-2 text-sm border-b border-gray-100 dark:border-gray-700">
                <Spinner size="sm" classNames={{ wrapper: "h-4 w-4" }} />
                <span className="text-gray-500 dark:text-gray-400">
                  Looking up {query}...
                </span>
              </div>
            )}

            {hubLookup.status === "error" && (
              <div className="flex items-center gap-2 px-3 py-2 text-sm border-b border-gray-100 dark:border-gray-700 text-red-500 dark:text-red-400">
                <AlertCircleIcon size={14} />
                <span>{hubLookup.message}</span>
              </div>
            )}

            {hubLookup.status === "found" && (
              <div
                className={cn(
                  "flex items-center gap-2 px-3 py-2 cursor-pointer text-sm border-b border-gray-100 dark:border-gray-700",
                  selectedIndex === 0
                    ? "bg-purple-50 dark:bg-purple-900/20 text-purple-600 dark:text-purple-400"
                    : "hover:bg-gray-50 dark:hover:bg-gray-700",
                )}
                onClick={selectHubRunbook}
              >
                <GlobeIcon size={14} className="text-purple-500" />
                <div className="flex flex-col min-w-0">
                  <span className="truncate font-medium">{hubLookup.data.runbook.name}</span>
                  <span className="text-xs text-gray-500 dark:text-gray-400 truncate">
                    {hubLookup.data.runbook.nwo}
                    {hubLookup.data.snapshot && `:${hubLookup.data.snapshot.tag}`}
                  </span>
                </div>
              </div>
            )}
          </>
        )}

        {filteredRunbooks.length === 0 && !showHubResult ? (
          <div className="p-3 text-sm text-gray-500 dark:text-gray-400 text-center">
            No runbooks found
          </div>
        ) : (
          filteredRunbooks.map((runbook, index) => {
            const itemIndex = hubResultSelectable ? index + 1 : index;
            return (
              <div
                key={runbook.id}
                className={cn(
                  "flex items-center gap-2 px-3 py-2 cursor-pointer text-sm border-b border-gray-100 dark:border-gray-700 last:border-b-0",
                  itemIndex === selectedIndex
                    ? "bg-purple-50 dark:bg-purple-900/20 text-purple-600 dark:text-purple-400"
                    : "hover:bg-gray-50 dark:hover:bg-gray-700",
                )}
                onClick={() => selectRunbook(runbook)}
              >
                {runbook.isOnline() ? (
                  <GlobeIcon size={14} className="text-purple-500" />
                ) : (
                  <FileIcon size={14} />
                )}
                <span className="truncate">{runbook.name || "Untitled Runbook"}</span>
              </div>
            );
          })
        )}
      </div>

      <div className="p-2 text-xs text-gray-500 dark:text-gray-400 border-t border-gray-100 dark:border-gray-700">
        ↑↓ navigate · Enter select · Esc close
      </div>
    </div>
  );
}

const SubRunbook = ({
  id,
  runbookId,
  runbookName,
  runbookPath,
  runbookUri,
  exportEnv,
  isEditable,
  onRunbookSelect,
  onTagChange,
  onExportEnvChange,
  currentRunbookId,
}: SubRunbookProps) => {
  const [selectorVisible, setSelectorVisible] = useState(false);
  const [selectorPosition, setSelectorPosition] = useState({ x: 0, y: 0 });
  const [availableTags, setAvailableTags] = useState<string[]>([]);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const selectButtonRef = useRef<HTMLButtonElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const execution = useBlockExecution(id);
  const state = useBlockState<SubRunbookState>(id);

  useEffect(() => {
    if (!runbookId) {
      setAvailableTags([]);
      return;
    }

    Runbook.load(runbookId).then((runbook) => {
      if (runbook && runbook.isOnline()) {
        const hubInfo = getHubInfoFromRunbook(runbook);
        setAvailableTags(hubInfo.tags);
      } else {
        setAvailableTags([]);
      }
    }).catch(() => {
      setAvailableTags([]);
    });
  }, [runbookId, runbookUri]);

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

  const handleRunbookSelected = (selection: RunbookSelection) => {
    onRunbookSelect(selection);
    setSelectorVisible(false);
  };

  const handleExecute = async () => {
    if (!runbookId) return;
    await execution.execute();
  };

  return (
    <div ref={containerRef} className="relative w-full">
      <Tooltip
        content="Execute another runbook as part of this one"
        delay={1000}
      >
        <div className="flex flex-col w-full bg-white dark:bg-slate-800 rounded-lg p-3 border border-gray-200 dark:border-gray-700 shadow-sm hover:shadow-md transition-all duration-200">
          {/* Header row with block type name and settings */}
          <div className="flex items-center justify-between mb-2">
            <span className="text-[10px] font-mono text-gray-400 dark:text-gray-500">subrunbook</span>
            <Tooltip content="Settings" delay={500}>
              <button
                onClick={() => setSettingsOpen(true)}
                className="p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
              >
                <SettingsIcon className="h-4 w-4" />
              </button>
            </Tooltip>
          </div>

          {/* Main content row */}
          <div className="flex flex-row items-start space-x-3">
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

            <div className="flex-1 min-w-0">
              <div className="flex gap-2">
                <Button
                  ref={selectButtonRef}
                  variant="flat"
                  className="flex-1 justify-between bg-default-100"
                  onPress={handleSelectClick}
                  isDisabled={!isEditable}
                  endContent={<ChevronDownIcon className="h-4 w-4 shrink-0" />}
                >
                  <span className="truncate text-sm">{runbookId ? runbookName : "Select Runbook"}</span>
                </Button>
                {runbookUri && availableTags.length > 0 && (
                  <Select
                    size="md"
                    className="w-28"
                    selectedKeys={[getTagFromUri(runbookUri)]}
                    onSelectionChange={(keys) => {
                      const selectedTag = Array.from(keys)[0] as string;
                      if (selectedTag) {
                        onTagChange(selectedTag);
                      }
                    }}
                    isDisabled={!isEditable}
                    aria-label="Select tag"
                    items={[{ key: 'latest', label: 'latest' }, ...availableTags.map(tag => ({ key: tag, label: tag }))]}
                  >
                    {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                  </Select>
                )}
              </div>
              {(runbookUri || runbookPath || status === "running" || status === "loading") && (
                <div className="flex items-center justify-between mt-1">
                  <span className="text-[10px] font-mono text-gray-400 dark:text-gray-500 truncate flex items-center gap-1">
                    {runbookUri ? (
                      <GlobeIcon className="h-3 w-3 shrink-0" />
                    ) : runbookPath ? (
                      <FileIcon className="h-3 w-3 shrink-0" />
                    ) : null}
                    {runbookUri || runbookPath}
                  </span>
                  {(status === "running" || status === "loading") && (
                    <span className="flex items-center gap-1.5 text-[10px] font-mono text-gray-400 dark:text-gray-500 whitespace-nowrap ml-2">
                      <Spinner size="sm" classNames={{ wrapper: "h-3 w-3" }} />
                      {status === "loading" ? "Loading..." : (
                        <>
                          {progress}
                          {currentBlock && <span className="truncate max-w-[120px]">({currentBlock})</span>}
                        </>
                      )}
                    </span>
                  )}
                </div>
              )}
            </div>
          </div>
        </div>
      </Tooltip>

      {isErrorStatus(status) && (
        <div className="mt-2 text-sm text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-900/20 rounded p-2">
          {getStatusLabel(status)}
        </div>
      )}

      {/* Settings Modal */}
      <Modal
        isOpen={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        size="sm"
      >
        <ModalContent>
          <ModalHeader className="text-base font-medium">Sub-Runbook Settings</ModalHeader>
          <ModalBody className="pb-6">
            <div className="flex items-center justify-between gap-4">
              <div className="flex flex-col">
                <span className="text-sm font-medium text-gray-700 dark:text-gray-300">
                  Export environment variables
                </span>
                <span className="text-xs text-gray-500 dark:text-gray-400">
                  Merge env vars from the sub-runbook into the parent
                </span>
              </div>
              <Switch
                size="sm"
                isSelected={exportEnv}
                onValueChange={onExportEnvChange}
                isDisabled={!isEditable}
              />
            </div>
          </ModalBody>
        </ModalContent>
      </Modal>

      <RunbookSelector
        isVisible={selectorVisible}
        position={selectorPosition}
        onSelect={handleRunbookSelected}
        onClose={() => setSelectorVisible(false)}
        anchorRef={containerRef}
        currentRunbookId={currentRunbookId}
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
      // Settings
      exportEnv: { default: false }, // Export env vars to parent runbook
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      const propMatter = exportPropMatter("sub-runbook", block.props, ["name", "runbookId", "runbookUri", "runbookPath", "runbookName", "exportEnv"]);
      return (
        <div>
          <pre lang="sub-runbook">{propMatter}</pre>
        </div>
      );
    },
    // @ts-ignore
    render: ({ block, editor }) => {
      const documentBridge = useDocumentBridge();
      const currentRunbookId = documentBridge?.runbookId ?? null;

      const onRunbookSelect = (selection: RunbookSelection): void => {
        // Desktop app sets ID as primary reference, plus path/uri for CLI portability
        editor.updateBlock(block, {
          // @ts-ignore
          props: {
            ...block.props,
            runbookId: selection.runbookId,
            runbookName: selection.runbookName,
            runbookPath: selection.runbookPath || "",
            runbookUri: selection.runbookUri || "",
          },
        });
      };

      const onTagChange = (tag: string): void => {
        // Update the URI with the new tag
        const currentUri = block.props.runbookUri;
        if (!currentUri) return;

        const baseUri = getBaseUri(currentUri);
        const newUri = `${baseUri}:${tag}`;

        editor.updateBlock(block, {
          // @ts-ignore
          props: {
            ...block.props,
            runbookUri: newUri,
          },
        });
      };

      const onExportEnvChange = (exportEnv: boolean): void => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: {
            ...block.props,
            exportEnv,
          },
        });
      };

      return (
        <SubRunbook
          id={block.id}
          runbookId={block.props.runbookId}
          runbookName={block.props.runbookName}
          runbookPath={block.props.runbookPath}
          runbookUri={block.props.runbookUri}
          exportEnv={block.props.exportEnv}
          isEditable={editor.isEditable}
          onRunbookSelect={onRunbookSelect}
          onTagChange={onTagChange}
          onExportEnvChange={onExportEnvChange}
          currentRunbookId={currentRunbookId}
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
