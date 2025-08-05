// @ts-ignore
import { createReactBlockSpec, useEditorChange, useEditorContentOrSelectionChange } from "@blocknote/react";

import { useMemo, useState, useEffect, useCallback, useRef } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";

import { useStore } from "@/state/store.ts";
import { Button, Input, Tooltip } from "@heroui/react";
import { FileTerminalIcon, Eye, EyeOff, TriangleAlertIcon } from "lucide-react";
import EditableHeading from "@/components/EditableHeading/index.tsx";

import { Command } from "@codemirror/view";
import { ScriptBlock as ScriptBlockType } from "@/lib/workflow/blocks/script.ts";
import { default as BlockType } from "@/lib/workflow/blocks/block.ts";
import { convertBlocknoteToAtuin } from "@/lib/workflow/blocks/convert.ts";
import { DependencySpec } from "@/lib/workflow/dependency.ts";
import BlockBus from "@/lib/workflow/block_bus.ts";
import track_event from "@/tracking";
import { Settings } from "@/state/settings.ts";
import CodeEditor, { TabAutoComplete } from "@/lib/blocks/common/CodeEditor/CodeEditor.tsx";
import Block from "@/lib/blocks/common/Block.tsx";
import InterpreterSelector, { supportedShells } from "@/lib/blocks/common/InterpreterSelector.tsx";
import { exportPropMatter } from "@/lib/utils.ts";
import PlayButton from "@/lib/blocks/common/PlayButton.tsx";

interface BlockOutput {
  stdout?: string;
  stderr?: string;
  lifecycle?: BlockLifecycleEvent;
}

type BlockLifecycleEvent =
  | { type: "started" }
  | { type: "finished"; exitCode?: number; success: boolean }
  | { type: "cancelled" }
  | { type: "error"; message: string };

interface ScriptBlockProps {
  onChange: (val: string) => void;
  setName: (name: string) => void;
  isEditable: boolean;
  editor: any;
  setInterpreter: (interpreter: string) => void;

  setOutputVariable: (outputVariable: string) => void;
  setOutputVisible: (visible: boolean) => void;
  setDependency: (dependency: DependencySpec) => void;
  onCodeMirrorFocus?: () => void;

  script: ScriptBlockType;
}

const ScriptBlock = ({
  onChange,
  setInterpreter,
  setName,
  isEditable,
  setOutputVariable,
  setOutputVisible,
  setDependency,
  editor,
  script,
  onCodeMirrorFocus,
}: ScriptBlockProps) => {
  const [isRunning, setIsRunning] = useState<boolean>(false);
  const [hasRun, setHasRun] = useState<boolean>(false);
  const [executionId, setExecutionId] = useState<string | null>(null);
  // Track available shells
  const [availableShells, setAvailableShells] = useState<Record<string, boolean>>({});

  // Terminal state
  const [terminal, setTerminal] = useState<Terminal | null>(null);
  const terminalDOMRef = useRef<HTMLDivElement>(null);
  const [currentRunbookId] = useStore((state) => [state.currentRunbookId]);

  // Check if selected shell is missing
  const shellMissing = useMemo(() => {
    // These shells are always available
    if (script.interpreter === "bash" || script.interpreter === "sh") return false;

    // Check if shell is in our supported list but not available
    return script.interpreter in availableShells && !availableShells[script.interpreter];
  }, [script.interpreter, availableShells]);

  const colorMode = useStore((state) => state.functionalColorMode);
  const [parentBlock, setParentBlock] = useState<BlockType | null>(null);
  const lightModeEditorTheme = useStore((state) => state.lightModeEditorTheme);
  const darkModeEditorTheme = useStore((state) => state.darkModeEditorTheme);
  const theme = useMemo(() => {
    return colorMode === "dark" ? darkModeEditorTheme : lightModeEditorTheme;
  }, [colorMode, lightModeEditorTheme, darkModeEditorTheme]);

  // Class name for validation styling
  const blockBorderClass = useMemo(() => {
    // Check output variable name first
    const hasOutputVarError = script.outputVariable && !/^[a-zA-Z0-9_]*$/.test(script.outputVariable);
    if (hasOutputVarError) {
      return "border-1 border-red-400 shadow-[0_0_10px_rgba(239,68,68,0.4)] rounded-lg transition-all duration-300";
    }

    if (shellMissing) {
      return "border-1 border-red-400 shadow-[0_0_10px_rgba(239,68,68,0.4)] rounded-lg transition-all duration-300";
    }

    return "border-1";
  }, [shellMissing, script.outputVariable]);

  // For the shell warning message in the top right
  const topRightWarning = useMemo(() => {
    if (shellMissing) {
      return (
        <div className="flex items-center gap-1 text-[10px] font-medium text-red-500">
          <div className="flex items-center">
            <TriangleAlertIcon size={16} />
          </div>
          {script.interpreter} not found
        </div>
      );
    }
    return null;
  }, [shellMissing, script.interpreter]);

  // Initialize terminal when needed
  const initializeTerminal = useCallback(async () => {
    if (terminal || !terminalDOMRef.current) return terminal;


    const term = new Terminal({
      fontFamily: "FiraCode, monospace",
      fontSize: 14,
      convertEol: true,
      theme: colorMode === "dark" ? {
        background: '#1a1b26',
        foreground: '#a9b1d6',
      } : undefined,
    });

    const fit = new FitAddon();
    term.loadAddon(fit);

    // Add WebGL support if enabled
    const useWebGL = await Settings.terminalGL();
    if (useWebGL) {
      try {
        const webglAddon = new WebglAddon();
        term.loadAddon(webglAddon);
      } catch (e) {
        console.warn("WebGL addon failed to load", e);
      }
    }

    // Attach to DOM
    term.open(terminalDOMRef.current);
    fit.fit();

    setTerminal(term);


    return term;
  }, [terminal, colorMode]);

  // Clean up terminal when output becomes hidden
  useEffect(() => {
    if (!script.outputVisible && terminal) {
      terminal.dispose();
      setTerminal(null);
    }
  }, [script.outputVisible, terminal]);

  // Check which shells are installed
  useEffect(() => {
    const checkShellsAvailable = async () => {
      try {
        const shellStatus: Record<string, boolean> = {};

        // Check each supported shell
        for (const shell of supportedShells) {
          // Skip bash and sh as they're always available
          if (shell.name === "bash" || shell.name === "sh") {
            shellStatus[shell.name] = true;
            continue;
          }

          // Check each possible path for this shell
          let found = false;
          for (const path of shell.paths) {
            try {
              const exists = await invoke<boolean>("check_binary_exists", { path });
              if (exists) {
                found = true;
                break;
              }
            } catch (e) {
              console.error(`Error checking ${path}:`, e);
            }
          }

          shellStatus[shell.name] = found;
        }

        setAvailableShells(shellStatus);
      } catch (error) {
        console.error("Failed to check available shells:", error);
      }
    };

    checkShellsAvailable();
  }, [supportedShells]);

  // handle dependency change
  useEffect(() => {
    if (!script.dependency.parent) {
      setParentBlock(null);
      return;
    }

    if (parentBlock && parentBlock.id === script.dependency.parent) {
      return;
    }

    let bnb = editor.document.find((b: any) => b.id === script.dependency.parent);
    if (bnb) {
      let block = convertBlocknoteToAtuin(bnb);
      setParentBlock(block);
    }
  }, [script.dependency]);

  const handleLifecycleEvent = useCallback((event: BlockLifecycleEvent) => {
    console.log("Lifecycle event:", event);

    switch (event.type) {
      case "started":
        console.log("Script execution started");
        break;
      case "finished":
        console.log(`Script execution finished with exit code ${event.exitCode}, success: ${event.success}`);
        setIsRunning(false);
        setExecutionId(null);
        break;
      case "cancelled":
        console.log("Script execution was cancelled");
        setIsRunning(false);
        setExecutionId(null);
        break;
      case "error":
        console.error("Script execution error:", event.message);
        setIsRunning(false);
        setExecutionId(null);
        break;
    }
  }, []);

  const handlePlay = useCallback(async () => {

    if (isRunning) return;

    setIsRunning(true);
    setHasRun(true);
    setExecutionId(null); // Clear any previous execution ID

    // Initialize terminal if needed
    const activeTerminal = await initializeTerminal();

    activeTerminal?.clear();

    try {
      // Create a channel for output streaming
      const outputChannel = new Channel<BlockOutput>();

      // Set up output handler
      outputChannel.onmessage = (output: BlockOutput) => {
        console.log(output);
        if (output.stdout) {
          activeTerminal?.write(output.stdout);
        }
        if (output.stderr) {
          activeTerminal?.write(`\x1b[31m${output.stderr}\x1b[0m`); // Red color for stderr
        }
        if (output.lifecycle) {
          handleLifecycleEvent(output.lifecycle);
        }
      };

      // Execute the block using the generic command
      const result = await invoke<string>("execute_block", {
        blockId: script.id,
        runbookId: currentRunbookId,
        editorDocument: editor.document,
        outputChannel,
      });

      console.log("Execution started:", result);

      // Extract execution ID from result (format: "Execution started with handle: <uuid>")
      const match = result.match(/handle: ([a-f0-9-]{36})/);
      if (match) {
        setExecutionId(match[1]);
      }
    } catch (error) {
      console.error("Failed to execute script:", error);
      activeTerminal?.write(`\x1b[31mError: ${error}\x1b[0m\n`);
      setIsRunning(false);
      setExecutionId(null);
    }
  }, [script, editor, currentRunbookId, isRunning, initializeTerminal]);

  const handleStop = useCallback(async () => {
    if (!executionId) {
      console.log("No execution ID available for cancellation");
      setIsRunning(false);
      return;
    }

    try {
      await invoke("cancel_block_execution", {
        executionId: executionId,
      });
      console.log("Script execution cancelled");
    } catch (error) {
      console.error("Failed to cancel script execution:", error);
    }

    setIsRunning(false);
    setExecutionId(null);
  }, [script, executionId]);

  // Note: Block execution is now handled entirely through the channel-based approach
  // No need for BlockBus subscriptions since we get lifecycle events directly from the backend

  const handleCmdEnter: Command = useCallback(() => {
    if (!isRunning) {
      handlePlay();
    } else {
      handleStop();
    }

    return true;
  }, [handlePlay, handleStop, isRunning]);

  // Border styling and validation handled in the blockBorderClass useMemo
  return (
    <Block
      hasDependency
      block={script}
      setDependency={setDependency}
      name={script.name}
      type={"Script"}
      setName={setName}
      inlineHeader
      className={blockBorderClass}

      topRightElement={topRightWarning}
      header={
        <>
          <div className="flex flex-row justify-between w-full">
            <h1 className="text-default-700 font-semibold">
              {
                <EditableHeading
                  initialText={script.name || "Script"}
                  onTextChange={(text) => setName(text)}
                />
              }
            </h1>

            <div className="flex flex-row items-center gap-2">
              <Input
                size="sm"
                variant="flat"
                className={`max-w-[250px] ${script.outputVariable && !/^[a-zA-Z0-9_]*$/.test(script.outputVariable) ? 'border-red-400 dark:border-red-400 focus:ring-red-500' : ''}`}
                placeholder="Output variable"
                autoComplete="off"
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck="false"
                value={script.outputVariable}
                onValueChange={(val) => setOutputVariable(val)}
                isInvalid={!!script.outputVariable && !/^[a-zA-Z0-9_]*$/.test(script.outputVariable)}
                errorMessage={"Variable names can only contain letters, numbers, and underscores"}
              />

              <InterpreterSelector
                interpreter={script.interpreter}
                onInterpreterChange={setInterpreter}
                size="sm"
                variant="flat"
              />

              <Tooltip
                content={script.outputVisible ? "Hide output terminal" : "Show output terminal"}
              >
                <Button
                  onPress={() => {
                    setHasRun(false);
                    setOutputVisible(!script.outputVisible);
                  }}
                  size="sm"
                  variant="flat"
                  isIconOnly
                >
                  {script.outputVisible ? <Eye size={20} /> : <EyeOff size={20} />}
                </Button>
              </Tooltip>
            </div>
          </div>

          <div className="flex flex-row gap-2 flex-grow w-full overflow-x-auto">
            <PlayButton
              eventName="runbooks.block.execute"
              eventProps={{ type: "script" }}
              onPlay={handlePlay}
              onStop={handleStop}
              isRunning={isRunning}
              cancellable={true}
            />

            <div className="min-w-0 flex-1 overflow-x-auto">
              <CodeEditor
                id={script.id}
                code={script.code}
                isEditable={isEditable}
                language={script.interpreter}
                theme={theme}
                onChange={onChange}
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
          </div>
        </>
      }
    >
      {/* Always render div for terminal attachment, but only show height when needed */}
      <div
        ref={terminalDOMRef}
        className={`w-full ${script.outputVisible && hasRun ? 'min-h-[200px]' : 'h-0 overflow-hidden'}`}
      />
    </Block>
  );
};

export default createReactBlockSpec(
  {
    type: "script",
    propSchema: {
      interpreter: {
        default: "zsh",
      },
      outputVariable: {
        default: "",
      },
      name: {
        default: "",
      },
      code: { default: "" },
      outputVisible: {
        default: true,
      },
      dependency: {
        default: "{}",
      },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      let propMatter = exportPropMatter("script", block.props, ["name", "interpreter"]);
      return (
        <pre lang="script">
          <code>
            {propMatter}
            {block.props.code}
          </code>
        </pre>
      );
    },
    // @ts-ignore
    render: ({ block, editor }) => {
      const handleCodeMirrorFocus = () => {
        // Ensure BlockNote knows which block contains the focused CodeMirror
        editor.setTextCursorPosition(block.id, "start");
      };

      const onCodeChange = (val: string) => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, code: val },
        });
      };

      const setName = (name: string) => {
        editor.updateBlock(block, {
          props: { ...block.props, name: name },
        });

        BlockBus.get().nameChanged(
          new ScriptBlockType(
            block.id,
            name,
            DependencySpec.deserialize(block.props.dependency),
            block.props.code,
            block.props.interpreter,
            block.props.outputVariable,
            block.props.outputVisible,
          ),
        );
      };

      const setInterpreter = (interpreter: string) => {
        editor.updateBlock(block, {
          props: { ...block.props, interpreter: interpreter },
        });
      };

      const setOutputVariable = (outputVariable: string) => {
        editor.updateBlock(block, {
          props: { ...block.props, outputVariable: outputVariable },
        });
      };

      const setOutputVisible = (visible: boolean) => {
        editor.updateBlock(block, {
          props: { ...block.props, outputVisible: visible },
        });
      };

      const setDependency = (dependency: DependencySpec) => {
        editor.updateBlock(block, {
          props: { ...block.props, dependency: dependency.serialize() },
        });

        BlockBus.get().dependencyChanged(
          new ScriptBlockType(
            block.id,
            block.props.name,
            dependency,
            block.props.code,
            block.props.interpreter,
            block.props.outputVariable,
            block.props.outputVisible,
          ),
        );
      };

      let dependency = DependencySpec.deserialize(block.props.dependency);
      let script = new ScriptBlockType(
        block.id,
        block.props.name,
        dependency,
        block.props.code,
        block.props.interpreter,
        block.props.outputVariable,
        block.props.outputVisible,
      );

      return (
        <ScriptBlock
          script={script}
          setName={setName}
          onChange={onCodeChange}
          setInterpreter={setInterpreter}
          isEditable={editor.isEditable}
          editor={editor}
          setOutputVariable={setOutputVariable}
          setOutputVisible={setOutputVisible}
          setDependency={setDependency}
          onCodeMirrorFocus={handleCodeMirrorFocus}
        />
      );
    },
  },
);

export const insertScript = (schema: any) => (editor: typeof schema.BlockNoteEditor) => ({
  title: "Script",
  subtext: "Non-interactive script",
  onItemClick: async () => {
    track_event("runbooks.block.create", { type: "script" });

    let scriptBlocks = editor.document.filter((block: any) => block.type === "script");
    let name = `Script ${scriptBlocks.length + 1}`;

    // Get default shell from settings
    const defaultShell = await Settings.scriptShell();
    const interpreter = defaultShell || "zsh";

    editor.insertBlocks(
      [
        {
          type: "script",
          // @ts-ignore
          props: {
            name: name,
            interpreter: interpreter,
          },
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <FileTerminalIcon size={18} />,
  group: "Execute",
});
