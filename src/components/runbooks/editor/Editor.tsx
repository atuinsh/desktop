import "./index.css";

import { Spinner, addToast } from "@heroui/react";

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
  ClipboardPasteIcon,
} from "lucide-react";

import { AIGeneratePopup } from "./AIGeneratePopup";
import { AIFeatureDisabledError } from "@/lib/ai/block_generator";
import AIPopup from "./ui/AIPopup";
import { AILoadingOverlay } from "./ui/AILoadingBlock";
import { AIFocusOverlay } from "./ui/AIFocusOverlay";
import { RunbookLinkPopup } from "./ui/RunbookLinkPopup";
import { executeBlock } from "@/lib/runtime";
import { onBlockFinished } from "@/lib/events/grand_central";
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

import Runbook from "@/state/runbooks/runbook";
import { insertHttp } from "@/lib/blocks/http";
import { uuidv7 } from "uuidv7";
import { DuplicateBlockItem } from "./ui/DuplicateBlockItem";
import { CopyBlockItem } from "./ui/CopyBlockItem";

import { schema } from "./create_editor";
import RunbookEditor from "@/lib/runbook_editor";
import { useStore } from "@/state/store";
import { usePromise } from "@/lib/utils";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
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
  const copiedBlock = useStore((state) => state.copiedBlock);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [aiPopupVisible, setAiPopupVisible] = useState(false);
  const [aiPopupPosition, setAiPopupPosition] = useState({ x: 0, y: 0 });
  const [isAIEditPopupOpen, setIsAIEditPopupOpen] = useState(false);
  const [currentEditBlock, setCurrentEditBlock] = useState<any>(null);
  const [aiEditPopupPosition, setAiEditPopupPosition] = useState({ x: 0, y: 0 });
  const [isVisible, setIsVisible] = useState(true);
  const [runbookLinkPopupVisible, setRunbookLinkPopupVisible] = useState(false);
  const [runbookLinkPopupPosition, setRunbookLinkPopupPosition] = useState({ x: 0, y: 0 });
  const [savedBlockPopupVisible, setSavedBlockPopupVisible] = useState(false);
  const [savedBlockPopupPosition, setSavedBlockPopupPosition] = useState({ x: 0, y: 0 });

  // Inline AI generation state
  const [isGeneratingInline, setIsGeneratingInline] = useState(false);
  const [generatingBlockId, setGeneratingBlockId] = useState<string | null>(null);
  const [loadingStatus, setLoadingStatus] = useState<"loading" | "cancelled">("loading");
  const generatingBlockIdRef = useRef<string | null>(null); // For cancellation check
  const originalPromptRef = useRef<string | null>(null); // For cancellation detection

  // Post-generation mode state - after AI generates a block, user can Cmd+Enter to run or Tab to continue
  const [postGenerationBlockId, setPostGenerationBlockId] = useState<string | null>(null);
  const [generatedBlockIds, setGeneratedBlockIds] = useState<string[]>([]);
  const [generatedBlockCount, setGeneratedBlockCount] = useState(0);

  // Edit mode state for follow-up adjustments
  const [isEditingGenerated, setIsEditingGenerated] = useState(false);
  const [editPrompt, setEditPrompt] = useState("");
  const isProgrammaticEditRef = useRef(false);

  // AI is enabled for logged-in Hub users
  const isLoggedIn = useStore((state) => state.isLoggedIn);
  const aiEnabledState = isLoggedIn();

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

  // Get editor context for AI operations (document as markdown + current position)
  const getEditorContext = useCallback(async () => {
    if (!editor) return undefined;

    try {
      const cursorPosition = editor.getTextCursorPosition();
      const blocks = editor.document;
      const currentBlockId = cursorPosition.block.id;
      const currentBlockIndex = blocks.findIndex((b: any) => b.id === currentBlockId);

      // Export document as markdown to save tokens
      const documentMarkdown = await editor.blocksToMarkdownLossy();

      return {
        documentMarkdown,
        currentBlockId,
        currentBlockIndex: currentBlockIndex >= 0 ? currentBlockIndex : 0,
      };
    } catch (error) {
      console.warn("Failed to get editor context:", error);
      return undefined;
    }
  }, [editor]);

  // Extract plain text from a BlockNote block's content
  const getBlockText = useCallback((block: any): string => {
    if (!block.content || !Array.isArray(block.content)) return "";
    return block.content
      .filter((item: any) => item.type === "text")
      .map((item: any) => item.text || "")
      .join("");
  }, []);

  // Handle inline AI generation from a paragraph block
  const handleInlineGenerate = useCallback(
    async (block: any) => {
      const prompt = getBlockText(block);
      if (!prompt.trim() || !editor) return;

      // Set up generation state
      setIsGeneratingInline(true);
      setGeneratingBlockId(block.id);
      setLoadingStatus("loading");
      generatingBlockIdRef.current = block.id;
      originalPromptRef.current = prompt;

      try {
        const context = await getEditorContext();
        const { generateBlocks } = await import("@/lib/ai/block_generator");

        const result = await generateBlocks({
          prompt,
          documentMarkdown: context?.documentMarkdown,
          insertAfterIndex: context?.currentBlockIndex,
        });

        // Check if the block was edited during generation (cancellation)
        const currentBlock = editor.document.find((b: any) => b.id === block.id);
        const currentBlockText = getBlockText(currentBlock);
        if (currentBlockText !== prompt) {
          // Block was edited, show cancelled state briefly then hide
          setLoadingStatus("cancelled");
          track_event("runbooks.ai.inline_generate_cancelled", {
            reason: "block_edited",
          });
          // Show "Cancelled" for 1.5 seconds
          await new Promise((resolve) => setTimeout(resolve, 1500));
          return;
        }

        // Cap at 3 blocks
        const blocksToInsert = result.blocks.slice(0, 3);

        let lastInsertedId = block.id;
        const insertedIds: string[] = [];
        for (const newBlock of blocksToInsert) {
          const inserted = editor.insertBlocks([newBlock as any], lastInsertedId, "after");
          if (inserted?.[0]?.id) {
            lastInsertedId = inserted[0].id;
            insertedIds.push(inserted[0].id);
          }
        }

        // Move cursor to after the last inserted block
        if (lastInsertedId !== block.id) {
          editor.setTextCursorPosition(lastInsertedId, "end");
        }

        // Enter post-generation mode - user can Cmd+Enter to run or Tab to continue
        if (blocksToInsert.length > 0) {
          setPostGenerationBlockId(lastInsertedId);
          setGeneratedBlockIds(insertedIds);
          setGeneratedBlockCount(blocksToInsert.length);
        }

        track_event("runbooks.ai.inline_generate_success", {
          prompt_length: prompt.length,
          blocks_generated: blocksToInsert.length,
        });
      } catch (error) {
        const message =
          error instanceof AIFeatureDisabledError
            ? "AI feature is not enabled for your account"
            : error instanceof Error
              ? error.message
              : "Failed to generate blocks";

        addToast({
          title: "Generation failed",
          description: message,
          color: "danger",
        });

        track_event("runbooks.ai.inline_generate_error", {
          error: message,
        });
      } finally {
        setIsGeneratingInline(false);
        setGeneratingBlockId(null);
        generatingBlockIdRef.current = null;
        originalPromptRef.current = null;
      }
    },
    [editor, getEditorContext, getBlockText]
  );

  // Block types that have inline text content (can be used as prompts)
  const textBlockTypes = [
    "paragraph",
    "heading",
    "bulletListItem",
    "numberedListItem",
    "checkListItem",
  ];

  // Block types that can be executed
  const executableBlockTypes = [
    "run",
    "script",
    "postgres",
    "sqlite",
    "mysql",
    "clickhouse",
    "http",
    "prometheus",
    "kubernetes-get",
  ];

  // Clear post-generation mode
  const clearPostGenerationMode = useCallback(() => {
    setPostGenerationBlockId(null);
    setGeneratedBlockIds([]);
    setGeneratedBlockCount(0);
    setIsEditingGenerated(false);
    setEditPrompt("");
  }, []);

  // Handle edit submission for follow-up adjustments
  const handleEditSubmit = useCallback(async () => {
    if (!editor || !postGenerationBlockId || !editPrompt.trim() || generatedBlockIds.length === 0) return;

    const blockToEditId = generatedBlockIds[0];

    // Use the same loading overlay as initial generation
    setIsEditingGenerated(false);
    setIsGeneratingInline(true);
    setGeneratingBlockId(blockToEditId);
    setLoadingStatus("loading");

    try {
      // Get the current block to edit
      const currentBlock = editor.document.find((b: any) => b.id === blockToEditId);
      if (!currentBlock) {
        throw new Error("Block not found");
      }

      const context = await getEditorContext();
      const { generateOrEditBlock } = await import("@/api/ai");

      const result = await generateOrEditBlock({
        action: "edit",
        block: currentBlock,
        instruction: editPrompt,
        document_markdown: context?.documentMarkdown,
      });

      // Replace the block with the edited version
      const newBlock = { ...result.block, id: currentBlock.id };
      isProgrammaticEditRef.current = true;
      editor.updateBlock(currentBlock.id, newBlock as any);
      isProgrammaticEditRef.current = false;

      // Reset edit prompt but stay in post-generation mode
      setEditPrompt("");

      track_event("runbooks.ai.post_generation_edit", {
        blockType: currentBlock.type,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : "Failed to edit block";
      addToast({
        title: "Edit failed",
        description: message,
        color: "danger",
      });
      track_event("runbooks.ai.post_generation_edit_error", { error: message });
    } finally {
      // Clear loading state - this will show the focus overlay again
      setIsGeneratingInline(false);
      setGeneratingBlockId(null);
    }
  }, [editor, postGenerationBlockId, editPrompt, generatedBlockIds, getEditorContext]);

  // Add keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!editor) return;

      // Handle post-generation mode shortcuts
      if (postGenerationBlockId) {
        // Don't handle shortcuts while editing (except escape which is handled in the input)
        if (isEditingGenerated) {
          return;
        }

        // Escape - dismiss and delete generated blocks
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          // Delete all generated blocks
          if (generatedBlockIds.length > 0) {
            editor.removeBlocks(generatedBlockIds);
          }
          clearPostGenerationMode();
          track_event("runbooks.ai.post_generation_dismiss");
          return;
        }

        // E - enter edit mode for follow-up adjustments
        if (e.key === "e" || e.key === "E") {
          e.preventDefault();
          e.stopPropagation();
          setIsEditingGenerated(true);
          track_event("runbooks.ai.post_generation_edit_start");
          return;
        }

        // Tab - insert paragraph after generated block and continue writing
        if (e.key === "Tab" && !e.metaKey && !e.ctrlKey && !e.altKey) {
          e.preventDefault();
          e.stopPropagation();
          const newParagraph = editor.insertBlocks(
            [{ type: "paragraph", content: "" }],
            postGenerationBlockId,
            "after"
          );
          if (newParagraph?.[0]?.id) {
            editor.setTextCursorPosition(newParagraph[0].id, "start");
          }
          clearPostGenerationMode();
          track_event("runbooks.ai.post_generation_continue");
          return;
        }

        // Cmd+Enter - accept and run the generated block
        if (e.metaKey && e.key === "Enter") {
          e.preventDefault();
          e.stopPropagation();

          // Check if multiple blocks were generated
          if (generatedBlockCount > 1) {
            addToast({
              title: "Multiple blocks generated",
              description: "Running multiple blocks in series is not yet supported. Please run them individually.",
              color: "warning",
            });
            clearPostGenerationMode();
            return;
          }

          // Check if the block is executable
          const block = editor.document.find((b: any) => b.id === postGenerationBlockId);
          if (block && executableBlockTypes.includes(block.type)) {
            if (runbook?.id) {
              executeBlock(runbook.id, postGenerationBlockId);
              track_event("runbooks.ai.post_generation_run", { blockType: block.type });
            }
          } else {
            addToast({
              title: "Cannot run this block",
              description: `Block type "${block?.type || "unknown"}" is not executable.`,
              color: "warning",
            });
          }

          // Move cursor after the block and insert a new paragraph
          const newParagraph = editor.insertBlocks(
            [{ type: "paragraph", content: "" }],
            postGenerationBlockId,
            "after"
          );
          if (newParagraph?.[0]?.id) {
            editor.setTextCursorPosition(newParagraph[0].id, "start");
          }

          clearPostGenerationMode();
          return;
        }
      }

      // Cmd+K or Cmd+Enter for AI operations
      if (e.metaKey && (e.key === "k" || e.key === "Enter")) {
        try {
          const cursorPosition = editor.getTextCursorPosition();
          const currentBlock = cursorPosition.block;

          const isTextBlock = textBlockTypes.includes(currentBlock.type);
          const hasContent =
            currentBlock.content &&
            Array.isArray(currentBlock.content) &&
            currentBlock.content.length > 0;

          if (isTextBlock && hasContent) {
            // Text block with content → inline generation (Cmd+K or Cmd+Enter)
            e.preventDefault();
            if (!isGeneratingInline) {
              clearPostGenerationMode(); // Clear any previous post-generation state
              track_event("runbooks.ai.inline_generate_trigger", {
                shortcut: e.key === "k" ? "cmd-k" : "cmd-enter",
                blockType: currentBlock.type,
              });
              handleInlineGenerate(currentBlock);
            }
          } else if (e.key === "k" && isTextBlock && !hasContent) {
            // Empty text block + Cmd+K = show generate popup
            e.preventDefault();
            clearPostGenerationMode();
            track_event("runbooks.ai.keyboard_shortcut");
            const position = calculateAIPopupPosition(editor, currentBlock.id);
            showAIPopup(position);
          } else if (e.key === "k" && !isTextBlock) {
            // Non-text block + Cmd+K = edit popup
            e.preventDefault();
            clearPostGenerationMode();
            track_event("runbooks.ai.edit_block", { blockType: currentBlock.type });
            const position = calculateAIPopupPosition(editor, currentBlock.id);
            setAiEditPopupPosition(position);
            setIsAIEditPopupOpen(true);
            setCurrentEditBlock(currentBlock);
          }
          // Cmd+Enter on empty text block or non-text block = do nothing (let default behavior)
        } catch (error) {
          console.warn("Could not get cursor position:", error);
        }
      }
    };

    // Use capture phase to intercept before BlockNote handles the event
    document.addEventListener("keydown", handleKeyDown, { capture: true });

    return () => {
      document.removeEventListener("keydown", handleKeyDown, { capture: true });
    };
  }, [editor, showAIPopup, isGeneratingInline, handleInlineGenerate, postGenerationBlockId, generatedBlockIds, generatedBlockCount, runbook?.id, clearPostGenerationMode, isEditingGenerated]);

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
        // Clear post-generation mode on any click
        if (postGenerationBlockId) {
          clearPostGenerationMode();
        }

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
          // Clear post-generation mode when user edits anything (but not programmatic edits)
          if (postGenerationBlockId && !isProgrammaticEditRef.current) {
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

                // Content group
                insertMarkdownRender(editor as any),
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
          sideMenu={(props: any) => (
            <SideMenu
              {...props}
              style={{ zIndex: 0 }}
              dragHandleMenu={(props) => (
                <DragHandleMenu {...props}>
                  <DeleteBlockItem {...props} />
                  <DuplicateBlockItem {...props} />
                  <CopyBlockItem {...props} />
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

      {/* Inline generation loading overlay */}
      {isGeneratingInline && generatingBlockId && (
        <AILoadingOverlay blockId={generatingBlockId} editor={editor} status={loadingStatus} />
      )}

      {/* Post-generation focus overlay - shows after AI generates a block */}
      {postGenerationBlockId && !isGeneratingInline && (
        <AIFocusOverlay
          blockId={postGenerationBlockId}
          editor={editor}
          isEditing={isEditingGenerated}
          editValue={editPrompt}
          onEditChange={setEditPrompt}
          onEditSubmit={handleEditSubmit}
          onEditCancel={() => setIsEditingGenerated(false)}
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
