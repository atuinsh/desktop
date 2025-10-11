import { DatabaseIcon } from "lucide-react";

// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";

import { langs } from "@uiw/codemirror-extensions-langs";

import { runQuery } from "./query";

import { PostgresBlock } from "@/lib/workflow/blocks/postgres";
import { DependencySpec } from "@/lib/workflow/dependency";
import track_event from "@/tracking";
import SQL from "@/lib/blocks/common/SQL";
import { exportPropMatter } from "@/lib/utils";
import { useBlockLocalState } from "@/lib/hooks/useBlockLocalState";

interface SQLProps {
  isEditable: boolean;
  collapseQuery: boolean;
  postgres: PostgresBlock;

  setCollapseQuery: (collapseQuery: boolean) => void;
  setQuery: (query: string) => void;
  setUri: (uri: string) => void;
  setAutoRefresh: (autoRefresh: number) => void;
  setName: (name: string) => void;
  setDependency: (dependency: DependencySpec) => void;
  onCodeMirrorFocus?: () => void;
}

const Postgres = ({
  postgres,
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
      block={postgres}
      id={postgres.id}
      sqlType="postgres"
      name={postgres.name}
      setName={setName}
      query={postgres.query}
      setQuery={setQuery}
      uri={postgres.uri}
      setUri={setUri}
      autoRefresh={postgres.autoRefresh}
      setAutoRefresh={setAutoRefresh}
      runQuery={runQuery}
      extensions={[langs.pgsql()]}
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
    type: "postgres",
    propSchema: {
      name: { default: "PostgreSQL" },
      query: { default: "" },
      uri: { default: "" },
      autoRefresh: { default: 0 },
      dependency: { default: "{}" },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      let propMatter = exportPropMatter("postgres", block.props, ["name", "uri"]);
      return (
        <pre lang="postgres">
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
      let postgres = new PostgresBlock(block.id, block.props.name, dependency, block.props.query, block.props.uri, block.props.autoRefresh);

      return (
        <Postgres
          postgres={postgres}
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

export const insertPostgres = (schema: any) => (editor: typeof schema.BlockNoteEditor) => ({
  title: "PostgreSQL",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "postgres" });
    
    let postgresBlocks = editor.document.filter((block: any) => block.type === "postgres");
    let name = `PostgreSQL ${postgresBlocks.length + 1}`;

    editor.insertBlocks(
      [
        {
          type: "postgres",
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
