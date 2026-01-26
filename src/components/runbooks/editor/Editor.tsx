import "./index.css";

import { Spinner } from "@heroui/react";

import { filterSuggestionItems } from "@blocknote/core/extensions";

import {
  SuggestionMenuController,
  getDefaultReactSlashMenuItems,
  SideMenu,
  SideMenuController,
  AddBlockButton,
  DragHandleButton,
} from "@blocknote/react";
import { BlockNoteView } from "@blocknote/mantine";

import "@blocknote/core/fonts/inter.css";
import "@blocknote/mantine/style.css";

import {
  FolderOpenIcon,
  VariableIcon,
  TextCursorInputIcon,
  EyeIcon,
  LinkIcon,
  BlocksIcon,
  MinusIcon,
  ClipboardPasteIcon,
  AlertCircleIcon,
} from "lucide-react";

import { AIGeneratePopup } from "./AIGeneratePopup";
import { AILoadingOverlay } from "./ui/AILoadingBlock";
import { AIFocusOverlay } from "./ui/AIFocusOverlay";
import { AIHint } from "./ui/AIHint";
import { RunbookLinkPopup } from "./ui/RunbookLinkPopup";
import AIAssistant, { AIContext } from "./ui/AIAssistant";
import { SparklesIcon } from "lucide-react";

import { insertSQLite } from "@/components/runbooks/editor/blocks/SQLite/SQLite";
import { insertPostgres } from "@/components/runbooks/editor/blocks/Postgres/Postgres";
import { insertMySQL } from "@/components/runbooks/editor/blocks/MySQL/MySQL";
import { insertClickhouse } from "@/components/runbooks/editor/blocks/Clickhouse/Clickhouse";
import { insertScript } from "@/components/runbooks/editor/blocks/Script/Script";
import { insertPrometheus } from "@/components/runbooks/editor/blocks/Prometheus/Prometheus";
import { insertEditor } from "@/components/runbooks/editor/blocks/Editor/Editor";
import { insertSshConnect } from "@/components/runbooks/editor/blocks/ssh/SshConnect";
import { insertHostSelect } from "@/components/runbooks/editor/blocks/Host";
import { insertLocalVar } from "@/components/runbooks/editor/blocks/LocalVar";
import { insertMarkdownRender } from "@/components/runbooks/editor/blocks/MarkdownRender";
import { insertTableOfContents } from "@/components/runbooks/editor/blocks/TableOfContents";

import Runbook from "@/state/runbooks/runbook";
import { insertHttp } from "@/lib/blocks/http";
import { uuidv7 } from "uuidv7";
import { DuplicateBlockItem } from "./ui/DuplicateBlockItem";
import { CopyBlockItem } from "./ui/CopyBlockItem";

import { schema } from "./create_editor";
import RunbookEditor from "@/lib/runbook_editor";
import { useStore } from "@/state/store";
import { usePromise } from "@/lib/utils";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useResizable } from "@/lib/hooks/useResizable";
import track_event from "@/tracking";
import {
  saveScrollPosition,
  restoreScrollPosition,
  getScrollPosition,
} from "@/utils/scroll-position";
import { insertDropdown } from "./blocks/Dropdown/Dropdown";
import { insertPause } from "./blocks/Pause";
import { insertSubRunbook } from "./blocks/SubRunbook";
import { insertTerminal } from "@/lib/blocks/terminal";
import { insertKubernetes } from "@/lib/blocks/kubernetes";
import { insertLocalDirectory } from "@/lib/blocks/localdirectory";
import { calculateAIPopupPosition, calculateLinkPopupPosition } from "./utils/popupPositioning";
import { useAIKeyboardShortcuts } from "./hooks/useAIKeyboardShortcuts";
import { useAIInlineGeneration } from "./hooks/useAIInlineGeneration";
import { useTauriEvent } from "@/lib/tauri";
import { save } from "@tauri-apps/plugin-dialog";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { SaveBlockItem } from "./ui/SaveBlockItem";
import { SavedBlockPopup } from "./ui/SavedBlockPopup";
import { DeleteBlockItem } from "./ui/DeleteBlockItem";
import { BlockNoteEditor } from "@blocknote/core";
import useDocumentBridge from "@/lib/hooks/useDocumentBridge";
import { ChargeTarget } from "@/rs-bindings/ChargeTarget";

// Fix for react-dnd interference with BlockNote drag-and-drop
// React-dnd wraps dataTransfer in a proxy that blocks access during drag operations
// We capture the original data during dragstart and resynthesize clean drop events
let originalDragData: any = null;

const insertDirectory = (editor: typeof schema.BlockNoteEditor) => ({
  title: "Directory",
  subtext: "Set current working directory (synced)",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "directory" });

    editor.insertBlocks(
      [
        {
          type: "directory",
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <FolderOpenIcon size={18} />,
  aliases: ["directory", "dir", "folder"],
  group: "Execute",
});

const insertEnv = (editor: typeof schema.BlockNoteEditor) => ({
  title: "Environment Variable",
  subtext: "Set environment variable for all subsequent code blocks",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "env" });

    editor.insertBlocks(
      [
        {
          type: "env",
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <VariableIcon size={18} />,
  aliases: ["env", "environment", "variable"],
  group: "Execute",
});

const insertVar = (editor: typeof schema.BlockNoteEditor) => ({
  title: "Template Variable",
  subtext: "Set template variable for use in subsequent blocks",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "var" });

    editor.insertBlocks(
      [
        {
          type: "var",
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <TextCursorInputIcon size={18} />,
  aliases: ["var", "template", "variable"],
  group: "Execute",
});

const insertVarDisplay = (editor: typeof schema.BlockNoteEditor) => ({
  title: "Display Variable",
  subtext: "Show the current value of a template variable",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "var_display" });

    editor.insertBlocks(
      [
        {
          type: "var_display",
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <EyeIcon size={18} />,
  aliases: ["show", "display", "view", "variable"],
  group: "Execute",
});

const insertRunbookLink = (
  editor: typeof schema.BlockNoteEditor,
  showRunbookLinkPopup: (position: { x: number; y: number }) => void,
) => ({
  title: "Runbook Link",
  subtext: "Link to another runbook",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "runbook_link" });

    // Show the runbook link popup
    const position = calculateLinkPopupPosition(editor);
    showRunbookLinkPopup(position);
  },
  icon: <LinkIcon size={18} />,
  aliases: ["link", "runbook", "reference"],
  group: "Content",
});

const insertSavedBlock = (
  editor: typeof schema.BlockNoteEditor,
  showSavedBlockPopup: (position: { x: number; y: number }) => void,
) => ({
  title: "Saved Block",
  subtext: "Insert a saved block",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "saved_block" });

    const position = calculateLinkPopupPosition(editor);
    showSavedBlockPopup(position);
  },
  icon: <BlocksIcon size={18} />,
  aliases: ["saved", "block"],
  group: "Content",
});

const insertHorizontalRule = (editor: typeof schema.BlockNoteEditor) => ({
  title: "Horizontal Rule",
  subtext: "Insert a horizontal divider line",
  onItemClick: () => {
    track_event("runbooks.block.create", { type: "horizontal_rule" });

    editor.insertBlocks(
      [
        {
          type: "horizontal_rule",
        },
      ],
      editor.getTextCursorPosition().block.id,
      "before",
    );
  },
  icon: <MinusIcon size={18} />,
  aliases: ["hr", "horizontal", "rule", "divider", "separator", "line"],
  group: "Content",
});

const insertPastedBlock = (editor: typeof schema.BlockNoteEditor, copiedBlock: any) => ({
  title: "Paste Block",
  subtext: "Paste the previously copied block",
  onItemClick: () => {
    track_event("runbooks.block.paste", { type: copiedBlock.type });

    editor.insertBlocks([copiedBlock], editor.getTextCursorPosition().block.id, "before");
  },
  icon: <ClipboardPasteIcon size={18} />,
  aliases: ["paste", "insert"],
  group: "Content",
});

// AI Generate function
const insertAIGenerate = (
  editor: any,
  showAIPopup: (position: { x: number; y: number }, blockId: string) => void,
) => ({
  title: "AI Generate",
  subtext: "Generate blocks from a natural language prompt (or press ⌘K)",
  onItemClick: () => {
    track_event("runbooks.ai.slash_menu_popup");
    const cursorPosition = editor.getTextCursorPosition();
    const position = calculateAIPopupPosition(editor);
    showAIPopup(position, cursorPosition.block.id);
  },
  icon: <SparklesIcon size={18} />,
  aliases: ["ai", "generate", "prompt"],
  group: "AI",
});

type EditorProps = {
  runbook: Runbook | null;
  editable: boolean;
  runbookEditor: RunbookEditor;
  isAIAssistantOpen: boolean;
  closeAIAssistant: () => void;
  owningOrgId: string | null;
};

export default function Editor({
  runbook,
  owningOrgId,
  editable,
  runbookEditor,
  isAIAssistantOpen,
  closeAIAssistant,
}: EditorProps) {
  const [editor, editorError] = usePromise<BlockNoteEditor, Error>(runbookEditor.getEditor());
  const colorMode = useStore((state) => state.functionalColorMode);
  const fontSize = useStore((state) => state.fontSize);
  const fontFamily = useStore((state) => state.fontFamily);
  const copiedBlock = useStore((state) => state.copiedBlock);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [aiPopupVisible, setAiPopupVisible] = useState(false);
  const [aiPopupPosition, setAiPopupPosition] = useState({ x: 0, y: 0 });
  const [aiPopupBlockId, setAiPopupBlockId] = useState<string | null>(null);
  const [isVisible, setIsVisible] = useState(true);
  const [runbookLinkPopupVisible, setRunbookLinkPopupVisible] = useState(false);
  const [runbookLinkPopupPosition, setRunbookLinkPopupPosition] = useState({ x: 0, y: 0 });
  const [savedBlockPopupVisible, setSavedBlockPopupVisible] = useState(false);
  const [savedBlockPopupPosition, setSavedBlockPopupPosition] = useState({ x: 0, y: 0 });

  const chargeTarget: ChargeTarget = useMemo(() => {
    if (owningOrgId) {
      return {
        org: owningOrgId,
      };
    }
    return "user";
  }, [owningOrgId]);

  // AI is enabled for logged-in Hub users when AI setting is on
  const isLoggedIn = useStore((state) => state.isLoggedIn);
  const aiEnabled = useStore((state) => state.aiEnabled);
  const aiShareContext = useStore((state) => state.aiShareContext);
  const user = useStore((state) => state.user);
  const username = user?.username ?? "";
  const showAiHint = aiEnabled;
  const aiEnabledState = isLoggedIn() && aiEnabled;

  // AI panel width for resizable panel
  const aiPanelWidth = useStore((state) => state.aiPanelWidth);
  const setAiPanelWidth = useStore((state) => state.setAiPanelWidth);

  const { onResizeStart: handleAiPanelResizeStart } = useResizable({
    width: aiPanelWidth,
    onWidthChange: setAiPanelWidth,
    minWidth: 300,
    maxWidth: 600,
    edge: "left",
  });

  const documentBridge = useDocumentBridge();

  const showAIPopup = useCallback((position: { x: number; y: number }, blockId: string) => {
    setAiPopupPosition(position);
    setAiPopupBlockId(blockId);
    setAiPopupVisible(true);
  }, []);

  const closeAIPopup = useCallback(() => {
    setAiPopupVisible(false);
    setAiPopupBlockId(null);
  }, []);

  const showRunbookLinkPopup = useCallback((position: { x: number; y: number }) => {
    setRunbookLinkPopupPosition(position);
    setRunbookLinkPopupVisible(true);
  }, []);

  const showSavedBlockPopup = useCallback((position: { x: number; y: number }) => {
    setSavedBlockPopupPosition(position);
    setSavedBlockPopupVisible(true);
  }, []);

  const closeRunbookLinkPopup = useCallback(() => {
    setRunbookLinkPopupVisible(false);
  }, []);

  const closeSavedBlockPopup = useCallback(() => {
    setSavedBlockPopupVisible(false);
  }, []);

  const handleExportMarkdown = async () => {
    let editor = await runbookEditor.getEditor();

    try {
      const markdown = await editor?.blocksToMarkdownLossy();
      const filePath = await save({
        defaultPath: `${runbook?.name}.md`,
        filters: [
          {
            name: "Markdown",
            extensions: ["md"],
          },
        ],
      });

      if (!filePath) return;

      await writeTextFile(filePath, markdown || "");

      track_event("runbooks.export.markdown", { runbookId: runbook?.id || "" });
    } catch (error) {
      console.error("Failed to export markdown:", error);
    }
  };

  // Listen for export-markdown menu event
  useTauriEvent("export-markdown", () => handleExportMarkdown());

  const handleRunbookLinkSelect = useCallback(
    (runbookId: string, runbookName: string) => {
      if (!editor) return;

      editor.insertInlineContent([
        {
          type: "runbook-link",
          props: {
            runbookId,
            runbookName,
          },
        } as any,
        " ", // add a space after the link
      ]);

      closeRunbookLinkPopup();

      // Focus back to the editor and position cursor after the inserted link
      setTimeout(() => {
        editor.focus();
      }, 10);
    },
    [editor, closeRunbookLinkPopup],
  );

  const handleSavedBlockSelect = useCallback(
    (_savedBlockId: string, block: any) => {
      if (!editor) return;

      editor.insertBlocks([block], editor.getTextCursorPosition().block.id, "after");

      closeSavedBlockPopup();

      // Focus back to the editor and position cursor after the inserted link
      setTimeout(() => {
        editor.focus();
      }, 10);
    },
    [editor, closeSavedBlockPopup],
  );

  // Get editor context for AI operations (document as markdown + current position)
  const getEditorContext = useCallback(async () => {
    if (!editor) return undefined;

    try {
      const cursorPosition = editor.getTextCursorPosition();
      const blocks = editor.document;
      const currentBlockId = cursorPosition.block.id;
      const currentBlockIndex = blocks.findIndex((b: any) => b.id === currentBlockId);

      // Export document as markdown to save tokens (only if sharing context is enabled)
      const documentMarkdown = aiShareContext ? await editor.blocksToMarkdownLossy() : undefined;

      return {
        documentMarkdown,
        currentBlockId,
        currentBlockIndex: currentBlockIndex >= 0 ? currentBlockIndex : 0,
        runbookId: runbook?.id,
      };
    } catch (error) {
      console.warn("Failed to get editor context:", error);
      return undefined;
    }
  }, [editor, aiShareContext, runbook?.id]);

  // Get context for AI Assistant channel
  const getAIAssistantContext = useCallback(async (): Promise<AIContext> => {
    const lastBlockContext = await documentBridge?.getLastBlockContext();

    // Get named blocks (blocks with names for reference)
    const namedBlocks: [string, string][] = [];
    if (editor) {
      for (const block of editor.document) {
        const props = (block as any).props;
        if (props?.name && typeof props.name === "string" && props.name.trim()) {
          namedBlocks.push([props.name, block.type]);
        }
      }
    }

    return {
      variables: Object.keys(lastBlockContext?.variables ?? {}),
      named_blocks: namedBlocks,
      working_directory: lastBlockContext?.cwd || null,
      environment_variables: Object.keys(lastBlockContext?.envVars ?? {}),
      ssh_host: lastBlockContext?.sshHost || null,
    };
  }, [editor, documentBridge]);

  // AI inline generation hook (handles Cmd+Enter and post-generation shortcuts)
  const {
    isGenerating,
    generatingBlockIds,
    generatedBlockIds,
    isEditing,
    editPrompt,
    loadingStatus,
    clearPostGenerationMode,
    handleEditSubmit,
    cancelEditing,
    setEditPrompt,
    startGenerationWithPrompt,
    getIsProgrammaticEdit,
    hasGeneratedBlocks,
    handleKeyDown: handleInlineGenerationKeyDown,
  } = useAIInlineGeneration({
    editor: editor ?? null,
    runbookId: runbook?.id,
    documentBridge,
    getEditorContext,
    username,
    chargeTarget,
  });

  // Handle popup submit - delegates to inline generation hook
  const handlePopupSubmit = useCallback(
    (prompt: string) => {
      if (!aiPopupBlockId) return;
      // Close popup immediately, generation UI is handled by the hook
      closeAIPopup();
      // Start generation with replacePromptBlock=true to replace the empty block
      startGenerationWithPrompt(prompt, aiPopupBlockId, true);
    },
    [aiPopupBlockId, closeAIPopup, startGenerationWithPrompt],
  );

  // AI keyboard shortcuts (Cmd+K only - for showing popup)
  const { handleKeyDown: handleAIShortcutsKeyDown } = useAIKeyboardShortcuts({
    editor,
    onShowAIPopup: showAIPopup,
  });

  // Combined keyboard handler for BlockNoteView
  const handleEditorKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      handleInlineGenerationKeyDown(e);
      handleAIShortcutsKeyDown(e);
    },
    [handleInlineGenerationKeyDown, handleAIShortcutsKeyDown]
  );

  // Handle visibility and scroll restoration when runbook changes
  useLayoutEffect(() => {
    if (!runbook?.id) return;

    const savedPosition = getScrollPosition(runbook.id);
    if (savedPosition > 0) {
      // Hide temporarily while we restore position
      setIsVisible(false);

      requestAnimationFrame(() => {
        try {
          if (scrollContainerRef.current) {
            restoreScrollPosition(scrollContainerRef.current, runbook.id);
          }
        } catch (error) {
          console.warn("Failed to restore scroll position:", error);
        } finally {
          // Always restore visibility regardless of scroll restoration success
          setIsVisible(true);
        }
      });
    } else {
      // Ensure visibility is set when no scroll position to restore
      setIsVisible(true);
    }
  }, [runbook?.id]);

  // Debounced scroll handler
  const timeoutRef = useRef<number | null>(null);
  const handleScroll = useCallback(
    (e: React.UIEvent<HTMLDivElement>) => {
      if (!runbook?.id) return;

      const target = e.currentTarget;

      // Debounce to avoid excessive localStorage writes
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      timeoutRef.current = window.setTimeout(() => {
        saveScrollPosition(runbook.id, target.scrollTop);
      }, 100);
    },
    [runbook?.id],
  );

  const [unsupportedBlocks, setUnsupportedBlocks] = useState<string[]>([]);
  useEffect(() => {
    if (!runbookEditor) return;

    return runbookEditor.onUnsupportedBlock((unknownTypes: string[]) => {
      console.log(">> unsupported block", unknownTypes);
      setUnsupportedBlocks(unknownTypes);
    });
  }, [runbookEditor]);

  if (editorError) {
    return (
      <div className="flex w-full h-full flex-col justify-center items-center gap-4">
        <AlertCircleIcon className="text-danger size-10" />
        <p className="text-danger max-w-lg text-center">{editorError.message}</p>
      </div>
    );
  }

  if (unsupportedBlocks.length > 0) {
    return (
      <div className="flex w-full h-full flex-col justify-center items-center gap-4">
        <AlertCircleIcon className="text-danger size-10" />
        <p className="text-danger max-w-lg text-center">
          This document contains blocks that your version of Atuin Desktop does not support.
        </p>
      </div>
    );
  }

  if (!editor || !runbook) {
    return (
      <div className="flex w-full h-full flex-col justify-center items-center">
        <Spinner />
      </div>
    );
  }

  // Renders the editor instance.
  return (
    <div className="flex h-full w-full min-h-0">
      {/* Main editor area */}
      <div className="flex-1 min-w-0 h-full">
        <div
          ref={scrollContainerRef}
          className="overflow-y-scroll editor h-full min-h-0 pt-3 relative"
          style={{
            fontSize: `${fontSize}px`,
            fontFamily: fontFamily,
            visibility: isVisible ? "visible" : "hidden",
          }}
          onScroll={handleScroll}
          onDragStart={(e) => {
            // Don't interfere with AG-Grid drag operations
            if (
              (e.target as Element).closest(".ag-theme-alpine, .ag-theme-alpine-dark, .ag-grid")
            ) {
              return;
            }

            // Capture original drag data before react-dnd can wrap it
            originalDragData = {
              effectAllowed: e.dataTransfer.effectAllowed,
              types: Array.from(e.dataTransfer.types),
              data: {},
            };

            e.dataTransfer.types.forEach((type) => {
              try {
                originalDragData.data[type] = e.dataTransfer.getData(type);
              } catch (err) {
                // Some types may not be readable during dragstart
              }
            });
          }}
          onDrop={(e) => {
            // Don't interfere with AG-Grid drop operations
            if (
              (e.target as Element).closest(".ag-theme-alpine, .ag-theme-alpine-dark, .ag-grid")
            ) {
              return;
            }

            if (!originalDragData) {
              return;
            }

            // This is only the case if the user is dragging a block from the sidebar
            if ((e.target as Element).matches(".bn-editor")) {
              return;
            }

            const view = editor._tiptapEditor.view;

            if (!view || !originalDragData.data["blocknote/html"]) {
              return;
            }

            e.preventDefault();
            e.stopPropagation();

            // Create clean DataTransfer with preserved data
            const cleanDataTransfer = new DataTransfer();
            Object.keys(originalDragData.data).forEach((type) => {
              cleanDataTransfer.setData(type, originalDragData.data[type]);
            });

            // Create fresh drop event with clean DataTransfer
            const syntheticEvent = new DragEvent("drop", {
              bubbles: false,
              cancelable: true,
              clientX: e.clientX,
              clientY: e.clientY,
              dataTransfer: cleanDataTransfer,
            });

            // Mark as synthetic to prevent recursion
            (syntheticEvent as any).synthetic = true;

            view.dispatchEvent(syntheticEvent);

            originalDragData = null;
          }}
          onDragOver={(e) => {
            // Don't interfere with AG-Grid drag operations
            if (
              (e.target as Element).closest(".ag-theme-alpine, .ag-theme-alpine-dark, .ag-grid")
            ) {
              return;
            }
            e.preventDefault();
          }}
          onClick={(e) => {
            // Clear post-generation mode on any click
            // Use hasGeneratedBlocks() to avoid stale closure issues
            if (hasGeneratedBlocks()) {
              clearPostGenerationMode();
            }

            // Don't interfere with AG-Grid clicks
            if (
              (e.target as Element).closest(".ag-theme-alpine, .ag-theme-alpine-dark, .ag-grid")
            ) {
              return;
            }

            // Only return if clicking inside editor content, not modals/inputs
            if (
              (e.target as Element).matches(".editor .bn-container *") ||
              (e.target as HTMLElement).tagName === "INPUT"
            )
              return;
            // If the user clicks below the document, focus on the last block
            // But if the last block is not an empty paragraph, create it :D
            let blocks = editor.document;
            let lastBlock = blocks[blocks.length - 1];
            let id = lastBlock.id;
            if (lastBlock.type !== "paragraph" || lastBlock.content.length > 0) {
              id = uuidv7();
              editor.insertBlocks(
                [
                  {
                    id,
                    type: "paragraph",
                    content: "",
                  },
                ],
                lastBlock.id,
                "after",
              );
            }
            editor.focus();
            editor.setTextCursorPosition(id, "start");
          }}
        >
          <BlockNoteView
            editor={editor}
            slashMenu={false}
            className="pb-[200px]"
            sideMenu={false}
            onKeyDownCapture={handleEditorKeyDown}
            onChange={() => {
              runbookEditor.save(runbook, editor);
              // Clear post-generation mode when user edits anything (but not programmatic edits)
              // Use hasGeneratedBlocks() to avoid stale closure issues
              if (hasGeneratedBlocks() && !getIsProgrammaticEdit()) {
                clearPostGenerationMode();
              }
            }}
            theme={colorMode === "dark" ? "dark" : "light"}
            editable={editable}
          >
            <SuggestionMenuController
              triggerCharacter={"/"}
              getItems={async (query: any) =>
                filterSuggestionItems(
                  [
                    // Execute group
                    insertTerminal(editor as any),
                    insertKubernetes(editor as any),
                    insertEnv(editor as any),
                    insertVar(editor as any),
                    insertVarDisplay(editor as any),
                    insertLocalVar(schema)(editor),
                    insertScript(schema)(editor),
                    insertDirectory(editor as any),
                    insertLocalDirectory(editor as any),
                    insertDropdown(schema)(editor),
                    insertPause(schema)(editor),
                    insertSubRunbook(editor as any),

                    // Content group
                    insertMarkdownRender(editor as any),
                    insertTableOfContents(schema)(editor),
                    insertRunbookLink(editor as any, showRunbookLinkPopup),
                    insertSavedBlock(editor as any, showSavedBlockPopup),
                    insertHorizontalRule(editor as any),
                    ...(copiedBlock.isSome()
                      ? [insertPastedBlock(editor as any, copiedBlock.unwrap())]
                      : []),

                    // Monitoring group
                    insertPrometheus(schema)(editor),

                    // Database group
                    insertSQLite(schema)(editor),
                    insertPostgres(schema)(editor),
                    insertMySQL(schema)(editor),
                    insertClickhouse(schema)(editor),

                    // Network group
                    insertHttp(schema)(editor),
                    insertSshConnect(schema)(editor),
                    insertHostSelect(schema)(editor),

                    // Misc group
                    insertEditor(schema)(editor),

                    ...getDefaultReactSlashMenuItems(editor),
                    // AI group (only if enabled)
                    ...(aiEnabledState ? [insertAIGenerate(editor, showAIPopup)] : []),
                  ],
                  query,
                )
              }
            />

            <SideMenuController
              sideMenu={() => (
                <SideMenu>
                  <AddBlockButton />
                  <DragHandleButton>
                    <DeleteBlockItem />
                    <DuplicateBlockItem />
                    <CopyBlockItem />
                    <SaveBlockItem />
                  </DragHandleButton>
                </SideMenu>
              )}
            />
          </BlockNoteView>

          {/* AI popup positioned relative to editor container (only if AI is enabled) */}
          {aiEnabledState && (
            <AIGeneratePopup
              isVisible={aiPopupVisible}
              position={aiPopupPosition}
              onSubmit={handlePopupSubmit}
              onClose={closeAIPopup}
            />
          )}

          {/* Subtle hint for AI generation */}
          {aiEnabledState && showAiHint && generatedBlockIds.length === 0 && (
            <AIHint editor={editor} isGenerating={isGenerating} aiEnabled={aiEnabledState} />
          )}

          {/* Inline generation loading overlay */}
          {isGenerating && generatingBlockIds && (
            <AILoadingOverlay blockIds={generatingBlockIds} editor={editor} status={loadingStatus} />
          )}

          {/* Post-generation focus overlay - shows after AI generates blocks */}
          {generatedBlockIds.length > 0 && (
            <AIFocusOverlay
              hideAllHints={isGenerating}
              showRunHint={generatedBlockIds.length === 1}
              blockIds={generatedBlockIds}
              editor={editor}
              isEditing={isEditing}
              editValue={editPrompt}
              onEditChange={setEditPrompt}
              onEditSubmit={handleEditSubmit}
              onEditCancel={cancelEditing}
            />
          )}

          {/* Runbook link popup */}
          <RunbookLinkPopup
            isVisible={runbookLinkPopupVisible}
            position={runbookLinkPopupPosition}
            onSelect={handleRunbookLinkSelect}
            onClose={closeRunbookLinkPopup}
          />
          <SavedBlockPopup
            isVisible={savedBlockPopupVisible}
            position={savedBlockPopupPosition}
            onSelect={handleSavedBlockSelect}
            onClose={closeSavedBlockPopup}
          />

          {/* AI Assistant toggle button */}
        </div>
      </div>

      {/* AI Assistant sidebar */}
      {aiEnabledState && isAIAssistantOpen && (
        <div className="relative h-full flex-shrink-0" style={{ width: aiPanelWidth }}>
          {/* Resize handle */}
          <div
            onMouseDown={handleAiPanelResizeStart}
            className="absolute top-0 left-0 w-2 h-full cursor-col-resize group z-10"
          >
            <div className="absolute top-0 left-0 w-0.5 h-full bg-transparent group-hover:bg-gray-300 dark:group-hover:bg-gray-600 transition-colors duration-150" />
          </div>
          <div className="h-full border-l border-default-200 dark:border-default-100">
            <AIAssistant
              runbookId={runbook.id}
              editor={editor}
              getContext={getAIAssistantContext}
              isOpen={isAIAssistantOpen}
              chargeTarget={chargeTarget}
              onClose={closeAIAssistant}
            />
          </div>
        </div>
      )}
    </div>
  );
}
