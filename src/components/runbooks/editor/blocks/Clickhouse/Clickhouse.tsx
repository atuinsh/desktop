import { DatabaseIcon } from "lucide-react";

// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";
import undent from "undent";
import AIBlockRegistry from "@/lib/ai/block_registry";

import { ClickhouseBlock } from "@/lib/workflow/blocks/clickhouse";
import { DependencySpec } from "@/lib/workflow/dependency";
import track_event from "@/tracking";
import SQL from "@/lib/blocks/common/SQL";
import { exportPropMatter } from "@/lib/utils";
import { useBlockKvValue } from "@/lib/hooks/useKvValue";

interface SQLProps {
  isEditable: boolean;
  collapseQuery: boolean;
  clickhouse: ClickhouseBlock;

  setQuery: (query: string) => void;
  setUri: (uri: string) => void;
  setAutoRefresh: (autoRefresh: number) => void;
  setName: (name: string) => void;
  setCollapseQuery: (collapseQuery: boolean) => void;
  setDependency: (dependency: DependencySpec) => void;
  onCodeMirrorFocus?: () => void;
}

const Clickhouse = ({
  clickhouse,
  setQuery,
  setUri,
  setAutoRefresh,
  isEditable,
  setName,
  collapseQuery,
  setCollapseQuery,
  setDependency,
  onCodeMirrorFocus,
}: SQLProps) => {
  return (
    <SQL
      block={clickhouse}
      id={clickhouse.id}
      sqlType="clickhouse"
      name={clickhouse.name}
      setName={setName}
      query={clickhouse.query}
      setQuery={setQuery}
      uri={clickhouse.uri}
      setUri={setUri}
      autoRefresh={clickhouse.autoRefresh}
      setAutoRefresh={setAutoRefresh}
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
    type: "clickhouse",
    propSchema: {
      name: { default: "Clickhouse" },
      query: { default: "" },
      uri: { default: "" },
      autoRefresh: { default: 0 },
      dependency: { default: "{}" },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      let propMatter = exportPropMatter("clickhouse", block.props, ["name", "uri"]);
      return (
        <div>
          <pre lang="sql">
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
      let clickhouse = new ClickhouseBlock(
        block.id,
        block.props.name,
        dependency,
        block.props.query,
        block.props.uri,
        block.props.autoRefresh,
      );

      return (
        <Clickhouse
          clickhouse={clickhouse}
          setName={setName}
          setUri={setUri}
          setQuery={setQuery}
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

export const insertClickhouse = (schema: any) => (editor: typeof schema.BlockNoteEditor) => ({
  title: "Clickhouse",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "clickhouse" });

    let clickhouseBlocks = editor.document.filter((block: any) => block.type === "clickhouse");
    let name = `Clickhouse ${clickhouseBlocks.length + 1}`;

    editor.insertBlocks(
      [
        {
          type: "clickhouse",
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
  typeName: "clickhouse",
  friendlyName: "ClickHouse",
  shortDescription: "Executes SQL queries against a ClickHouse database.",
  description: undent`
    ClickHouse blocks execute SQL queries against a ClickHouse database and display results in an interactive table.

    The available props are:
    - name (string): The display name of the block
    - query (string): The SQL query to execute
    - uri (string): ClickHouse connection string
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
      "type": "clickhouse",
      "props": {
        "name": "Analytics Query",
        "uri": "{{ var.clickhouse_uri }}",
        "query": "SELECT count() FROM events WHERE date >= today() - 7"
      }
    }
  `,
});
