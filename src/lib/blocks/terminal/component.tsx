// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";

import "./index.css";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { invoke } from "@tauri-apps/api/core";
import { platform } from "@tauri-apps/plugin-os";

import { useBlockNoteEditor } from "@blocknote/react";

import "@xterm/xterm/css/xterm.css";
import { AtuinState, useStore } from "@/state/store.ts";
import { addToast, Button, Chip, Spinner, Tooltip } from "@heroui/react";
import { formatDuration, cn } from "@/lib/utils.ts";
import { usePtyStore } from "@/state/ptyStore.ts";
import track_event from "@/tracking.ts";
import { Clock, Eye, EyeOff, Maximize2, Minimize2, ArrowDownToLineIcon, ArrowUpToLineIcon } from "lucide-react";
import EditableHeading from "@/components/EditableHeading/index.tsx";
import { templateString } from "@/state/templates.ts";
import CodeEditor, { TabAutoComplete } from "../common/CodeEditor/CodeEditor.tsx";
import { Command } from "@codemirror/view";
import { TerminalBlock } from "./schema.ts";
import { logExecution } from "@/lib/exec_log.ts";
import { DependencySpec } from "@/lib/workflow/dependency.ts";
import { convertBlocknoteToAtuin } from "@/lib/workflow/blocks/convert.ts";
import BlockBus from "@/lib/workflow/block_bus.ts";
import {
  useBlockBusRunSubscription,
  useBlockBusStopSubscription,
} from "@/lib/hooks/useBlockBus.ts";
import { uuidv7 } from "uuidv7";
import { useBlockDeleted, useBlockInserted } from "@/lib/buses/editor.ts";
import { Settings } from "@/state/settings";
import Terminal from "./components/terminal.tsx";
import { findAllParentsOfType, findFirstParentOfType, getCurrentDirectory } from "../exec.ts";
import Block from "../common/Block.tsx";
import { default as BlockType } from "@/lib/workflow/blocks/block.ts";
import PlayButton from "../common/PlayButton.tsx";
import { useCurrentRunbookId } from "@/context/runbook_id_context.ts";

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

  // isRunning = a terminal is running
  const [isRunning, setIsRunning] = useState<boolean>(false);
  // isLoading - we're waiting for a pty to open. usually network related
  const [isLoading, setIsLoading] = useState<boolean>(false);

  // commandRunning = an individual command is running
  const [commandRunning, setCommandRunning] = useState<boolean>(false);
  const [exitCode, setExitCode] = useState<number | null>(null);
  const [commandDuration, setCommandDuration] = useState<number | null>(null);
  const [commandStart, setCommandStart] = useState<number | null>(null);
  const [parentBlock, setParentBlock] = useState<BlockType | null>(null);
  const elementRef = useRef<HTMLDivElement>(null);
  const [isFullscreen, setIsFullscreen] = useState<boolean>(false);

  const unsubscribeNameChanged = useRef<(() => void) | null>(null);
  const unsubscribeDependencyChanged = useRef<(() => void) | null>(null);

  const lightModeEditorTheme = useStore((state) => state.lightModeEditorTheme);
  const darkModeEditorTheme = useStore((state) => state.darkModeEditorTheme);
  const theme = useMemo(() => {
    return colorMode === "dark" ? darkModeEditorTheme : lightModeEditorTheme;
  }, [colorMode, lightModeEditorTheme, darkModeEditorTheme]);



  const currentRunbookId = useCurrentRunbookId();

  const pty = usePtyStore((store) => store.ptyForBlock(terminal.id));
  const [sshParent, setSshParent] = useState<any | null>(null);

  const updateSshParent = useCallback(() => {
    let host = findFirstParentOfType(editor, terminal.id, ["ssh-connect", "host-select"]);
    if (host?.type === "ssh-connect") {
      setSshParent(host);
    } else {
      setSshParent(null);
    }
  }, [editor, terminal.id]);

  useEffect(updateSshParent, []);

  useBlockInserted("ssh-connect", updateSshParent);
  useBlockInserted("host-select", updateSshParent);
  useBlockDeleted("ssh-connect", updateSshParent);
  useBlockDeleted("host-select", updateSshParent);

  const sshBorderClass = useMemo(() => {
    if (!sshParent) return "";

    return "border-2 border-blue-400 shadow-[0_0_10px_rgba(59,130,246,0.4)] rounded-md transition-all duration-300";
  }, [sshParent]);

  useEffect(() => {
    setIsRunning(pty != null);

    if (pty) {
      setCommandStart(Date.now() * 1000000);

      if (elementRef.current) {
        elementRef.current.scrollIntoView({ behavior: "smooth", block: "center" });
      }
    }

    if (!pty && commandStart) {
      logExecution(terminal, terminal.typeName, commandStart, Date.now() * 1000000, "");
      BlockBus.get().blockFinished(terminal);
      setCommandStart(null);
    }
  }, [pty]);

  const handleStop = useCallback(async () => {
    console.log("handleStop", pty);
    if (pty === null) return;

    await invoke("pty_kill", { pid: pty.pid, runbook: currentRunbookId });

    terminals[pty.pid].dispose();
    cleanupPtyTerm(pty.pid);

    if (onStop) onStop(pty.pid);

    setCommandRunning(false);
    setExitCode(null);
    setCommandDuration(null);
  }, [pty, currentRunbookId, terminal.id, terminals, cleanupPtyTerm, onStop]);

  const openPty = async (): Promise<string> => {
    let cwd = await getCurrentDirectory(editor, terminal.id, currentRunbookId);

    let vars = findAllParentsOfType(editor, terminal.id, "env");
    let env: { [key: string]: string } = {};

    for (var i = 0; i < vars.length; i++) {
      let name = await templateString(
        terminal.id,
        vars[i].props.name,
        editor.document,
        currentRunbookId,
      );
      let value = await templateString(
        terminal.id,
        vars[i].props.value,
        editor.document,
        currentRunbookId,
      );
      env[name] = value;
    }

    // Check for SSH block or Host block, prioritizing SSH if both exist
    let connectionBlock = findFirstParentOfType(editor, terminal.id, [
      "ssh-connect",
      "host-select",
    ]);

    // If SSH block found, use SSH connection
    if (connectionBlock && connectionBlock.type === "ssh-connect") {
      let pty = uuidv7();
      let user: string | undefined;
      let host: string;

      // Handle both "user@host" and just "host" formats
      if (connectionBlock.props.userHost.includes("@")) {
        [user, host] = connectionBlock.props.userHost.split("@");
      } else {
        // No username specified, let SSH config determine it
        user = undefined;
        host = connectionBlock.props.userHost;
      }

      try {
        await invoke<void>("ssh_open_pty", {
          host: host,
          username: user,
          channel: pty,
          runbook: currentRunbookId,
          block: terminal.id,
          width: 80,
          height: 24,
        });
      } catch (error) {
        console.error(error);
        setIsLoading(false);
        addToast({
          title: `ssh ${connectionBlock.props.userHost}`,
          description: `${error}`,
          color: "danger",
        });
      }
      return pty;
    }

    // Default to local execution if Host block found or no connection block found
    // Get the custom shell from settings if available
    const customShell = await Settings.terminalShell();

    try {
      let pty = await invoke<string>("pty_open", {
        cwd,
        env,
        runbook: currentRunbookId,
        block: terminal.id,
        shell: customShell,
      });
      return pty;
    } catch (error) {
      console.error(error);
      setIsLoading(false);
      addToast({
        title: `Terminal error`,
        description: `${error}`,
        color: "danger",
      });
      throw error;
    }
  };

  const handlePlay = useCallback(
    async (force: boolean = false) => {
      if (isRunning && !force) return;
      if (!terminal.code) return;

      await invoke("workflow_block_start_event", {
        workflow: currentRunbookId,
        block: terminal.id,
      });

      setIsLoading(true);
      try {
        let p = await openPty();
        setIsLoading(false);

        if (onRun) onRun(p);
      } catch (error) {
        // Error is already handled in openPty
        setIsLoading(false);
      }

      track_event("runbooks.block.execute", { type: "terminal" });
    },
    [isRunning, terminal.code, terminal.id, currentRunbookId, onRun, openPty],
  );

  const handleRefresh = async () => {
    if (!isRunning) return;
    if (pty === null) return;

    let terminalData = terminals[pty.pid];

    let isWindows = platform() == "windows";
    let cmdEnd = isWindows ? "\r\n" : "\n";
    let val = !terminal.code.endsWith("\n") ? terminal.code + cmdEnd : terminal.code;

    terminalData.terminal.clear();
    terminalData.write(terminal.id, val, editor.document, currentRunbookId);
  };

  const handleCmdEnter: Command = useCallback(() => {
    if (!isRunning) {
      handlePlay();
    } else {
      handleStop();
    }

    return true;
  }, [isRunning]);

  const refreshParentBlock = () => {
    if (!terminal.dependency.parent) {
      setParentBlock(null);
      return;
    }

    if (parentBlock && parentBlock.id === terminal.dependency.parent) {
      return;
    }

    let bnb = editor.document.find((b: any) => b.id === terminal.dependency.parent);
    if (bnb) {
      let block = convertBlocknoteToAtuin(bnb);
      setParentBlock(block);
    }
  };

  useEffect(() => {
    if (!terminal.dependency.parent) {
      setParentBlock(null);
      return;
    }

    if (parentBlock && parentBlock.id === terminal.dependency.parent) {
      return;
    }

    if (unsubscribeDependencyChanged.current) {
      unsubscribeDependencyChanged.current();
    }
    if (unsubscribeNameChanged.current) {
      unsubscribeNameChanged.current();
    }

    unsubscribeDependencyChanged.current = BlockBus.get().subscribeDependencyChanged(
      terminal.dependency.parent,
      refreshParentBlock,
    );
    unsubscribeNameChanged.current = BlockBus.get().subscribeNameChanged(
      terminal.dependency.parent,
      refreshParentBlock,
    );

    refreshParentBlock();

    return () => {
      if (unsubscribeDependencyChanged.current) {
        unsubscribeDependencyChanged.current();
      }
      if (unsubscribeNameChanged.current) {
        unsubscribeNameChanged.current();
      }
    };
  }, [terminal.dependency.parent]);

  useBlockBusRunSubscription(terminal.id, handlePlay);
  useBlockBusStopSubscription(terminal.id, handleStop);

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
      hideChild={!terminal.outputVisible || !isRunning}
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
              {isRunning && commandRunning && <Spinner size="sm" />}
              {isRunning && commandDuration && (
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
                  disabled={!terminal.outputVisible || !isRunning}
                >
                  {isFullscreen ? <Minimize2 size={20} /> : <Maximize2 size={20} />}
                </button>
              </Tooltip>
              <Tooltip content={collapseCode ? "Expand code" : "Collapse code"}>
                <button
                  onClick={() => setCollapseCode(!collapseCode)}
                  className="p-2 hover:bg-default-100 rounded-md"
                >
                  {collapseCode ? <ArrowDownToLineIcon size={20} /> : <ArrowUpToLineIcon size={20} />}
                </button>
              </Tooltip>
            </div>
          </div>

          <div className="flex flex-row gap-2 flex-grow w-full" ref={elementRef}>
            <PlayButton
              isLoading={isLoading}
              isRunning={isRunning}
              cancellable={true}
              onPlay={handlePlay}
              onStop={handleStop}
              onRefresh={handleRefresh}
              alwaysStop
            />
            <div className={cn("min-w-0 flex-1 overflow-x-auto transition-all duration-300 ease-in-out relative", {
              "max-h-10 overflow-hidden": collapseCode,
            })}>
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
          {!isFullscreen && pty && isRunning && (
            <div className="overflow-hidden transition-all duration-300 ease-in-out min-w-0 max-h-[400px]">
              <Terminal
                block_id={terminal.id}
                pty={pty.pid}
                script={terminal.code}
                setCommandRunning={setCommandRunning}
                setExitCode={setExitCode}
                setCommandDuration={setCommandDuration}
                editor={editor}
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
                {isRunning && commandRunning && <Spinner size="sm" />}
                {isRunning && commandDuration && (
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
