// Base for database block implementation
// Intended for databases that have tables
// postgres, sqlite, etc - not document stores.

import { useCallback, useRef, useState, useEffect } from "react";
import {
  Dropdown,
  DropdownTrigger,
  DropdownMenu,
  Button,
  DropdownItem,
  ButtonGroup,
  Tooltip,
  DropdownSection,
} from "@heroui/react";
import {
  ArrowDownToLineIcon,
  ArrowUpToLineIcon,
  ChevronDown,
  CloudOffIcon,
  DatabaseIcon,
  FileTerminalIcon,
  LockIcon,
  RefreshCwIcon,
  Maximize2,
  Minimize2,
} from "lucide-react";
import CodeMirror, { Extension } from "@uiw/react-codemirror";
import { langs } from "@uiw/codemirror-extensions-langs";

// @ts-ignore
import { CardHeader } from "@/components/ui/card";

import { useInterval } from "usehooks-ts";
import PlayButton from "./PlayButton";
import { QueryResult } from "./database";
import SQLResults from "./SQLResults";
import MaskedInput from "@/components/MaskedInput/MaskedInput";
import Block from "./Block";
import { templateString } from "@/state/templates";
import { useBlockNoteEditor } from "@blocknote/react";
import { cn, toSnakeCase } from "@/lib/utils";
import { logExecution } from "@/lib/exec_log";
import { DependencySpec } from "@/lib/workflow/dependency";
import { useBlockBusRunSubscription } from "@/lib/hooks/useBlockBus";
import BlockBus from "@/lib/workflow/block_bus";
import useCodemirrorTheme from "@/lib/hooks/useCodemirrorTheme";
import { useCodeMirrorValue } from "@/lib/hooks/useCodeMirrorValue";
import EditableHeading from "@/components/EditableHeading/index";
import { useCurrentRunbookId } from "@/context/runbook_id_context";

interface SQLProps {
  id: string;
  name: string;
  placeholder?: string;
  collapseQuery: boolean;
  extensions?: Extension[];
  sqlType: "sqlite" | "postgres" | "mysql" | "clickhouse"; // explicit SQL type: 'sqlite', 'postgres', 'mysql', 'clickhouse'
  isEditable: boolean;
  block: any;

  uri: string;
  query: string;
  autoRefresh: number;
  runQuery: (uri: string, query: string) => Promise<QueryResult & { queryCount?: number; executedQueries?: string[] }>;

  setCollapseQuery: (collapseQuery: boolean) => void;
  setQuery: (query: string) => void;
  setUri: (uri: string) => void;
  setAutoRefresh: (autoRefresh: number) => void;
  setName: (name: string) => void;
  setDependency: (dependency: DependencySpec) => void;
  onCodeMirrorFocus?: () => void;
}

const autoRefreshChoices = [
  { label: "Off", value: 0 },
  { label: "1s", value: 1000 },
  { label: "5s", value: 5000 },
  { label: "10s", value: 10000 },
  { label: "30s", value: 30000 },
  { label: "1m", value: 60000 },
  { label: "2m", value: 120000 },
  { label: "5m", value: 300000 },
  { label: "10m", value: 600000 },
  { label: "30m", value: 1800000 },
];

const SQL = ({
  id,
  name,
  block,
  setName,
  placeholder,
  query,
  setQuery,
  uri,
  setUri,
  autoRefresh,
  setAutoRefresh,
  collapseQuery,
  setCollapseQuery,
  setDependency,
  isEditable,
  runQuery,
  sqlType,
  extensions = [],
  onCodeMirrorFocus,
}: SQLProps) => {
  let editor = useBlockNoteEditor();
  const [isRunning, setIsRunning] = useState<boolean>(false);

  const [results, setResults] = useState<QueryResult & { queryCount?: number; executedQueries?: string[] } | null>(null);
  const [columns, setColumns] = useState<
    { id: string; title: string; grow?: number; width?: number }[]
  >([]);

  const [error, setError] = useState<string | null>(null);
  const currentRunbookId = useCurrentRunbookId();
  const elementRef = useRef<HTMLDivElement>(null);
  const [isFullscreen, setIsFullscreen] = useState<boolean>(false);
  const [isFullscreenQueryCollapsed, setIsFullscreenQueryCollapsed] = useState<boolean>(false);

  const themeObj = useCodemirrorTheme();
  const codeMirrorValue = useCodeMirrorValue(query, setQuery);
  
  // Get SQL language extension based on sqlType
  const getSqlExtension = () => {
    switch (sqlType) {
      case "postgres":
        return langs.pgsql();
      case "mysql":
        return langs.mysql();
      case "sqlite":
        return langs.sql();
      case "clickhouse":
        return langs.sql(); // Use generic SQL for ClickHouse
      default:
        return langs.sql();
    }
  };

  const handlePlay = useCallback(async () => {
    console.log("sql handlePlay called");
    setIsRunning(true);

    let startTime = new Date().getTime() * 1000000;
    try {
      let tUri = await templateString(id, uri, editor.document, currentRunbookId);
      let tQuery = await templateString(id, query, editor.document, currentRunbookId);

      if (elementRef.current) {
        elementRef.current.scrollIntoView({ behavior: "smooth", block: "center" });
      }

      let res = await runQuery(tUri, tQuery);
      let endTime = new Date().getTime() * 1000000;

      // Don't log the actual data, but log the query and metadata
      let output = {
        query,
        rowCount: res.rows?.length,
        queryCount: res.queryCount || 1,
        executedQueries: res.executedQueries || [],
      };
      await logExecution(block, block.typeName, startTime, endTime, JSON.stringify(output));
      BlockBus.get().blockFinished(block);

      setIsRunning(false);

      setColumns(columns);
      setResults(res);
      setError(null);
    } catch (e: any) {
      if (e.message) {
        e = e.message;
      }

      let endTime = new Date().getTime() * 1000000;
      await logExecution(block, block.typeName, startTime, endTime, JSON.stringify({ error: e }));
      BlockBus.get().blockFinished(block);

      setError(e);
      setIsRunning(false);
    }
  }, [block, editor.document, currentRunbookId, query, uri, runQuery]);

  useBlockBusRunSubscription(block.id, handlePlay);

  useInterval(
    () => {
      // let's not stack queries
      if (isRunning) return;

      (async () => {
        await handlePlay();
      })();
    },
    autoRefresh > 0 ? autoRefresh : null,
  );

  const addScriptForUri = () => {
    let blockName = toSnakeCase(name);

    // TODO: register custom schema type for typescript blocknote
    editor.insertBlocks(
      [
        {
          // @ts-ignore
          type: "script",
          props: {
            name: `uri for ${name}`,
            // @ts-ignore
            outputVariable: blockName,
            outputVisible: false,
            code: `# Output the uri for ${name}`,
          },
        },
      ],
      id,
      "before",
    );

    setUri(`{{ var.${blockName} }}`);
  };

  const addLocalVar = () => {
    let blockName = toSnakeCase(name);

    editor.insertBlocks(
      [
        {
          // @ts-ignore
          type: "local-var",
          props: {
            name: `${blockName}`,
          },
        },
      ],
      id,
      "before",
    );

    setUri(`{{ var.${blockName} }}`);
  };

  // Handle ESC key to exit fullscreen and prevent body scroll
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && isFullscreen) {
        setIsFullscreen(false);
      }
    };

    if (isFullscreen) {
      document.addEventListener("keydown", handleKeyDown);
      document.body.style.overflow = "hidden";
      return () => {
        document.removeEventListener("keydown", handleKeyDown);
        document.body.style.overflow = "auto";
      };
    }
  }, [isFullscreen]);

  return (
    <Block
      hasDependency
      name={name}
      block={block}
      setDependency={setDependency}
      type={block.typeName}
      setName={setName}
      inlineHeader
      header={
        <>
          <div className="flex flex-row justify-between w-full">
            <h1 className="text-default-700 font-semibold">
              <EditableHeading initialText={name} onTextChange={(text) => setName(text)} />
            </h1>
            <div className="flex flex-row items-center gap-2">
              <Tooltip content={isFullscreen ? "Exit fullscreen" : "Open in fullscreen"}>
                <button
                  onClick={() => setIsFullscreen(!isFullscreen)}
                  className="p-2 hover:bg-default-100 rounded-md"
                >
                  {isFullscreen ? <Minimize2 size={20} /> : <Maximize2 size={20} />}
                </button>
              </Tooltip>
            </div>
          </div>

          <div className="flex flex-row gap-2 w-full items-center">
            <MaskedInput
              size="sm"
              maskRegex={/(?<=:\/\/).*(?=@[^@]*$)/}
              placeholder={placeholder || "protocol://user:password@host:port/db"}
              label="URI"
              isRequired
              startContent={<DatabaseIcon size={18} />}
              value={uri}
              onChange={(val: string) => {
                setUri(val);
              }}
              disabled={!isEditable}
            />

            <Dropdown>
              <DropdownTrigger>
                <Button isIconOnly variant="flat">
                  <LockIcon size={16} />
                </Button>
              </DropdownTrigger>
              <DropdownMenu disabledKeys={["secret"]}>
                <DropdownSection title="Use a local variable or script">
                  <DropdownItem
                    key="local-var"
                    description="Local variable - not synced"
                    startContent={<CloudOffIcon size={16} />}
                    onPress={addLocalVar}
                  >
                    Variable
                  </DropdownItem>
                  <DropdownItem
                    key="template"
                    description="Shell command output"
                    startContent={<FileTerminalIcon size={16} />}
                    onPress={addScriptForUri}
                  >
                    Script
                  </DropdownItem>
                  <DropdownItem
                    key="secret"
                    description="Synchronized + encrypted secret"
                    startContent={<LockIcon size={16} />}
                  >
                    Secret
                  </DropdownItem>
                </DropdownSection>
              </DropdownMenu>
            </Dropdown>
          </div>

          <div className="flex flex-row gap-2 w-full" ref={elementRef}>
            <PlayButton
              eventName="runbooks.block.execute"
              eventProps={{ type: sqlType }}
              isRunning={isRunning}
              onPlay={handlePlay}
              cancellable={false}
            />
            <div className={cn("flex-grow relative transition-all duration-300 ease-in-out", {
              "max-h-10 overflow-hidden": collapseQuery,
            })}>
              <CodeMirror
                placeholder={"Write your query here..."}
                className="!pt-0 max-w-full border border-gray-300 rounded"
                basicSetup={true}
                extensions={[getSqlExtension(), ...extensions]}
                value={codeMirrorValue.value}
                onChange={codeMirrorValue.onChange}
                editable={isEditable}
                theme={themeObj}
                maxHeight="100vh"
                onFocus={onCodeMirrorFocus}
              />
              {collapseQuery && (
                <div className="absolute bottom-0 left-0 right-0 h-6 bg-gradient-to-t from-white dark:from-gray-900 to-transparent pointer-events-none" />
              )}
            </div>
          </div>
        </>
      }
      footer={
        <div className="flex flex-row justify-between items-center w-full">
          <div className="flex flex-row items-center gap-2">
            {results?.queryCount && results.queryCount > 1 && (
              <span className="text-xs text-default-500 px-2 py-1 bg-default-100 rounded">
                Executed {results.queryCount} queries
                {results.rows && results.columns && " • Showing last result with data"}
              </span>
            )}
          </div>
          <ButtonGroup>
            <Dropdown showArrow>
              <DropdownTrigger>
                <Button
                  size="sm"
                  variant="flat"
                  startContent={<RefreshCwIcon size={16} />}
                  endContent={<ChevronDown size={16} />}
                >
                  Auto refresh:{" "}
                  {autoRefresh == 0
                    ? "Off"
                    : (
                        autoRefreshChoices.find((a) => a.value == autoRefresh) || {
                          label: "Off",
                        }
                      ).label}
                </Button>
              </DropdownTrigger>
              <DropdownMenu variant="faded" aria-label="Select time frame for chart">
                {autoRefreshChoices.map((setting) => {
                  return (
                    <DropdownItem
                      key={setting.label}
                      onPress={() => {
                        setAutoRefresh(setting.value);
                      }}
                    >
                      {setting.label}
                    </DropdownItem>
                  );
                })}
              </DropdownMenu>
            </Dropdown>
            <Button
              size="sm"
              isIconOnly
              variant="flat"
              onPress={() => setCollapseQuery(!collapseQuery)}
            >
              <Tooltip content={collapseQuery ? "Expand query" : "Collapse query"}>
                {collapseQuery ? <ArrowDownToLineIcon size={16} /> : <ArrowUpToLineIcon size={16} />}
              </Tooltip>
            </Button>
          </ButtonGroup>
        </div>
      }
    >
      {(results || error) && !isFullscreen && (
        <SQLResults
          results={results}
          error={error}
          dismiss={() => {
            setResults(null);
            setError(null);
          }}
        />
      )}

      {/* Fullscreen SQL Block Modal */}
      {isFullscreen && (
        <div
          className="fixed inset-0 bg-black/90 backdrop-blur-md z-[9999]"
          onClick={(e) => {
            if (e.target === e.currentTarget) {
              setIsFullscreen(false);
            }
          }}
        >
          <div className="h-full bg-background overflow-hidden rounded-lg shadow-2xl flex flex-col">
            {/* Fullscreen Header */}
            <div
              data-tauri-drag-region
              className="flex justify-between items-center w-full border-default-200/50 bg-content1/95 backdrop-blur-sm flex-shrink-0"
            >
              <div
                data-tauri-drag-region
                className="flex items-center gap-3 ml-16 w-full justify-between"
              >
                <span className="text-sm text-default-700">{name || "SQL Query"}</span>
              </div>
              <ButtonGroup>
                <Button
                  isIconOnly
                  size="sm"
                  variant="flat"
                  onPress={() => setIsFullscreenQueryCollapsed(!isFullscreenQueryCollapsed)}
                >
                  <Tooltip
                    content={isFullscreenQueryCollapsed ? "Show query editor" : "Hide query editor"}
                  >
                    {isFullscreenQueryCollapsed ? (
                      <ArrowDownToLineIcon size={16} />
                    ) : (
                      <ArrowUpToLineIcon size={16} />
                    )}
                  </Tooltip>
                </Button>
                <Button isIconOnly size="sm" variant="flat" onPress={() => setIsFullscreen(false)}>
                  <Tooltip content="Exit fullscreen">
                    <Minimize2 size={18} />
                  </Tooltip>
                </Button>
              </ButtonGroup>
            </div>

            {/* Fullscreen Content */}
            <div className="min-h-0 flex-1 overflow-hidden flex flex-col">
              {/* URI and Controls Section - 1/3 height */}
              {!isFullscreenQueryCollapsed && (
                <div className="flex-none h-1/3 border-b border-default-200/50 flex flex-col overflow-hidden">
                  <div className="flex-shrink-0 p-4 pb-2">
                    <div className="flex flex-row gap-2 w-full items-center">
                      <MaskedInput
                        size="sm"
                        maskRegex={/(?<=:\/\/).*(?=@[^@]*$)/}
                        placeholder={placeholder || "protocol://user:password@host:port/db"}
                        label="URI"
                        isRequired
                        startContent={<DatabaseIcon size={18} />}
                        value={uri}
                        onChange={(val: string) => {
                          setUri(val);
                        }}
                        disabled={!isEditable}
                      />

                      <Dropdown>
                        <DropdownTrigger>
                          <Button isIconOnly variant="flat">
                            <LockIcon size={16} />
                          </Button>
                        </DropdownTrigger>
                        <DropdownMenu disabledKeys={["secret"]}>
                          <DropdownSection title="Use a local variable or script">
                            <DropdownItem
                              key="local-var"
                              description="Local variable - not synced"
                              startContent={<CloudOffIcon size={16} />}
                              onPress={addLocalVar}
                            >
                              Variable
                            </DropdownItem>
                            <DropdownItem
                              key="template"
                              description="Shell command output"
                              startContent={<FileTerminalIcon size={16} />}
                              onPress={addScriptForUri}
                            >
                              Script
                            </DropdownItem>
                            <DropdownItem
                              key="secret"
                              description="Synchronized + encrypted secret"
                              startContent={<LockIcon size={16} />}
                            >
                              Secret
                            </DropdownItem>
                          </DropdownSection>
                        </DropdownMenu>
                      </Dropdown>
                    </div>
                  </div>

                  {/* Query Editor */}
                  <div className="flex-1 min-h-0 p-4 pt-2 flex flex-col overflow-hidden">
                    <div className="flex flex-row gap-2 w-full h-full min-h-0">
                      <PlayButton
                        eventName="runbooks.block.execute"
                        eventProps={{ type: sqlType }}
                        isRunning={isRunning}
                        onPlay={handlePlay}
                        cancellable={false}
                      />
                      <div className="flex-grow min-h-0 min-w-0">
                        <CodeMirror
                          placeholder={"Write your query here..."}
                          className="!pt-0 border border-gray-300 rounded h-full overflow-scroll"
                          basicSetup={true}
                          extensions={[getSqlExtension(), ...extensions]}
                          value={codeMirrorValue.value}
                          onChange={codeMirrorValue.onChange}
                          editable={isEditable}
                          theme={themeObj}
                          onFocus={onCodeMirrorFocus}
                        />
                      </div>
                    </div>
                  </div>
                </div>
              )}

              {/* Results Section - 2/3 height */}
              <div className={cn("flex-1 min-h-0 overflow-hidden p-4", {
                "h-2/3": !isFullscreenQueryCollapsed,
                "h-full": isFullscreenQueryCollapsed
              })}>
                {(results || error) && (
                  <SQLResults
                    results={results}
                    error={error}
                    isFullscreen={true}
                    dismiss={() => {
                      setResults(null);
                      setError(null);
                    }}
                  />
                )}
              </div>
            </div>
          </div>
        </div>
      )}
    </Block>
  );
};

export default SQL;
