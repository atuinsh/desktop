import "./index.css";

import { Spinner, addToast } from "@heroui/react";

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
import { AIFeatureDisabledError, AIQuotaExceededError } from "@/lib/ai/block_generator";
import AIPopup from "./ui/AIPopup";
import { AILoadingOverlay } from "./ui/AILoadingBlock";
import { AIFocusOverlay } from "./ui/AIFocusOverlay";
import { AIHint, incrementAIHintUseCount } from "./ui/AIHint";
import { RunbookLinkPopup } from "./ui/RunbookLinkPopup";
import AIAssistant, { AIContext } from "./ui/AIAssistant";
import { SparklesIcon } from "lucide-react";
import { ResizablePanelGroup, ResizablePanel, ResizableHandle } from "@/components/ui/resizable";

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
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
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
import { useTauriEvent } from "@/lib/tauri";
import { save } from "@tauri-apps/plugin-dialog";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { SaveBlockItem } from "./ui/SaveBlockItem";
import { SavedBlockPopup } from "./ui/SavedBlockPopup";
import { DeleteBlockItem } from "./ui/DeleteBlockItem";
import { BlockNoteEditor } from "@blocknote/core";
import useDocumentBridge from "@/lib/hooks/useDocumentBridge";
import { AISingleBlockResponse } from "@/api/ai";
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
  const [isAIEditPopupOpen, setIsAIEditPopupOpen] = useState(false);
  const [currentEditBlock, setCurrentEditBlock] = useState<any>(null);
  const [aiEditPopupPosition, setAiEditPopupPosition] = useState({ x: 0, y: 0 });
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

  // Inline AI generation state
  const [isGeneratingInline, setIsGeneratingInline] = useState(false);
  const [generatingBlockId, setGeneratingBlockId] = useState<string | null>(null);
  const [loadingStatus, setLoadingStatus] = useState<"loading" | "cancelled">("loading");
  const generatingBlockIdRef = useRef<string | null>(null); // For cancellation check
  const originalPromptRef = useRef<string | null>(null); // For cancellation detection
  const errorToastShownRef = useRef(false); // Prevent duplicate error toasts

  // Post-generation mode state - after AI generates a block, user can Cmd+Enter to run or Tab to continue
  const [postGenerationBlockId, setPostGenerationBlockId] = useState<string | null>(null);
  const [generatedBlockIds, setGeneratedBlockIds] = useState<string[]>([]);
  const [generatedBlockCount, setGeneratedBlockCount] = useState(0);

  // Edit mode state for follow-up adjustments
  const [isEditingGenerated, setIsEditingGenerated] = useState(false);
  const [editPrompt, setEditPrompt] = useState("");
  const isProgrammaticEditRef = useRef(false);

  // AI is enabled for logged-in Hub users when AI setting is on
  const isLoggedIn = useStore((state) => state.isLoggedIn);
  const aiEnabled = useStore((state) => state.aiEnabled);
  const aiShareContext = useStore((state) => state.aiShareContext);
  const user = useStore((state) => state.user);
  const username = user?.username?.toLowerCase() ?? "";
  const showAiHint = ["ellie", "binarymuse"].includes(username);
  const aiEnabledState = isLoggedIn() && aiEnabled;

  const documentBridge = useDocumentBridge();

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
      errorToastShownRef.current = false;

      try {
        const context = await getEditorContext();
        const { generateBlocks } = await import("@/lib/ai/block_generator");
        const lastBlockContext = await documentBridge?.getLastBlockContext();
        console.log("lastBlockContext", lastBlockContext);

        const result = await generateBlocks({
          prompt,
          documentMarkdown: context?.documentMarkdown,
          insertAfterIndex: context?.currentBlockIndex,
          runbookId: context?.runbookId,
          context: {
            variables: Object.keys(lastBlockContext?.variables ?? {}),
            named_blocks: [], // TODO: Implement named blocks
            environment_variables: Object.keys(lastBlockContext?.envVars ?? {}),
            working_directory: lastBlockContext?.cwd || null,
            ssh_host: lastBlockContext?.sshHost || null,
          },
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

        // Increment usage count for AI hint dismissal
        incrementAIHintUseCount();
      } catch (error) {
        // Prevent duplicate error toasts
        if (errorToastShownRef.current) {
          console.log("[AI] Duplicate toast prevented", new Error().stack);
          return;
        }
        errorToastShownRef.current = true;
        console.log("[AI] Showing error toast", new Error().stack);

        const message =
          error instanceof AIFeatureDisabledError
            ? "AI feature is not enabled for your account"
            : error instanceof AIQuotaExceededError
            ? "AI quota exceeded"
            : error instanceof Error
            ? error.message
            : "Failed to generate blocks";

        addToast({
          title: error instanceof AIQuotaExceededError ? "Quota exceeded" : "Generation failed",
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
    [editor, getEditorContext, getBlockText],
  );

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
    if (!editor || !postGenerationBlockId || !editPrompt.trim() || generatedBlockIds.length === 0)
      return;

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

      const result = (await generateOrEditBlock({
        action: "edit",
        block: currentBlock,
        instruction: editPrompt,
        document_markdown: context?.documentMarkdown,
        runbook_id: context?.runbookId,
      })) as AISingleBlockResponse;

      // Replace the block with the edited version
      const newBlock = { ...result.block, id: currentBlock.id };
      isProgrammaticEditRef.current = true;
      editor.updateBlock(currentBlock.id, newBlock as any);
      // Reset after microtask to ensure onChange has fired
      queueMicrotask(() => {
        isProgrammaticEditRef.current = false;
      });

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

  // Callback for showing the edit popup
  const handleShowEditPopup = useCallback((position: { x: number; y: number }, block: any) => {
    setAiEditPopupPosition(position);
    setIsAIEditPopupOpen(true);
    setCurrentEditBlock(block);
  }, []);

  // AI keyboard shortcuts (Cmd+K, Cmd+Enter, Tab, Escape, E)
  useAIKeyboardShortcuts({
    editor,
    runbookId: runbook?.id,
    postGenerationBlockId,
    generatedBlockIds,
    generatedBlockCount,
    isEditingGenerated,
    isGeneratingInline,
    onShowAIPopup: showAIPopup,
    onShowEditPopup: handleShowEditPopup,
    onInlineGenerate: handleInlineGenerate,
    onClearPostGeneration: clearPostGenerationMode,
    onStartEditing: () => setIsEditingGenerated(true),
  });

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
    <ResizablePanelGroup direction="horizontal" className="h-full w-full min-h-0">
      {/* Main editor area */}
      <ResizablePanel defaultSize={isAIAssistantOpen ? 70 : 100} minSize={40}>
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
            if (postGenerationBlockId) {
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
                    insertPause(schema)(editor),
                    insertSubRunbook(editor as any),

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

          {/* Subtle hint for AI generation */}
          {aiEnabledState && showAiHint && !postGenerationBlockId && (
            <AIHint editor={editor} isGenerating={isGeneratingInline} aiEnabled={aiEnabledState} />
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

          {/* AI Assistant toggle button */}
        </div>
      </ResizablePanel>

      {/* AI Assistant sidebar */}
      {aiEnabledState && isAIAssistantOpen && (
        <>
          <ResizableHandle withHandle />
          <ResizablePanel defaultSize={30} minSize={20} maxSize={100}>
            <AIAssistant
              runbookId={runbook.id}
              editor={editor}
              getContext={getAIAssistantContext}
              isOpen={isAIAssistantOpen}
              chargeTarget={chargeTarget}
              onClose={closeAIAssistant}
            />
          </ResizablePanel>
        </>
      )}
    </ResizablePanelGroup>
  );
}
