import { DatabaseIcon } from "lucide-react";

// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";
import undent from "undent";
import AIBlockRegistry from "@/lib/ai/block_registry";

import { SQLiteBlock } from "@/lib/workflow/blocks/sqlite";
import { DependencySpec } from "@/lib/workflow/dependency";
import track_event from "@/tracking";
import SQL from "@/lib/blocks/common/SQL";
import { exportPropMatter } from "@/lib/utils";
import { useBlockKvValue } from "@/lib/hooks/useKvValue";

interface SQLiteProps {
  isEditable: boolean;
  collapseQuery: boolean;
  sqlite: SQLiteBlock;

  setQuery: (query: string) => void;
  setUri: (uri: string) => void;
  setAutoRefresh: (autoRefresh: number) => void;
  setName: (name: string) => void;
  setCollapseQuery: (collapseQuery: boolean) => void;
  setDependency: (dependency: DependencySpec) => void;
  onCodeMirrorFocus?: () => void;
}

const SQLite = ({
  sqlite,
  setQuery,
  setUri,
  setAutoRefresh,
  isEditable,
  setName,
  collapseQuery,
  setCollapseQuery,
  setDependency,
  onCodeMirrorFocus,
}: SQLiteProps) => {
  return (
    <SQL
      block={sqlite}
      id={sqlite.id}
      sqlType="sqlite"
      name={sqlite.name}
      setName={setName}
      query={sqlite.query}
      setQuery={setQuery}
      uri={sqlite.uri}
      setUri={setUri}
      autoRefresh={sqlite.autoRefresh}
      setAutoRefresh={setAutoRefresh}
      isEditable={isEditable}
      collapseQuery={collapseQuery}
      setCollapseQuery={setCollapseQuery}
      setDependency={setDependency}
      onCodeMirrorFocus={onCodeMirrorFocus}
      placeholder="/path/to/database.db"
    />
  );
};

export default createReactBlockSpec(
  {
    type: "sqlite",
    propSchema: {
      name: { default: "SQLite" },
      query: { default: "" },
      uri: { default: "" },
      autoRefresh: { default: 0 },
      dependency: { default: "{}" },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      let propMatter = exportPropMatter("sqlite", block.props, ["name", "uri"]);
      return (
        <div>
          <pre lang="sqlite">
            <code>
              {propMatter}
              {block.props.query}
            </code>
          </pre>
        </div>
      );
    },
    // @ts-ignore
    render: ({ block, editor, code, type }) => {
      const [collapseQuery, setCollapseQuery] = useBlockKvValue<boolean>(
        block.id,
        "collapsed",
        false,
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
      let sqlite = new SQLiteBlock(
        block.id,
        block.props.name,
        dependency,
        block.props.query,
        block.props.uri,
        block.props.autoRefresh,
      );

      return (
        <SQLite
          sqlite={sqlite}
          setDependency={setDependency}
          setName={setName}
          setUri={setUri}
          setQuery={setQuery}
          setAutoRefresh={setAutoRefresh}
          isEditable={editor.isEditable}
          collapseQuery={collapseQuery}
          setCollapseQuery={setCollapseQuery}
          onCodeMirrorFocus={handleCodeMirrorFocus}
        />
      );
    },
  },
);

export const insertSQLite = (schema: any) => (editor: typeof schema.BlockNoteEditor) => ({
  title: "SQLite",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "sqlite" });

    let sqliteBlocks = editor.document.filter((block: any) => block.type === "sqlite");
    let name = `SQLite ${sqliteBlocks.length + 1}`;

    editor.insertBlocks(
      [
        {
          type: "sqlite",
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

AIBlockRegistry.getInstance().addBlock({
  typeName: "sqlite",
  friendlyName: "SQLite",
  shortDescription: "Executes SQL queries against a SQLite database.",
  description: undent`
    SQLite blocks execute SQL queries against a local SQLite database file and display results in an interactive table.

    The available props are:
    - name (string): The display name of the block
    - query (string): The SQL query to execute
    - uri (string): Path to the SQLite database file
    - autoRefresh (number): Auto-refresh interval in milliseconds (0 to disable)

    You can reference template variables in the query and uri: {{ var.variable_name }}.

    OUTPUT ACCESS (requires block to have a name):
    - output.rows (array): Rows from the first SELECT query
    - output.columns (array): Column names
    - output.total_rows (number): Total row count
    - output.total_rows_affected (number): Rows affected by INSERT/UPDATE/DELETE
    - output.total_duration (number): Execution time in seconds
    - output.results (array): All results for multi-statement queries

    MULTI-STATEMENT QUERIES:
    Multiple statements separated by semicolons are supported. Access via output.results[index].

    Example: {
      "type": "sqlite",
      "props": {
        "name": "Users Table",
        "uri": "/path/to/database.db",
        "query": "SELECT * FROM users WHERE active = 1"
      }
    }
  `,
});
