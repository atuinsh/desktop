// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";

import { useMemo, useState, useEffect, useRef, useCallback, useContext } from "react";

import { useStore } from "@/state/store.ts";
import { Button, Input, Tooltip } from "@heroui/react";
import {
  FileTerminalIcon,
  Eye,
  EyeOff,
  TriangleAlertIcon,
  ArrowDownToLineIcon,
  ArrowUpToLineIcon,
} from "lucide-react";
import EditableHeading from "@/components/EditableHeading/index.tsx";

import { Command } from "@codemirror/view";
import { ScriptBlock as ScriptBlockType } from "@/lib/workflow/blocks/script.ts";
import { default as BlockType } from "@/lib/workflow/blocks/block.ts";
import { convertBlocknoteToAtuin } from "@/lib/workflow/blocks/convert.ts";
import { DependencySpec } from "@/lib/workflow/dependency.ts";
import BlockBus from "@/lib/workflow/block_bus.ts";
import track_event from "@/tracking";
import { invoke } from "@tauri-apps/api/core";
import { Settings } from "@/state/settings.ts";
import PlayButton from "@/lib/blocks/common/PlayButton.tsx";
import CodeEditor, { TabAutoComplete } from "@/lib/blocks/common/CodeEditor/CodeEditor.tsx";
import Block from "@/lib/blocks/common/Block.tsx";
import InterpreterSelector, { supportedShells } from "@/lib/blocks/common/InterpreterSelector.tsx";
import { exportPropMatter, cn } from "@/lib/utils";
import { useBlockLocalState } from "@/lib/hooks/useBlockLocalState";
import {
  GenericBlockOutput,
  useBlockContext,
  useBlockExecution,
  useBlockOutput,
  useBlockStart,
  useBlockStop,
} from "@/lib/hooks/useDocumentBridge";
import Xterm, { XtermHandle } from "@/components/runbooks/editor/components/Xterm";
import { TabsContext } from "@/routes/root/Tabs";

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

  collapseCode: boolean;
  setCollapseCode: (collapse: boolean) => void;

  script: ScriptBlockType;
}

// Now using the supportedShells from InterpreterSelector

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
  collapseCode,
  setCollapseCode,
}: ScriptBlockProps) => {
  const [hasRun, setHasRun] = useState<boolean>(false);
  const xtermRef = useRef<XtermHandle>(null);
  // Track available shells
  const [availableShells, setAvailableShells] = useState<Record<string, boolean>>({});

  // Check if selected shell is missing
  const shellMissing = useMemo(() => {
    // These shells are always available
    if (script.interpreter === "bash" || script.interpreter === "sh") return false;

    // Check if shell is in our supported list but not available
    return script.interpreter in availableShells && !availableShells[script.interpreter];
  }, [script.interpreter, availableShells]);

  const { incrementBadge, decrementBadge } = useContext(TabsContext);
  const colorMode = useStore((state) => state.functionalColorMode);
  const [parentBlock, setParentBlock] = useState<BlockType | null>(null);
  const elementRef = useRef<HTMLDivElement>(null);
  const lightModeEditorTheme = useStore((state) => state.lightModeEditorTheme);
  const darkModeEditorTheme = useStore((state) => state.darkModeEditorTheme);
  const theme = useMemo(() => {
    return colorMode === "dark" ? darkModeEditorTheme : lightModeEditorTheme;
  }, [colorMode, lightModeEditorTheme, darkModeEditorTheme]);

  const blockExecution = useBlockExecution(script.id);
  const blockContext = useBlockContext(script.id);
  const sshParent = blockContext.sshHost;

  const onBlockOutput = useCallback(async (output: GenericBlockOutput<void>) => {
    if (output.stdout) {
      xtermRef.current?.write(output.stdout);
    }
    if (output.binary) {
      xtermRef.current?.write(new Uint8Array(output.binary));
    }
    if (output.stderr) {
      xtermRef.current?.write(output.stderr);
    }
  }, []);

  useBlockOutput<void>(script.id, onBlockOutput);
  useBlockStart(script.id, () => {
    setHasRun(true);
    xtermRef.current?.clear();
    incrementBadge(1);
  });
  useBlockStop(script.id, () => {
    decrementBadge(1);
  });

  // Class name for SSH indicator styling based on connection status
  const blockBorderClass = useMemo(() => {
    // Check output variable name first
    const hasOutputVarError =
      script.outputVariable && !/^[a-zA-Z0-9_]*$/.test(script.outputVariable);
    if (hasOutputVarError) {
      return "border-1 border-red-400 shadow-[0_0_10px_rgba(239,68,68,0.4)] rounded-lg transition-all duration-300";
    }

    if (shellMissing) {
      return "border-1 border-red-400 shadow-[0_0_10px_rgba(239,68,68,0.4)] rounded-lg transition-all duration-300";
    }

    if (sshParent) {
      return "border-1 border-blue-400 shadow-[0_0_10px_rgba(59,130,246,0.4)] rounded-lg transition-all duration-300";
    }

    return "border-1";
  }, [sshParent, shellMissing, script.outputVariable]);

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

  const handlePlay = useCallback(async () => {
    if (blockExecution.isRunning) return;

    await blockExecution.execute();
  }, [blockExecution]);

  const handleCmdEnter: Command = useCallback(() => {
    if (!blockExecution.isRunning) {
      handlePlay();
    } else {
      blockExecution.cancel();
    }

    return true;
  }, [handlePlay, blockExecution.cancel, blockExecution.isRunning]);

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
      hideChild={!script.outputVisible || !hasRun}
      topRightElement={topRightWarning}
      header={
        <>
          <div className="flex flex-row justify-between w-full">
            <h1 className="text-default-700 font-semibold">
              <EditableHeading
                initialText={script.name || "Script"}
                onTextChange={(text) => setName(text)}
              />
            </h1>

            <div className="flex flex-row items-center gap-2" ref={elementRef}>
              <Input
                size="sm"
                variant="flat"
                className={`max-w-[250px] ${
                  script.outputVariable && !/^[a-zA-Z0-9_]*$/.test(script.outputVariable)
                    ? "border-red-400 dark:border-red-400 focus:ring-red-500"
                    : ""
                }`}
                placeholder="Output variable"
                autoComplete="off"
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck="false"
                value={script.outputVariable}
                onValueChange={(val) => setOutputVariable(val)}
                isInvalid={
                  !!script.outputVariable && !/^[a-zA-Z0-9_]*$/.test(script.outputVariable)
                }
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
                    setOutputVisible(!script.outputVisible);
                  }}
                  size="sm"
                  variant="flat"
                  isIconOnly
                >
                  {script.outputVisible ? <Eye size={20} /> : <EyeOff size={20} />}
                </Button>
              </Tooltip>

              <Tooltip content={collapseCode ? "Expand code" : "Collapse code"}>
                <Button
                  onPress={() => setCollapseCode(!collapseCode)}
                  size="sm"
                  variant="flat"
                  isIconOnly
                >
                  {collapseCode ? (
                    <ArrowDownToLineIcon size={20} />
                  ) : (
                    <ArrowUpToLineIcon size={20} />
                  )}
                </Button>
              </Tooltip>
            </div>
          </div>

          <div className="flex flex-row gap-2 flex-grow w-full overflow-x-auto">
            <Tooltip
              content={
                shellMissing
                  ? `${script.interpreter} shell not found. This script may not run correctly.`
                  : ""
              }
              isDisabled={!shellMissing}
              color="danger"
            >
              <div>
                <PlayButton
                  eventName="runbooks.block.execute"
                  eventProps={{ type: "script" }}
                  onPlay={handlePlay}
                  onStop={blockExecution.cancel}
                  isRunning={blockExecution.isRunning}
                  cancellable={true}
                />
              </div>
            </Tooltip>

            <div
              className={cn(
                "min-w-0 flex-1 overflow-x-auto transition-all duration-300 ease-in-out relative",
                {
                  "max-h-10 overflow-hidden": collapseCode,
                },
              )}
            >
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
              {collapseCode && (
                <div className="absolute bottom-0 left-0 right-0 h-6 bg-gradient-to-t from-white dark:from-gray-900 to-transparent pointer-events-none" />
              )}
            </div>
          </div>
        </>
      }
    >
      <Xterm
        ref={xtermRef}
        className="min-h-[200px] w-full"
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
      const [collapseCode, setCollapseCode] = useBlockLocalState<boolean>(
        block.id,
        "collapsed",
        false,
      );

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
          collapseCode={collapseCode}
          setCollapseCode={setCollapseCode}
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
