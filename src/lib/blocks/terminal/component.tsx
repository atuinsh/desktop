// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";

import "./index.css";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { useBlockNoteEditor } from "@blocknote/react";

import "@xterm/xterm/css/xterm.css";
import { AtuinState, useStore } from "@/state/store.ts";
import { Button, Chip, Spinner, Tooltip } from "@heroui/react";
import { formatDuration, cn } from "@/lib/utils.ts";
import { usePtyStore } from "@/state/ptyStore.ts";
import track_event from "@/tracking.ts";
import {
  Clock,
  Eye,
  EyeOff,
  Maximize2,
  Minimize2,
  ArrowDownToLineIcon,
  ArrowUpToLineIcon,
} from "lucide-react";
import EditableHeading from "@/components/EditableHeading/index.tsx";
import CodeEditor, { TabAutoComplete } from "../common/CodeEditor/CodeEditor.tsx";
import { Command } from "@codemirror/view";
import { TerminalBlock } from "./schema.ts";
import { logExecution } from "@/lib/exec_log.ts";
import { DependencySpec } from "@/lib/workflow/dependency.ts";
import Terminal from "./components/terminal.tsx";
import Block from "../common/Block.tsx";
import PlayButton from "../common/PlayButton.tsx";
import { useCurrentRunbookId } from "@/context/runbook_id_context.ts";
import {
  useBlockContext,
  useBlockExecution,
  useBlockOutput,
} from "@/lib/hooks/useDocumentBridge.ts";
import { PtyMetadata } from "@/rs-bindings/PtyMetadata.ts";

interface RunBlockProps {
  onChange: (val: string) => void;
  onRun?: (pty: string) => void;
  onStop?: (pty: string) => void;
  setName: (name: string) => void;
  type: string;
  pty: string;
  isEditable: boolean;
  setOutputVisible: (visible: boolean) => void;
  setDependency: (dependency: DependencySpec) => void;
  onCodeMirrorFocus?: () => void;

  collapseCode: boolean;
  setCollapseCode: (collapse: boolean) => void;

  terminal: TerminalBlock;
}

export const RunBlock = ({
  onChange,
  setName,
  isEditable,
  onRun,
  onStop,
  setOutputVisible,
  terminal,
  setDependency,
  onCodeMirrorFocus,
  collapseCode,
  setCollapseCode,
}: RunBlockProps) => {
  let editor = useBlockNoteEditor();
  const colorMode = useStore((state) => state.functionalColorMode);
  const cleanupPtyTerm = useStore((store: AtuinState) => store.cleanupPtyTerm);
  const terminals = useStore((store: AtuinState) => store.terminals);

  const [isLoading, setIsLoading] = useState<boolean>(false);

  // commandRunning = an individual command is running
  // TODO: what to do about this??
  const [commandRunning, setCommandRunning] = useState<boolean>(false);
  const [exitCode, setExitCode] = useState<number | null>(null);
  const [commandDuration, setCommandDuration] = useState<number | null>(null);
  const [commandStart, setCommandStart] = useState<number | null>(null);
  const elementRef = useRef<HTMLDivElement>(null);
  const [isFullscreen, setIsFullscreen] = useState<boolean>(false);

  const lightModeEditorTheme = useStore((state) => state.lightModeEditorTheme);
  const darkModeEditorTheme = useStore((state) => state.darkModeEditorTheme);
  const theme = useMemo(() => {
    return colorMode === "dark" ? darkModeEditorTheme : lightModeEditorTheme;
  }, [colorMode, lightModeEditorTheme, darkModeEditorTheme]);

  const currentRunbookId = useCurrentRunbookId();

  const addPty = usePtyStore((store) => store.addPty);
  const removePty = usePtyStore((store) => store.removePty);
  const pty = usePtyStore((store) => store.ptyForBlock(terminal.id));
  const context = useBlockContext(terminal.id);
  const execution = useBlockExecution(terminal.id);
  const sshParent = context.sshHost;

  useBlockOutput<PtyMetadata>(terminal.id, (output) => {
    if (output.object) {
      const newPty = output.object;
      addPty(newPty);
      onRun?.(newPty.pid);
    } else if (output.binary && pty?.pid) {
      const terminalData = terminals[pty.pid];
      terminalData?.terminal.write(new Uint8Array(output.binary));
    }
  });

  const sshBorderClass = useMemo(() => {
    if (!sshParent) return "";

    return "border-2 border-blue-400 shadow-[0_0_10px_rgba(59,130,246,0.4)] rounded-md transition-all duration-300";
  }, [sshParent]);

  useEffect(() => {
    if (pty) {
      setCommandStart(Date.now() * 1000000);
    }

    if (!pty && commandStart) {
      logExecution(terminal, terminal.typeName, commandStart, Date.now() * 1000000, "");
      setCommandStart(null);
    }
  }, [pty]);

  useEffect(() => {
    if (!execution.isRunning && pty) {
      const terminalData = terminals[pty.pid];
      terminalData?.terminal.clear();
    }
  }, [execution.isRunning, pty]);

  const [replay, setReplay] = useState<boolean>(false);

  const onTerminalStop = useCallback(
    (replay = false) => {
      if (pty === null) return;

      removePty(pty.pid);
      terminals[pty.pid]?.dispose();
      cleanupPtyTerm(pty.pid);

      if (onStop) onStop(pty.pid);
      setCommandRunning(false);
      setExitCode(null);
      setCommandDuration(null);

      if (replay) {
        handlePlay(true);
      }
    },
    [pty, terminals, cleanupPtyTerm, onStop],
  );

  const handleStop = useCallback(async () => {
    if (!execution.isRunning) return;
    await execution.cancel();
  }, [execution.isRunning, execution.cancel]);

  const handlePlay = useCallback(
    async (force: boolean = false) => {
      if (execution.isRunning && !force) return;
      if (!terminal.code) return;

      setIsLoading(true);
      try {
        await execution.execute();
      } catch (error) {
        console.error("handlePlay error", error);
      } finally {
        setIsLoading(false);
      }

      track_event("runbooks.block.execute", { type: "terminal" });
    },
    [execution.isRunning, execution.execute, terminal.code, terminal.id, currentRunbookId, onRun],
  );

  useEffect(() => {
    if (!execution.isRunning && pty) {
      onTerminalStop();

      if (replay) {
        setReplay(false);
        handlePlay(true);
      }
    }
  }, [execution.isRunning, pty, onTerminalStop, handlePlay, replay]);

  const handleRefresh = useCallback(async () => {
    if (!execution.isRunning) return;
    if (pty === null) return;

    setReplay(true);
    await handleStop();
  }, [execution.isRunning, pty, handleStop, setReplay]);

  const handleCmdEnter: Command = useCallback(() => {
    if (!execution.isRunning) {
      handlePlay();
    } else {
      handleStop();
    }

    return true;
  }, [execution.isRunning]);

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
      className={sshBorderClass}
      hasDependency
      name={terminal.name}
      block={terminal}
      type={"Terminal"}
      setName={setName}
      inlineHeader
      setDependency={setDependency}
      hideChild={!terminal.outputVisible || !execution.isRunning}
      header={
        <>
          <div className="flex flex-row justify-between w-full">
            <h1 className="text-default-700 font-semibold">
              {
                <EditableHeading
                  initialText={terminal.name}
                  onTextChange={(text) => setName(text)}
                />
              }
            </h1>
            <div className="flex flex-row items-center gap-2">
              {execution.isRunning && commandRunning && <Spinner size="sm" />}
              {execution.isRunning && commandDuration && (
                <Chip
                  variant="flat"
                  size="sm"
                  className="pl-3 py-2"
                  startContent={<Clock size={14} />}
                  color={exitCode == 0 ? "success" : "danger"}
                >
                  {formatDuration(commandDuration)}
                </Chip>
              )}
              <Tooltip
                content={terminal.outputVisible ? "Hide output terminal" : "Show output terminal"}
              >
                <button
                  onClick={() => setOutputVisible(!terminal.outputVisible)}
                  className="p-2 hover:bg-default-100 rounded-md"
                >
                  {terminal.outputVisible ? <Eye size={20} /> : <EyeOff size={20} />}
                </button>
              </Tooltip>
              <Tooltip content={isFullscreen ? "Exit fullscreen" : "Open in fullscreen"}>
                <button
                  onClick={() => setIsFullscreen(!isFullscreen)}
                  className="p-2 hover:bg-default-100 rounded-md"
                  disabled={!terminal.outputVisible || !execution.isRunning}
                >
                  {isFullscreen ? <Minimize2 size={20} /> : <Maximize2 size={20} />}
                </button>
              </Tooltip>
              <Tooltip content={collapseCode ? "Expand code" : "Collapse code"}>
                <button
                  onClick={() => setCollapseCode(!collapseCode)}
                  className="p-2 hover:bg-default-100 rounded-md"
                >
                  {collapseCode ? (
                    <ArrowDownToLineIcon size={20} />
                  ) : (
                    <ArrowUpToLineIcon size={20} />
                  )}
                </button>
              </Tooltip>
            </div>
          </div>

          <div className="flex flex-row gap-2 flex-grow w-full" ref={elementRef}>
            <PlayButton
              isLoading={isLoading}
              isRunning={execution.isRunning}
              cancellable={true}
              onPlay={handlePlay}
              onStop={handleStop}
              onRefresh={handleRefresh}
              alwaysStop
            />
            <div
              className={cn(
                "min-w-0 flex-1 overflow-x-auto transition-all duration-300 ease-in-out relative",
                {
                  "max-h-10 overflow-hidden": collapseCode,
                },
              )}
            >
              <CodeEditor
                id={terminal.id}
                code={terminal.code}
                onChange={onChange}
                isEditable={isEditable}
                language="bash"
                theme={theme}
                onFocus={onCodeMirrorFocus}
                keyMap={[
                  TabAutoComplete,
                  {
                    key: "Mod-Enter",
                    run: handleCmdEnter,
                  },
                ]}
              />
              {collapseCode && (
                <div className="absolute bottom-0 left-0 right-0 h-6 bg-gradient-to-t from-white dark:from-gray-900 to-transparent pointer-events-none" />
              )}
            </div>
          </div>
        </>
      }
    >
      {terminal.outputVisible && (
        <>
          {!isFullscreen && pty && execution.isRunning && (
            <div className="overflow-hidden transition-all duration-300 ease-in-out min-w-0 max-h-[400px]">
              <Terminal
                pty={pty.pid}
                setCommandRunning={setCommandRunning}
                setExitCode={setExitCode}
                setCommandDuration={setCommandDuration}
              />
            </div>
          )}
        </>
      )}

      {/* Fullscreen Terminal Modal */}
      {isFullscreen && pty && (
        <div
          className="fixed inset-0 z-50 bg-black/90 backdrop-blur-md z-[9999]"
          onClick={(e) => {
            if (e.target === e.currentTarget) {
              setIsFullscreen(false);
            }
          }}
        >
          <div className="h-full bg-background overflow-hidden rounded-lg shadow-2xl flex flex-col">
            {/* Fullscreen Terminal Header */}
            <div
              data-tauri-drag-region
              className="flex justify-between items-center w-full border-default-200/50 bg-content1/95 backdrop-blur-sm flex-shrink-0"
            >
              <div
                data-tauri-drag-region
                className="flex items-center gap-3 ml-16 w-full justify-between"
              >
                <span className="text-sm text-default-700">{terminal.name || "Terminal"}</span>
                {execution.isRunning && commandRunning && <Spinner size="sm" />}
                {execution.isRunning && commandDuration && (
                  <Chip
                    variant="flat"
                    size="sm"
                    className="pl-3 py-2"
                    startContent={<Clock size={14} />}
                    color={exitCode == 0 ? "success" : "danger"}
                  >
                    {formatDuration(commandDuration)}
                  </Chip>
                )}
              </div>
              <Button isIconOnly size="sm" variant="flat" onPress={() => setIsFullscreen(false)}>
                <Minimize2 size={18} />
              </Button>
            </div>

            {/* Fullscreen Terminal Content */}
            <div className="bg-black min-h-0 flex-1 overflow-hidden">
              <Terminal
                block_id={terminal.id}
                pty={pty.pid}
                script={terminal.code}
                setCommandRunning={setCommandRunning}
                setExitCode={setExitCode}
                setCommandDuration={setCommandDuration}
                editor={editor}
                isFullscreen={true}
              />
            </div>
          </div>
        </div>
      )}
    </Block>
  );
};
