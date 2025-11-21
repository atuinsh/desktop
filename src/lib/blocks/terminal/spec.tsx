import { TERMINAL_BLOCK_SCHEMA, TerminalBlock } from "@/lib/blocks/terminal";
import { DependencySpec } from "@/lib/workflow/dependency";
import { createReactBlockSpec } from "@blocknote/react";
import { RunBlock } from "./component";
import track_event from "@/tracking";
import { CodeIcon } from "lucide-react";
import { exportPropMatter } from "@/lib/utils";
import { useBlockLocalState } from "@/lib/hooks/useBlockLocalState";

export default createReactBlockSpec(TERMINAL_BLOCK_SCHEMA, {
  // @ts-ignore
  render: ({ block, editor, code, type }) => {
    const [collapseCode, setCollapseCode] = useBlockLocalState<boolean>(
      block.id,
      "collapsed",
      false,
    );

    const handleCodeMirrorFocus = () => {
      // Ensure BlockNote knows which block contains the focused CodeMirror
      editor.setTextCursorPosition(block.id, "start");
    };

    const onInputChange = (val: string) => {
      editor.updateBlock(block, {
        // @ts-ignore
        props: { ...block.props, code: val },
      });
    };

    const setName = (name: string) => {
      editor.updateBlock(block, {
        props: { ...block.props, name: name },
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
    };

    const setTerminalRows = (rows: number) => {
      editor.updateBlock(block, {
        props: { ...block.props, terminalRows: rows },
      });
    };

    let dependency = DependencySpec.deserialize(block.props.dependency);
    let terminal = new TerminalBlock(
      block.id,
      block.props.name,
      dependency,
      block.props.code,
      block.props.outputVisible,
    );

    return (
      <RunBlock
        setName={setName}
        onChange={onInputChange}
        type={block.props.type}
        pty={block.props.pty}
        isEditable={editor.isEditable}
        setOutputVisible={setOutputVisible}
        terminal={terminal}
        setDependency={setDependency}
        onCodeMirrorFocus={handleCodeMirrorFocus}
        collapseCode={collapseCode}
        setCollapseCode={setCollapseCode}
        terminalRows={block.props.terminalRows}
        setTerminalRows={setTerminalRows}
      />
    );
  },
  toExternalHTML: ({ block }) => {
    let propMatter = exportPropMatter("terminal", block.props, ["name"]);
    return (
      <pre lang="beep boop">
        <code lang="bash">
          {propMatter}
          {block?.props?.code}
        </code>
      </pre>
    );
  },
});

export const insertTerminal = (editor: any) => ({
  title: "Terminal",
  subtext: "Interactive terminal",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "run" });

    // Count the number of terminal blocks
    let terminalBlocks = editor.document.filter((block: any) => block.type === "run");
    let name = `Terminal ${terminalBlocks.length + 1}`;

    editor.insertBlocks(
      [
        {
          type: "run",
          props: {
            name,
          },
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <CodeIcon size={18} />,
  aliases: ["terminal", "run"],
  group: "Execute",
});
