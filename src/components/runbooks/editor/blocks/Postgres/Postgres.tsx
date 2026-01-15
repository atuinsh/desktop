import { DatabaseIcon } from "lucide-react";

// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";
import undent from "undent";
import AIBlockRegistry from "@/lib/ai/block_registry";

import { langs } from "@uiw/codemirror-extensions-langs";

import { PostgresBlock } from "@/lib/workflow/blocks/postgres";
import { DependencySpec } from "@/lib/workflow/dependency";
import track_event from "@/tracking";
import SQL from "@/lib/blocks/common/SQL";
import { exportPropMatter } from "@/lib/utils";
import { useBlockKvValue } from "@/lib/hooks/useKvValue";

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
      let postgres = new PostgresBlock(
        block.id,
        block.props.name,
        dependency,
        block.props.query,
        block.props.uri,
        block.props.autoRefresh,
      );

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

AIBlockRegistry.getInstance().addBlock({
  typeName: "postgres",
  friendlyName: "PostgreSQL",
  shortDescription: "Executes SQL queries against a PostgreSQL database.",
  description: undent`
    PostgreSQL blocks execute SQL queries against a PostgreSQL database and display results in an interactive table.

    The available props are:
    - name (string): The display name of the block
    - query (string): The SQL query to execute
    - uri (string): PostgreSQL connection string (e.g., postgres://user:pass@host:port/db)
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
      "type": "postgres",
      "props": {
        "name": "Active Orders",
        "uri": "{{ var.postgres_uri }}",
        "query": "SELECT * FROM orders WHERE status = 'active'"
      }
    }
  `,
});
