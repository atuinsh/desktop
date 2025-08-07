// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";

import "./index.css";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { invoke, Channel } from "@tauri-apps/api/core";
import { platform } from "@tauri-apps/plugin-os";

import { useBlockNoteEditor } from "@blocknote/react";

import "@xterm/xterm/css/xterm.css";
import { AtuinState, useStore } from "@/state/store.ts";
import { addToast, Button, Chip, Spinner, Tooltip } from "@heroui/react";
import { formatDuration } from "@/lib/utils.ts";
import track_event from "@/tracking.ts";
import { Clock, Eye, EyeOff, Maximize2, Minimize2 } from "lucide-react";
import EditableHeading from "@/components/EditableHeading/index.tsx";
import CodeEditor, { TabAutoComplete } from "../common/CodeEditor/CodeEditor.tsx";
import { Command } from "@codemirror/view";
import { TerminalBlock } from "./schema.ts";
import { DependencySpec } from "@/lib/workflow/dependency.ts";
import {
  useBlockBusRunSubscription,
  useBlockBusStopSubscription,
} from "@/lib/hooks/useBlockBus.ts";
import { useBlockDeleted, useBlockInserted } from "@/lib/buses/editor.ts";
import TerminalComponent from "./components/terminal.tsx";
import { findFirstParentOfType } from "../exec.ts";
import Block from "../common/Block.tsx";
import PlayButton from "../common/PlayButton.tsx";
import { BlockOutput } from "@/rs-bindings/BlockOutput";
import { useTerminalEvents } from "./useTerminalEvents";

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
}: RunBlockProps) => {
  let editor = useBlockNoteEditor();
  const colorMode = useStore((state) => state.functionalColorMode);

  // TerminalData is the single source of truth
  const [terminalData, setTerminalData] = useState<any | null>(null);
  const elementRef = useRef<HTMLDivElement>(null);
  const [isFullscreen, setIsFullscreen] = useState<boolean>(false);

  // Use custom hook for event handling and state management
  const { isLoading, setIsLoading, isRunning, exitCode, commandDuration } = useTerminalEvents(terminalData, terminal);
  
  // Don't show spinner during terminal session, only during loading
  const commandRunning = false;



  const lightModeEditorTheme = useStore((state) => state.lightModeEditorTheme);
  const darkModeEditorTheme = useStore((state) => state.darkModeEditorTheme);
  const theme = useMemo(() => {
    return colorMode === "dark" ? darkModeEditorTheme : lightModeEditorTheme;
  }, [colorMode, lightModeEditorTheme, darkModeEditorTheme]);

  const [currentRunbookId] = useStore((store: AtuinState) => [store.currentRunbookId]);

  const [sshParent, setSshParent] = useState<any | null>(null);

  // Connect to existing TerminalData
  useEffect(() => {
    const { terminals } = useStore.getState();
    const existingTerminal = terminals[terminal.id];
    
    if (existingTerminal) {
      console.log("Found existing terminal data for", terminal.id);
      setTerminalData(existingTerminal);
    }
  }, [terminal.id]);

  // Event handling is now managed by useTerminalEvents hook

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

  // Event cleanup is handled by useTerminalEvents hook

  const sshBorderClass = useMemo(() => {
    if (!sshParent) return "";

    return "border-2 border-blue-400 shadow-[0_0_10px_rgba(59,130,246,0.4)] rounded-md transition-all duration-300";
  }, [sshParent]);

  const handleStop = useCallback(async () => {
    console.log("handleStop", terminalData?.executionId);
    if (!terminalData?.executionId) return;

    try {
      // Cancel the block execution
      await invoke("cancel_block_execution", { 
        executionId: terminalData.executionId 
      });
    } catch (error) {
      console.error("Error stopping terminal:", error);
    }
    
    if (onStop) onStop(terminal.id);

    // TerminalData will handle state updates via events
  }, [terminalData, terminal.id, currentRunbookId, onStop]);

  const handlePlay = useCallback(
    async (force: boolean = false) => {
      if (isRunning && !force) return;
      if (!terminal.code) return;

      setIsLoading(true);
      
      try {
        // Create channel for BlockOutput
        const outputChannel = new Channel<BlockOutput>();
        console.log("Created channel with ID:", outputChannel.id);
        
        // Create or get TerminalData and set up BlockOutput handling
        const { newPtyTerm } = useStore.getState();
        const td = await newPtyTerm(terminal.id, outputChannel);
        
        // Set up event listeners immediately on the TerminalData object
        const handleExecutionStarted = () => {
          console.log("Terminal execution started (immediate)");
          setIsLoading(false);
        };
        
        td.on('execution_started', handleExecutionStarted);
        
        // Set TerminalData for the custom hook to manage other events
        setTerminalData(td);
        
        const executionIdResult = await invoke<string>('execute_block', {
          blockId: terminal.id,
          runbookId: currentRunbookId,
          editorDocument: editor.document,
          outputChannel: outputChannel
        });
        
        td.setExecutionId(executionIdResult);

        if (onRun) onRun(terminal.id);
        
      } catch (error) {
        console.error("Error starting terminal:", error);
        setIsLoading(false);
        addToast({
          title: "Terminal error",
          description: `${error}`,
          color: "danger",
        });
      }

      track_event("runbooks.block.execute", { type: "terminal" });
    },
    [isRunning, terminal.code, terminal.id, currentRunbookId, onRun, editor.document]
  );

  const handleRefresh = async () => {
    if (!isRunning) return;

    let isWindows = platform() == "windows";
    let cmdEnd = isWindows ? "\r\n" : "\n";
    let val = !terminal.code.endsWith("\n") ? terminal.code + cmdEnd : terminal.code;
    
    await invoke('pty_write', {
      pid: terminal.id,
      data: val
    });
  };

  const handleCmdEnter: Command = useCallback(() => {
    if (!isRunning) {
      handlePlay();
    } else {
      handleStop();
    }

    return true;
  }, [isRunning, handlePlay, handleStop]);

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
          </div>
        </>
      }
    >
      {terminal.outputVisible && (
        <>
          {!isFullscreen && isRunning && (
            <TerminalComponent
              block_id={terminal.id}
              pty={terminal.id}
              script={terminal.code}
              runScript={false}
              setCommandRunning={() => {}} // No-op: state managed by TerminalData
              setExitCode={() => {}} // No-op: state managed by TerminalData
              setCommandDuration={() => {}} // No-op: state managed by TerminalData
              editor={editor}
              isFullscreen={false}
            />
          )}
        </>
      )}

      {/* Fullscreen Terminal Modal */}
      {isFullscreen && isRunning && (
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
              <TerminalComponent
                block_id={terminal.id}
                pty={terminal.id}
                script={terminal.code}
                runScript={false}
                setCommandRunning={() => {}} // No-op: state managed by TerminalData
                setExitCode={() => {}} // No-op: state managed by TerminalData
                setCommandDuration={() => {}} // No-op: state managed by TerminalData
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
