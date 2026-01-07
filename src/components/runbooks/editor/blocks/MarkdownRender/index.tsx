import { useEffect, useRef } from "react";
import { Tooltip, Button, Input, ButtonGroup } from "@heroui/react";
import {
  FileTextIcon,
  ArrowDownToLineIcon,
  ArrowUpToLineIcon,
  Maximize2,
  Minimize2,
} from "lucide-react";
import { createReactBlockSpec } from "@blocknote/react";
import { exportPropMatter } from "@/lib/utils";
import track_event from "@/tracking";
import { useBlockContext, useBlockExecution, useBlockState } from "@/lib/hooks/useDocumentBridge";
import { useBlockKvValue } from "@/lib/hooks/useKvValue";
import { cn } from "@/lib/utils";
import { MarkdownRenderState } from "@/rs-bindings/MarkdownRenderState";
import Markdown from "../../components/Markdown";

/**
 * Opens a URL in the external browser via Tauri shell API
 */
export const openExternalLink = (href: string): void => {
  import("@tauri-apps/plugin-shell").then((shell) => {
    shell.open(href);
  });
};

interface MarkdownRenderProps {
  blockId: string;
  variableName: string;
  maxLines: number;
  isEditable: boolean;
  onUpdateVariableName: (name: string) => void;
  onUpdateMaxLines: (lines: number) => void;
}

/**
 * Renders markdown content from a variable with expand/fullscreen support
 */
const MarkdownRender = (props: MarkdownRenderProps) => {
  const context = useBlockContext(props.blockId);
  const [collapsed, setCollapsed] = useBlockKvValue<boolean>(props.blockId, "collapsed", false);
  const [isFullscreen, setIsFullscreen] = useBlockKvValue<boolean>(
    props.blockId,
    "fullscreen",
    false,
  );

  const execution = useBlockExecution(props.blockId);
  const blockState = useBlockState<MarkdownRenderState>(props.blockId);

  let executionScheduled = useRef(false);
  useEffect(() => {
    async function doExecute() {
      try {
        await execution.execute();
      } catch (_err) {
      } finally {
        if (executionScheduled.current) {
          executionScheduled.current = false;
          doExecute();
        }
      }
    }

    if (execution.isRunning) {
      executionScheduled.current = true;
      return;
    }

    doExecute();
  }, [props.variableName]);

  // Get variable value
  let var_name = blockState?.resolved_variable_name || props.variableName;
  let value: string | undefined = undefined;
  if (var_name && Object.hasOwn(context.variables, var_name)) {
    value = context.variables[var_name];
  }

  // Handle ESC key to close fullscreen
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && isFullscreen) {
        setIsFullscreen(false);
      }
    };

    if (isFullscreen) {
      document.addEventListener("keydown", handleKeyDown);
      document.body.style.overflow = "hidden";
    }

    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.body.style.overflow = "";
    };
  }, [isFullscreen, setIsFullscreen]);

  const displayTitle = props.variableName || "Markdown Render";

  const source = value
    ? value
    : var_name
    ? `Variable "${var_name}" is empty`
    : "No variable selected";

  return (
    <>
      <Tooltip
        content="Render markdown content from a variable"
        delay={1000}
        className="outline-none"
      >
        <div className="flex flex-col w-full bg-gradient-to-r from-emerald-50 to-teal-50 dark:from-slate-800 dark:to-emerald-950 rounded-lg p-3 border border-emerald-200 dark:border-emerald-900 shadow-sm hover:shadow-md transition-all duration-200">
          {/* Header */}
          <div className="flex flex-row items-center justify-between mb-2">
            <span className="text-[10px] font-mono text-gray-400 dark:text-gray-500">
              markdown_render
            </span>
            <ButtonGroup size="sm">
              <Tooltip content={collapsed ? "Expand" : "Collapse"}>
                <Button
                  isIconOnly
                  variant="light"
                  size="sm"
                  onPress={() => setCollapsed(!collapsed)}
                >
                  {collapsed ? <ArrowDownToLineIcon size={16} /> : <ArrowUpToLineIcon size={16} />}
                </Button>
              </Tooltip>
              <Tooltip content="Fullscreen">
                <Button
                  isIconOnly
                  variant="light"
                  size="sm"
                  onPress={() => setIsFullscreen(true)}
                  isDisabled={!value}
                >
                  <Maximize2 size={16} />
                </Button>
              </Tooltip>
            </ButtonGroup>
          </div>

          {/* Edit controls - only shown when editable */}
          {props.isEditable ? (
            <div className="flex flex-row items-center space-x-3 mb-3">
              <div className="flex items-center">
                <Button
                  isIconOnly
                  variant="light"
                  className="bg-emerald-100 dark:bg-emerald-800 text-emerald-600 dark:text-emerald-300"
                >
                  <FileTextIcon className="h-4 w-4" />
                </Button>
              </div>

              <div className="flex-1">
                <Input
                  placeholder="Variable name"
                  value={props.variableName}
                  onValueChange={props.onUpdateVariableName}
                  autoComplete="off"
                  autoCapitalize="off"
                  autoCorrect="off"
                  spellCheck="false"
                  size="sm"
                  className="flex-1"
                />
              </div>

              <div className="w-20">
                <Input
                  type="number"
                  placeholder="Lines"
                  value={String(props.maxLines)}
                  onValueChange={(val) => {
                    const num = parseInt(val, 10);
                    if (!isNaN(num) && num > 0) {
                      props.onUpdateMaxLines(num);
                    }
                  }}
                  size="sm"
                  min={1}
                  max={100}
                  endContent={<span className="text-xs text-gray-400">lines</span>}
                />
              </div>
            </div>
          ) : (
            /* View mode - just show title */
            <div className="flex items-center gap-2 mb-3">
              <FileTextIcon className="h-4 w-4 text-emerald-600 dark:text-emerald-300" />
              <span className="text-sm font-medium text-default-700">{displayTitle}</span>
            </div>
          )}

          {/* Content area */}
          <div
            className={cn(
              "bg-white dark:bg-slate-900 rounded-md px-4 py-3 border border-emerald-200 dark:border-emerald-800 text-sm overflow-auto transition-all duration-300 ease-in-out relative select-text cursor-text",
              {
                "max-h-24 overflow-hidden": collapsed,
              },
            )}
            style={!collapsed ? { maxHeight: `${props.maxLines * 1.5}rem` } : undefined}
          >
            <Markdown content={source} />

            {/* Gradient fade when collapsed */}
            {collapsed && value && (
              <div className="absolute bottom-0 left-0 right-0 h-12 bg-gradient-to-t from-white dark:from-slate-900 to-transparent pointer-events-none" />
            )}
          </div>
        </div>
      </Tooltip>

      {/* Fullscreen Modal */}
      {isFullscreen && (
        <div
          className="fixed inset-0 bg-black/90 backdrop-blur-md z-[9999]"
          onClick={(e) => {
            if (e.target === e.currentTarget) {
              setIsFullscreen(false);
            }
          }}
        >
          <div className="h-full bg-background overflow-hidden rounded-lg shadow-2xl flex flex-col">
            {/* Fullscreen Header */}
            <div
              data-tauri-drag-region
              className="flex justify-between items-center w-full p-4 border-b border-default-200/50 bg-content1/95 backdrop-blur-sm flex-shrink-0"
            >
              <div data-tauri-drag-region className="flex items-center gap-3">
                <FileTextIcon size={20} className="text-emerald-500" />
                <span className="text-lg font-medium text-default-700">{displayTitle}</span>
              </div>
              <Button isIconOnly size="sm" variant="flat" onPress={() => setIsFullscreen(false)}>
                <Tooltip content="Exit fullscreen (ESC)">
                  <Minimize2 size={18} />
                </Tooltip>
              </Button>
            </div>

            {/* Fullscreen Content */}
            <div className="flex-1 overflow-auto p-6 select-text cursor-text">
              <div className="max-w-4xl mx-auto">
                <Markdown content={source} />
              </div>
            </div>
          </div>
        </div>
      )}
    </>
  );
};

/**
 * Insert helper for the slash command menu
 */
export const insertMarkdownRender = (editor: any) => ({
  title: "Markdown Render",
  subtext: "Render markdown content from a variable",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "markdown_render" });
    editor.insertBlocks(
      [{ type: "markdown_render", props: { variableName: "", maxLines: 12 } }],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <FileTextIcon size={18} />,
  aliases: ["markdown", "md", "render", "display"],
  group: "Content",
});

/**
 * BlockNote block specification for the MarkdownRender component
 */
export default createReactBlockSpec(
  {
    type: "markdown_render",
    propSchema: {
      variableName: { default: "" },
      maxLines: { default: 12 },
    },
    content: "none",
  },
  {
    toExternalHTML: ({ block }) => {
      let propMatter = exportPropMatter("markdown_render", block.props, [
        "variableName",
        "maxLines",
      ]);
      return (
        <pre lang="markdown_render">
          <code>{propMatter}</code>
        </pre>
      );
    },
    // @ts-ignore
    render: ({ block, editor }) => {
      const onUpdateVariableName = (variableName: string): void => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, variableName },
        });
      };

      const onUpdateMaxLines = (maxLines: number): void => {
        editor.updateBlock(block, {
          // @ts-ignore
          props: { ...block.props, maxLines },
        });
      };

      return (
        <MarkdownRender
          blockId={block.id}
          variableName={block.props.variableName}
          maxLines={block.props.maxLines}
          onUpdateVariableName={onUpdateVariableName}
          onUpdateMaxLines={onUpdateMaxLines}
          isEditable={editor.isEditable}
        />
      );
    },
  },
);
