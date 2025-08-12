import { DatabaseIcon } from "lucide-react";
import { useCallback } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";

// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";

import { SQLiteBlock } from "@/lib/workflow/blocks/sqlite";
import { DependencySpec } from "@/lib/workflow/dependency";
import track_event from "@/tracking";
import SQL from "@/lib/blocks/common/SQL";
import { exportPropMatter } from "@/lib/utils";
import { BlockOutput } from "@/rs-bindings/BlockOutput";
import { useStore } from "@/state/store";

interface SQLiteProps {
  isEditable: boolean;
  collapseQuery: boolean;
  sqlite: SQLiteBlock;
  editor: any;

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
  editor,
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
  const [currentRunbookId] = useStore((state) => [state.currentRunbookId]);

  // Custom runQuery function - clean version without Promise wrapper
  const runQuery = useCallback(
    async (onResult: any, onError: any) => {
      // Create a channel for output streaming
      const outputChannel = new Channel<BlockOutput>();

      // Set up output handler
      outputChannel.onmessage = (output: BlockOutput) => {
        console.log("SQLite output:", output);

        // Handle lifecycle events
        if (output.lifecycle) {
          switch (output.lifecycle.type) {
            case "started":
              console.log("SQLite execution started");
              break;
            case "finished":
              console.log(`SQLite execution finished, success: ${output.lifecycle.data.success}`);
              break;
            case "cancelled":
              console.log("SQLite execution was cancelled");
              break;
            case "error":
              console.error("SQLite execution error:", output.lifecycle.data.message);
              onError(output.lifecycle.data.message);
              return; // Don't continue processing after error
          }
        }

        // Handle structured JSON object data (success case)
        if (output.object && typeof output.object === "object" && output.object !== null) {
          const parsed = output.object as any;
          let queryResult = {
            time: new Date(),
            columns: parsed.columns?.map((col: string) => ({ name: col, type: "" })) || null,
            rows: parsed.rows || null,
            rowsAffected: parsed?.rowsAffected,
            lastInsertID: parsed?.lastInsertId,
            duration: 0,
          };
          onResult(queryResult);
        }
      };

      try {
        // Execute the block using the generic command
        await invoke<string>("execute_block", {
          blockId: sqlite.id,
          runbookId: currentRunbookId || "",
          editorDocument: editor.document,
          outputChannel,
        });
      } catch (err: any) {
        console.error("Failed to execute SQLite query:", err);
        onError(err.message || "Failed to execute query");
      }
    },
    [sqlite.id, currentRunbookId, editor],
  );

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
      runQuery={runQuery}
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
      collapseQuery: { default: false },
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

      const setCollapseQuery = (collapseQuery: boolean) => {
        editor.updateBlock(block, {
          props: { ...block.props, collapseQuery: collapseQuery },
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
          editor={editor}
          setDependency={setDependency}
          setName={setName}
          setUri={setUri}
          setQuery={setQuery}
          setAutoRefresh={setAutoRefresh}
          isEditable={editor.isEditable}
          collapseQuery={block.props.collapseQuery}
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

