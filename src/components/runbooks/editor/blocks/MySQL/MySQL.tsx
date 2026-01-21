import { DatabaseIcon } from "lucide-react";
import { Switch } from "@heroui/react";

// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";
import undent from "undent";
import AIBlockRegistry from "@/lib/ai/block_registry";

import { langs } from "@uiw/codemirror-extensions-langs";

import { MySqlBlock } from "@/lib/workflow/blocks/mysql";
import { DependencySpec } from "@/lib/workflow/dependency";
import track_event from "@/tracking";
import SQL from "@/lib/blocks/common/SQL";
import { exportPropMatter } from "@/lib/utils";
import { useBlockKvValue } from "@/lib/hooks/useKvValue";

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
  setSkipSqlModeInit: (skip: boolean) => void;
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
  setSkipSqlModeInit,
  onCodeMirrorFocus,
}: SQLProps) => {
  const settingsContent = (
    <div className="flex items-center justify-between gap-4">
      <div className="flex flex-col">
        <span className="text-sm font-medium text-gray-700 dark:text-gray-300">
          Skip SQL mode initialization
        </span>
        <span className="text-xs text-gray-500 dark:text-gray-400">
          Enable for MySQL-compatible databases like StarRocks or Doris
        </span>
      </div>
      <Switch
        size="sm"
        isSelected={mysql.skipSqlModeInit}
        onValueChange={setSkipSqlModeInit}
        isDisabled={!isEditable}
      />
    </div>
  );

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
      settingsContent={settingsContent}
      settingsTitle="MySQL Settings"
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
      skipSqlModeInit: { default: false },
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

      const setSkipSqlModeInit = (skip: boolean) => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, skipSqlModeInit: skip },
        });
      };

      let dependency = DependencySpec.deserialize(block.props.dependency);
      let mysql = new MySqlBlock(
        block.id,
        block.props.name,
        dependency,
        block.props.query,
        block.props.uri,
        block.props.autoRefresh,
        block.props.skipSqlModeInit,
      );

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
          setSkipSqlModeInit={setSkipSqlModeInit}
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

AIBlockRegistry.getInstance().addBlock({
  typeName: "mysql",
  friendlyName: "MySQL",
  shortDescription: "Executes SQL queries against a MySQL database.",
  description: undent`
    MySQL blocks execute SQL queries against a MySQL database and display results in an interactive table.

    The available props are:
    - name (string): The display name of the block
    - query (string): The SQL query to execute
    - uri (string): MySQL connection string (e.g., mysql://user:pass@host:port/db)
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
      "type": "mysql",
      "props": {
        "name": "Recent Logs",
        "uri": "{{ var.mysql_uri }}",
        "query": "SELECT * FROM logs ORDER BY created_at DESC LIMIT 100"
      }
    }
  `,
});
