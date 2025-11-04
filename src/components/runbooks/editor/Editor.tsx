import "./index.css";

import { Spinner } from "@heroui/react";

import { filterSuggestionItems } from "@blocknote/core";

import {
  SuggestionMenuController,
  getDefaultReactSlashMenuItems,
  SideMenu,
  SideMenuController,
  DragHandleMenu,
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
} from "lucide-react";

import { AIGeneratePopup } from "./AIGeneratePopup";
import AIPopup from "./ui/AIPopup";
import { RunbookLinkPopup } from "./ui/RunbookLinkPopup";
import { isAIEnabled } from "@/lib/ai/block_generator";
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

import Runbook from "@/state/runbooks/runbook";
import { insertHttp } from "@/lib/blocks/http";
import { uuidv7 } from "uuidv7";
import { DuplicateBlockItem } from "./ui/DuplicateBlockItem";

import { schema } from "./create_editor";
import RunbookEditor from "@/lib/runbook_editor";
import { useStore } from "@/state/store";
import { usePromise } from "@/lib/utils";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import BlockBus from "@/lib/workflow/block_bus";
import { invoke } from "@tauri-apps/api/core";
import { convertBlocknoteToAtuin } from "@/lib/workflow/blocks/convert";
import track_event from "@/tracking";
import {
  saveScrollPosition,
  restoreScrollPosition,
  getScrollPosition,
} from "@/utils/scroll-position";
import { insertDropdown } from "./blocks/Dropdown/Dropdown";
import { insertTerminal } from "@/lib/blocks/terminal";
import { insertKubernetes } from "@/lib/blocks/kubernetes";
import { insertLocalDirectory } from "@/lib/blocks/localdirectory";
import { calculateAIPopupPosition, calculateLinkPopupPosition } from "./utils/popupPositioning";
import { useTauriEvent } from "@/lib/tauri";
import { save } from "@tauri-apps/plugin-dialog";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { SaveBlockItem } from "./ui/SaveBlockItem";
import { SavedBlockPopup } from "./ui/SavedBlockPopup";
import { DeleteBlockItem } from "./ui/DeleteBlockItem";

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

// AI Generate function
const insertAIGenerate = (
  editor: any,
  showAIPopup: (position: { x: number; y: number }) => void,
) => ({
  title: "AI Generate",
  subtext: "Generate blocks from a natural language prompt (or press ⌘K)",
  onItemClick: () => {
    track_event("runbooks.ai.slash_menu_popup");
    const position = calculateAIPopupPosition(editor);
    showAIPopup(position);
  },
  icon: <SparklesIcon size={18} />,
  aliases: ["ai", "generate", "prompt"],
  group: "AI",
});

type EditorProps = {
  runbook: Runbook | null;
  editable: boolean;
  runbookEditor: RunbookEditor;
};

export default function Editor({ runbook, editable, runbookEditor }: EditorProps) {
  const editor = usePromise(runbookEditor.getEditor());
  const colorMode = useStore((state) => state.functionalColorMode);
  const fontSize = useStore((state) => state.fontSize);
  const fontFamily = useStore((state) => state.fontFamily);
  const serialExecuteRef = useRef<(() => void) | null>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [aiPopupVisible, setAiPopupVisible] = useState(false);
  const [aiPopupPosition, setAiPopupPosition] = useState({ x: 0, y: 0 });
  const [aiEnabledState, setAiEnabledState] = useState(false);
  const [isAIEditPopupOpen, setIsAIEditPopupOpen] = useState(false);
  const [currentEditBlock, setCurrentEditBlock] = useState<any>(null);
  const [aiEditPopupPosition, setAiEditPopupPosition] = useState({ x: 0, y: 0 });
  const [isVisible, setIsVisible] = useState(true);
  const [runbookLinkPopupVisible, setRunbookLinkPopupVisible] = useState(false);
  const [runbookLinkPopupPosition, setRunbookLinkPopupPosition] = useState({ x: 0, y: 0 });
  const [savedBlockPopupVisible, setSavedBlockPopupVisible] = useState(false);
  const [savedBlockPopupPosition, setSavedBlockPopupPosition] = useState({ x: 0, y: 0 });

  // Check AI enabled status
  useEffect(() => {
    isAIEnabled().then(setAiEnabledState);
  }, []);

  const showAIPopup = useCallback((position: { x: number; y: number }) => {
    setAiPopupPosition(position);
    setAiPopupVisible(true);
  }, []);

  const closeAIPopup = useCallback(() => {
    setAiPopupVisible(false);
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

  const getEditorContext = useCallback(async () => {
    if (!editor) return undefined;

    try {
      // Get current document blocks
      const blocks = editor.document;

      // Get cursor position (current block)
      const textCursorPosition = editor.getTextCursorPosition();
      const currentBlockId = textCursorPosition.block.id;

      // Find current block index
      const currentBlockIndex = blocks.findIndex((block) => block.id === currentBlockId);

      return {
        blocks,
        currentBlockId,
        currentBlockIndex: currentBlockIndex >= 0 ? currentBlockIndex : 0,
      };
    } catch (error) {
      console.warn("Could not get editor context:", error);
      return undefined;
    }
  }, [editor]);

  const insertionAnchorRef = useRef<string | null>(null);
  const lastInsertedBlockRef = useRef<string | null>(null);

  const handleBlockGenerated = useCallback(
    (block: any) => {
      if (!editor) return;

      // On first block, store the anchor (cursor position) and insert after it
      if (!insertionAnchorRef.current) {
        insertionAnchorRef.current = editor.getTextCursorPosition().block.id;
      }
      
      // Insert after the last inserted block, or after anchor if this is the first
      const insertAfterId = lastInsertedBlockRef.current || insertionAnchorRef.current;
      
      const insertedBlocks = editor.insertBlocks([block], insertAfterId, "after");
      
      // Track the last inserted block for the next one
      if (insertedBlocks && insertedBlocks.length > 0) {
        lastInsertedBlockRef.current = insertedBlocks[0].id;
      }
    },
    [editor],
  );

  const handleGenerateComplete = useCallback(() => {
    insertionAnchorRef.current = null;
    lastInsertedBlockRef.current = null;
    closeAIPopup();
  }, [closeAIPopup]);

  const serialExecuteCallback = useCallback(async () => {
    if (!editor || !runbook) {
      return;
    }

    let workflow = editor.document
      .map(convertBlocknoteToAtuin)
      .filter((block) => block !== null)
      .map((block) => ({ type: block.typeName, ...block.object() }));
    console.log(workflow);
    await invoke("workflow_serial", { id: runbook.id, workflow });
  }, [editor, runbook]);

  useEffect(() => {
    if (!editor || !runbook || serialExecuteRef.current) {
      return;
    }

    serialExecuteRef.current = BlockBus.get().subscribeStartWorkflow(
      runbook.id,
      serialExecuteCallback,
    );

    return () => {
      if (serialExecuteRef.current) {
        BlockBus.get().unsubscribeStartWorkflow(runbook.id, serialExecuteRef.current);
      }
    };
  }, [editor, runbook]);

  // Add keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Cmd+K for AI popup
      if (e.metaKey && e.key === "k") {
        e.preventDefault();

        if (!editor) return;

        try {
          // Get the current cursor position in the editor
          const cursorPosition = editor.getTextCursorPosition();
          const currentBlock = cursorPosition.block;

          // Check if we're in an empty paragraph (for generation) or specific block (for editing)
          const isEmptyParagraph =
            currentBlock.type === "paragraph" &&
            (!currentBlock.content || currentBlock.content.length === 0);

          if (isEmptyParagraph) {
            // Generate new blocks mode
            track_event("runbooks.ai.keyboard_shortcut");
            const position = calculateAIPopupPosition(editor, currentBlock.id);
            showAIPopup(position);
          } else {
            // Edit existing block mode
            track_event("runbooks.ai.edit_block", { blockType: currentBlock.type });
            const position = calculateAIPopupPosition(editor, currentBlock.id);
            setAiEditPopupPosition(position);
            setIsAIEditPopupOpen(true);
            setCurrentEditBlock(currentBlock);
          }
        } catch (error) {
          console.warn("Could not get cursor position for Cmd+K, using fallback:", error);
          // Fallback to center if APIs fail
          showAIPopup({ x: 250, y: 100 });
        }
      }
    };

    document.addEventListener("keydown", handleKeyDown);

    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [editor, showAIPopup, showRunbookLinkPopup]);

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
        console.log("saving scroll position for runbook", runbook.id);
        saveScrollPosition(runbook.id, target.scrollTop);
      }, 100);
    },
    [runbook?.id],
  );

  if (!editor || !runbook) {
    return (
      <div className="flex w-full h-full flex-col justify-center items-center">
        <Spinner />
      </div>
    );
  }

  // Renders the editor instance.
  return (
    <div
      ref={scrollContainerRef}
      className="overflow-y-scroll editor flex-grow pt-3 relative"
      style={{
        fontSize: `${fontSize}px`,
        fontFamily: fontFamily,
        visibility: isVisible ? "visible" : "hidden",
      }}
      onScroll={handleScroll}
      onDragStart={(e) => {
        // Don't interfere with AG-Grid drag operations
        if ((e.target as Element).closest(".ag-theme-alpine, .ag-theme-alpine-dark, .ag-grid")) {
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
        if ((e.target as Element).closest(".ag-theme-alpine, .ag-theme-alpine-dark, .ag-grid")) {
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
        if ((e.target as Element).closest(".ag-theme-alpine, .ag-theme-alpine-dark, .ag-grid")) {
          return;
        }
        e.preventDefault();
      }}
      onClick={(e) => {
        // Don't interfere with AG-Grid clicks
        if ((e.target as Element).closest(".ag-theme-alpine, .ag-theme-alpine-dark, .ag-grid")) {
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
        onChange={() => {
          runbookEditor.save(runbook, editor);
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

                // Content group
                insertRunbookLink(editor as any, showRunbookLinkPopup),
                insertSavedBlock(editor as any, showSavedBlockPopup),
                insertHorizontalRule(editor as any),

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
          sideMenu={(props: any) => (
            <SideMenu
              {...props}
              style={{ zIndex: 0 }}
              dragHandleMenu={(props) => (
                <DragHandleMenu {...props}>
                  <DeleteBlockItem {...props} />
                  <DuplicateBlockItem {...props} />
                  <SaveBlockItem {...props} />
                </DragHandleMenu>
              )}
            ></SideMenu>
          )}
        />
      </BlockNoteView>

      {/* AI popup positioned relative to editor container (only if AI is enabled) */}
      {aiEnabledState && (
        <AIGeneratePopup
          isVisible={aiPopupVisible}
          position={aiPopupPosition}
          onBlockGenerated={handleBlockGenerated}
          onGenerateComplete={handleGenerateComplete}
          onClose={closeAIPopup}
          getEditorContext={getEditorContext}
        />
      )}

      {/* AI edit popup for modifying existing blocks */}
      {aiEnabledState && (
        <AIPopup
          isOpen={isAIEditPopupOpen}
          onClose={() => setIsAIEditPopupOpen(false)}
          editor={editor}
          currentBlock={currentEditBlock}
          position={aiEditPopupPosition}
          getEditorContext={getEditorContext}
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
    </div>
  );
}
