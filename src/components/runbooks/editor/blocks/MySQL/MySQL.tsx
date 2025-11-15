import { DatabaseIcon } from "lucide-react";

// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";

import { langs } from "@uiw/codemirror-extensions-langs";

import { MySqlBlock } from "@/lib/workflow/blocks/mysql";
import { DependencySpec } from "@/lib/workflow/dependency";
import track_event from "@/tracking";
import SQL from "@/lib/blocks/common/SQL";
import { exportPropMatter } from "@/lib/utils";
import { useBlockLocalState } from "@/lib/hooks/useBlockLocalState";

interface SQLProps {
  isEditable: boolean;
  collapseQuery: boolean;
  mysql: MySqlBlock;

  setCollapseQuery: (collapseQuery: boolean) => void;
  setQuery: (query: string) => void;
  setUri: (uri: string) => void;
  setAutoRefresh: (autoRefresh: number) => void;
  setName: (name: string) => void;
  setDependency: (dependency: DependencySpec) => void;
  onCodeMirrorFocus?: () => void;
}

const MySQL = ({
  mysql,
  setName,
  setQuery,
  setUri,
  setAutoRefresh,
  isEditable,
  collapseQuery,
  setCollapseQuery,
  setDependency,
  onCodeMirrorFocus,
}: SQLProps) => {
  return (
    <SQL
      block={mysql}
      id={mysql.id}
      sqlType="mysql"
      name={mysql.name}
      setName={setName}
      query={mysql.query}
      setQuery={setQuery}
      uri={mysql.uri}
      setUri={setUri}
      autoRefresh={mysql.autoRefresh}
      setAutoRefresh={setAutoRefresh}
      extensions={[langs.mysql()]}
      isEditable={isEditable}
      collapseQuery={collapseQuery}
      setCollapseQuery={setCollapseQuery}
      setDependency={setDependency}
      onCodeMirrorFocus={onCodeMirrorFocus}
    />
  );
};

export default createReactBlockSpec(
  {
    type: "mysql",
    propSchema: {
      name: { default: "MySQL" },
      query: { default: "" },
      uri: { default: "" },
      autoRefresh: { default: 0 },
      dependency: { default: "{}" },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      let propMatter = exportPropMatter("mysql", block.props, ["name", "uri"]);
      return (
        <pre lang="mysql">
          <code>
            {propMatter}
            {block.props.query}
          </code>
        </pre>
      );
    },
    // @ts-ignore
    render: ({ block, editor, code, type }) => {
      const [collapseQuery, setCollapseQuery] = useBlockLocalState<boolean>(
        block.id,
        "collapsed",
        false
      );

      const handleCodeMirrorFocus = () => {
        // Ensure BlockNote knows which block contains the focused CodeMirror
        editor.setTextCursorPosition(block.id, "start");
      };

      const setQuery = (query: string) => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, query: query },
        });
      };

      const setUri = (uri: string) => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, uri: uri },
        });
      };

      const setAutoRefresh = (autoRefresh: number) => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, autoRefresh: autoRefresh },
        });
      };

      const setName = (name: string) => {
        editor.updateBlock(block, {
          props: { ...block.props, name: name },
        });
      };

      const setDependency = (dependency: DependencySpec) => {
        editor.updateBlock(block, {
          props: { ...block.props, dependency: dependency.serialize() },
        });
      };

      let dependency = DependencySpec.deserialize(block.props.dependency);
      let mysql = new MySqlBlock(block.id, block.props.name, dependency, block.props.query, block.props.uri, block.props.autoRefresh);

      return (
        <MySQL
          mysql={mysql}
          setName={setName}
          setQuery={setQuery}
          setUri={setUri}
          setAutoRefresh={setAutoRefresh}
          isEditable={editor.isEditable}
          collapseQuery={collapseQuery}
          setCollapseQuery={setCollapseQuery}
          setDependency={setDependency}
          onCodeMirrorFocus={handleCodeMirrorFocus}
        />
      );
    },
  },
);

export const insertMySQL = (schema: any) => (editor: typeof schema.BlockNoteEditor) => ({
  title: "MySQL",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "mysql" });
    
    let mysqlBlocks = editor.document.filter((block: any) => block.type === "mysql");
    let name = `MySQL ${mysqlBlocks.length + 1}`;

    editor.insertBlocks(
      [
        {
          type: "mysql",
          // @ts-ignore
          props: {
            name: name,
          },
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <DatabaseIcon size={18} />,
  group: "Database",
});
