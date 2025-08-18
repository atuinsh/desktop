import { DatabaseIcon } from "lucide-react";
import { useCallback } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";

// @ts-ignore
import { createReactBlockSpec } from "@blocknote/react";

import { ClickhouseBlock } from "@/lib/workflow/blocks/clickhouse";
import { DependencySpec } from "@/lib/workflow/dependency";
import track_event from "@/tracking";
import SQL from "@/lib/blocks/common/SQL";
import { exportPropMatter } from "@/lib/utils";
import { BlockOutput } from "@/rs-bindings/BlockOutput";
import { useStore } from "@/state/store";

interface SQLProps {
  isEditable: boolean;
  collapseQuery: boolean;
  clickhouse: ClickhouseBlock;
  editor: any;

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
}: SQLProps) => {
  const [currentRunbookId] = useStore((state) => [state.currentRunbookId]);

  // Custom runQuery function - clean version without Promise wrapper
  const runQuery = useCallback(
    async (onResult: any, onError: any) => {
      // Create a channel for output streaming
      const outputChannel = new Channel<BlockOutput>();

      // Set up output handler
      outputChannel.onmessage = (output: BlockOutput) => {
        console.log("Clickhouse output:", output);

        // Handle lifecycle events
        if (output.lifecycle) {
          switch (output.lifecycle.type) {
            case "started":
              console.log("Clickhouse execution started");
              break;
            case "finished":
              console.log(`Clickhouse execution finished, success: ${output.lifecycle.data.success}`);
              break;
            case "cancelled":
              console.log("Clickhouse execution was cancelled");
              break;
            case "error":
              console.error("Clickhouse execution error:", output.lifecycle.data.message);
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
          blockId: clickhouse.id,
          runbookId: currentRunbookId || "",
          editorDocument: editor.document,
          outputChannel,
        });
      } catch (err: any) {
        console.error("Failed to execute Clickhouse query:", err);
        onError(err.message || "Failed to execute query");
      }
    },
    [clickhouse.id, currentRunbookId, editor],
  );

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
      runQuery={runQuery}
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
      collapseQuery: { default: false },
      dependency: { default: "{}"},
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
      let clickhouse = new ClickhouseBlock(
        block.id, 
        block.props.name, 
        dependency,
        block.props.query, 
        block.props.uri, 
        block.props.autoRefresh
      );

      return (
        <Clickhouse
          clickhouse={clickhouse}
          editor={editor}
          setName={setName}
          setUri={setUri}
          setQuery={setQuery}
          setAutoRefresh={setAutoRefresh}
          isEditable={editor.isEditable}
          collapseQuery={block.props.collapseQuery}
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