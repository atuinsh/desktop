import { User } from "@/state/models";
import Runbook, { OfflineRunbook, OnlineRunbook } from "@/state/runbooks/runbook";
import untitledRunbook from "@/state/runbooks/untitled.json";
import { BlockNoteEditor } from "@blocknote/core";
import track_event from "@/tracking";
import * as Y from "yjs";
import PhoenixProvider, { PresenceUserInfo } from "./phoenix_provider";
import {
  createBasicEditor,
  createCollaborativeEditor,
  createLocalOnlyEditor,
} from "@/components/runbooks/editor/create_editor";
import { randomColor } from "./colors";
import Logger from "./logger";
import Snapshot from "@/state/runbooks/snapshot";
import Operation from "@/state/runbooks/operation";
import { invoke } from "@tauri-apps/api/core";
import Emittery, { UnsubscribeFunction } from "emittery";

const SAVE_DEBOUNCE = 1000;
const SEND_CHANGES_DEBOUNCE = 100;

const RUNBOOK_EDITOR_CREATION_ERROR_MESSAGE =
  "There was an error creating the runbook editor. This usually means that the " +
  "runbook you're trying to open contains blocks that are not supported by your version of Atuin Desktop.";

function isContentBlank(content: any) {
  return (
    content.length === 0 ||
    (content.length === 1 &&
      content[0].content.length === 0 &&
      content[0].type === "paragraph" &&
      content[0].id === "initialBlockId")
  );
}

export default class RunbookEditor {
  public runbook: Runbook;
  private user: User;
  private selectedTag: string;
  private isOnline: boolean;
  private onPresenceJoin: (user: PresenceUserInfo) => void;
  private onPresenceLeave: (user: PresenceUserInfo) => void;
  private onClearPresences: () => void;
  private presenceColor: string | null = null;
  private yDoc: Y.Doc;
  private hashes: string[] = [];
  private emitter: Emittery;
  private sendChangeInProgress = false;

  private editor: Promise<BlockNoteEditor> | null = null;
  private provider: PhoenixProvider | null = null;

  private logger: Logger;
  private saveTimer: number | null = null;
  private sendChangesTimer: number | null = null;
  private saveArgs: [Runbook | undefined, BlockNoteEditor] | null = null;
  private isShutdown = false;
  private maybeNeedsContentConversion = true;
  private unsubSelectionChange: UnsubscribeFunction | null = null;

  constructor(
    runbook: Runbook,
    user: User,
    selectedTag: string | null,
    isOnline: boolean,
    onPresenceJoin: (user: PresenceUserInfo) => void,
    onPresenceLeave: (user: PresenceUserInfo) => void,
    onClearPresences: () => void,
  ) {
    this.logger = new Logger(`RunbookEditor (${runbook.id})`, "black", "white");
    this.runbook = runbook;
    this.user = user;
    this.selectedTag = selectedTag || "latest";
    this.isOnline = isOnline;
    this.onPresenceJoin = onPresenceJoin;
    this.onPresenceLeave = onPresenceLeave;
    this.onClearPresences = onClearPresences;
    this.yDoc = new Y.Doc();
    this.emitter = new Emittery();

    if (this.runbook instanceof OfflineRunbook) {
      this.hashes.push(this.runbook.contentHash);
    }

    if (
      this.runbook instanceof OnlineRunbook &&
      this.runbook.ydoc &&
      this.runbook.ydoc.byteLength > 0
    ) {
      Y.applyUpdate(this.yDoc, this.runbook.ydoc);
      // If the runbook had any bytes at all in its ydoc field, then the content
      // is already in the YJS document and we won't need to convert it.
      this.maybeNeedsContentConversion = false;
    }
  }

  updateRunbook(runbook: Runbook) {
    if (runbook.id !== this.runbook.id) {
      throw new Error("Can only update runbook with runbook of same ID");
    }

    this.runbook = runbook;
  }

  async updateUser(user: User) {
    if (this.user.is(user)) return;
    this.user = user;
    if (!this.editor) return;

    const editor = await this.editor;
    const extension = editor.getExtension("yCursor") as any; // yCursorPlugin is from y-prosemirror
    if (extension) {
      // https://github.com/TypeCellOS/BlockNote/blob/356a3ef7224fb0b4778a3b975ab84d5565344b62/packages/core/src/extensions/Collaboration/YCursorPlugin.ts#L178C7-L178C17
      extension.extension.updateUser({
        name: user.username,
        color: this.presenceColor || randomColor(),
      });
    }
  }

  updateSelectedTag(tag: string | null) {
    tag = tag || "latest";
    if (tag === this.selectedTag) return;

    this.editor = null;
    if (tag !== "latest") {
      this.flushSave();
      this.onClearPresences();
      this.provider?.shutdown();
      this.provider = null;
    }
    this.selectedTag = tag;
  }

  runbookUpdatedExternally(runbook: OfflineRunbook, contentHash: string) {
    if (runbook.id !== this.runbook.id) {
      return;
    }

    if (this.hashes.length > 0 && this.hashes[this.hashes.length - 1] === contentHash) {
      return;
    }

    this.editor?.then((editor) => {
      editor.replaceBlocks(editor.document, JSON.parse(runbook.content));
    });
  }

  setOnline(isOnline: boolean) {
    this.isOnline = isOnline;
  }

  resetEditor() {
    this.flushSave();
    this.onClearPresences();
    this.provider?.shutdown();
    this.provider = null;
    this.editor = null;
  }

  onBlockFocus(callback: (blockId: string) => void) {
    return this.emitter.on("block_focus", callback);
  }

  onUnsupportedBlock(callback: (unknownTypes: string[]) => void) {
    return this.emitter.on("unsupported_block", callback);
  }

  getEditor(): Promise<BlockNoteEditor> {
    if (this.editor) return this.editor;

    this.editor = new Promise(async (resolve, reject) => {
      // If viewing a tag, we just want a basic, no-frills editor
      if (this.selectedTag && this.selectedTag !== "latest") {
        const snapshot = await Snapshot.findByRunbookIdAndTag(this.runbook.id, this.selectedTag);
        if (!snapshot) {
          // Fallback to latest if snapshot not found
          this.selectedTag = "latest";
        } else {
          let editor: BlockNoteEditor | null = null;
          try {
            editor = createBasicEditor(JSON.parse(snapshot.content)) as any as BlockNoteEditor;
          } catch (error) {
            reject(new Error(RUNBOOK_EDITOR_CREATION_ERROR_MESSAGE));
            return;
          }
          resolve(editor as any as BlockNoteEditor);
          return;
        }
      }

      let content = JSON.parse(this.runbook.content || "[]");
      // convert any block of type sql -> sqlite
      for (var i = 0; i < content.length; i++) {
        if (content[i].type == "sql") {
          content[i].type = "sqlite";
        }
      }

      if (!this.isOnline) {
        let needsSave = false;
        if (content.length == 0) {
          content = untitledRunbook;
          needsSave = true;
        }

        let editor: BlockNoteEditor | null = null;
        try {
          editor = createLocalOnlyEditor(content) as any as BlockNoteEditor;
        } catch (error) {
          reject(new Error(RUNBOOK_EDITOR_CREATION_ERROR_MESSAGE));
          return;
        }
        if (needsSave) {
          this.runbook.content = JSON.stringify(content);
          this.save(this.runbook, editor as any as BlockNoteEditor);
        }

        this.unsubSelectionChange = editor.onSelectionChange((editor: any) => {
          const typedEditor = editor as BlockNoteEditor;
          this.emitter.emit("block_focus", typedEditor.getTextCursorPosition().block.id);
        });
        resolve(editor as any as BlockNoteEditor);
        return;
      }

      this.presenceColor = randomColor();
      const provider = new PhoenixProvider(this.runbook.id, this.yDoc, this.presenceColor);
      this.provider = provider;
      let editor: BlockNoteEditor | null = null;
      try {
        editor = createCollaborativeEditor(
          provider,
          this.user,
          this.presenceColor,
        ) as any as BlockNoteEditor;
      } catch (error) {
        reject(new Error(RUNBOOK_EDITOR_CREATION_ERROR_MESSAGE));
        return;
      }

      provider.on("remote_update", () => {
        this.save(this.runbook, editor as any as BlockNoteEditor);
      });

      provider.on("unsupported_block", async (unknownTypes: string[]) => {
        this.logger.warn(`Unsupported block types detected: ${unknownTypes.join(", ")}`);
        this.emitter.emit("unsupported_block", unknownTypes);
        this.shutdown();
      });

      provider.on("presence:join", this.onPresenceJoin);
      provider.on("presence:leave", this.onPresenceLeave);

      this.unsubSelectionChange = editor.onSelectionChange((editor: any) => {
        const typedEditor = editor as BlockNoteEditor;
        this.emitter.emit("block_focus", typedEditor.getTextCursorPosition().block.id);
      });

      if (this.maybeNeedsContentConversion) {
        provider.once("synced").then(() => {
          // If the loaded YJS doc has no content, and the server has no content,
          // we should take the `content` field (if any) and populate the editor
          // so that we trigger a save, creating the YJS document.
          //
          // This is common when importing a runbook to an online workspace via
          // Open in Desktop.
          //
          // This doesn't work if we set the content on the same tick, so defer it.
          setTimeout(() => {
            let currentContent = editor.document;

            if (isContentBlank(currentContent) && !this.isShutdown) {
              this.logger.info(
                "BlockNote editor has empty content after sync; inserting existing content.",
                JSON.stringify(currentContent),
                JSON.stringify(content),
              );

              editor.replaceBlocks(currentContent, content);
            }
          }, 100);
          resolve(editor as any as BlockNoteEditor);
        });
      } else {
        resolve(editor as any as BlockNoteEditor);
      }
    });
    // silence uncaught promise rejection errors
    this.editor.catch((_error) => {});

    return this.editor;
  }

  fetchName(blocks: any[]): string {
    // Infer the title from the first text block
    for (const block of blocks) {
      if (block.type == "heading" || block.type == "paragraph") {
        if (block.content.length == 0) continue;
        if (block.content[0].text.length == 0) continue;

        let name: string[] = block.content
          .filter((i: any) => i.type === "text")
          .map((i: any) => i.text);

        return name.join(" ");
      }
    }

    return "Untitled";
  }

  async save(runbook: Runbook | undefined, editorArg: BlockNoteEditor) {
    const editor = await this.getEditor();
    if (editor !== editorArg) {
      // Note[mkt]: I'm not sure why, but after BlockNote 0.39, `this.editor` is no longer the same as the editor from args.
      // `this.editor` contains a document that is out of date, so we go ahead and replace the editor with the one from the args.
      this.logger.warn("Replacing editor with editor from args");
      this.unsubSelectionChange?.();
      this.unsubSelectionChange = editorArg.onSelectionChange((editor: any) => {
        const typedEditor = editor as BlockNoteEditor;
        this.emitter.emit("block_focus", typedEditor.getTextCursorPosition().block.id);
      });
      this.editor = Promise.resolve(editorArg);
    }

    this.scheduleSendChanges();

    // Don't allow `onChange` events from BlockNote to fire a save
    // if we're viewing a tag
    if (this.selectedTag !== "latest") return;

    if (this.saveTimer) {
      clearTimeout(this.saveTimer);
    }

    this.saveArgs = [runbook, editor];
    this.saveTimer = setTimeout(() => {
      this.saveTimer = null;
      this.saveArgs = null;
      this._save(runbook, editor);
    }, SAVE_DEBOUNCE) as unknown as number;
  }

  flushSave() {
    if (this.saveTimer) {
      clearTimeout(this.saveTimer);
      this.saveTimer = null;
    }

    if (this.saveArgs) {
      this._save(...this.saveArgs);
      this.saveArgs = null;
    }
  }

  // Schedule a send changes event to the backend.
  // Changes are sent as fast as possible, once the previous
  // change has been sent.
  async scheduleSendChanges() {
    if (!this.sendChangeInProgress && !this.sendChangesTimer) {
      this._sendChanges();
      return;
    }

    if (!this.sendChangesTimer) {
      this.sendChangesTimer = setTimeout(() => {
        this.sendChangesTimer = null;
        this._sendChanges();
      }, SEND_CHANGES_DEBOUNCE) as unknown as number;
    }
  }

  async _sendChanges() {
    if (this.sendChangeInProgress) {
      this.scheduleSendChanges();
      return;
    }

    this.sendChangeInProgress = true;

    const editor = await this.editor;
    if (!editor) return;

    try {
      await invoke("update_document", {
        documentId: this.runbook.id,
        documentContent: editor.document,
      });
    } catch (error) {
      this.logger.error("Error updating document for runbook", this.runbook.id, error);
    } finally {
      this.sendChangeInProgress = false;
    }
  }

  async _save(runbookArg: Runbook | undefined, _editorArg: BlockNoteEditor) {
    if (!runbookArg) return;
    if (runbookArg.id !== this.runbook.id) {
      this.logger.warn(
        "Runbook from args not the same as runbook from container",
        runbookArg.id,
        this.runbook.id,
      );
      return;
    }
    const editor = await this.editor;
    if (!editor) return;
    // if (!editable) return; // TODO

    Runbook.count().then((num) => {
      track_event("runbooks.save", { total: num });
    });

    const previousName = this.runbook.name;
    this.runbook.name = this.fetchName(editor.document);
    this.runbook.content = JSON.stringify(editor.document);

    if (this.provider && this.runbook instanceof OnlineRunbook) {
      this.runbook.ydoc = Y.encodeStateAsUpdate(this.provider.doc);
    }

    // Hashes are only returned from `save` for offline runbooks
    const maybeHash = await this.runbook.save();
    if (
      maybeHash &&
      (this.hashes.length === 0 || this.hashes[this.hashes.length - 1] !== maybeHash)
    ) {
      this.hashes.push(maybeHash);
      if (this.hashes.length > 5) {
        this.hashes.shift();
      }
    }

    if (previousName !== this.runbook.name && this.isOnline) {
      const op = new Operation({
        operation: {
          type: "runbook_name_updated",
          runbookId: this.runbook.id,
          newName: this.runbook.name,
        },
      });
      op.save();
    }
  }

  shutdown() {
    this.isShutdown = true;
    this.onClearPresences();
    this.provider?.shutdown();
    this.flushSave();
  }
}
