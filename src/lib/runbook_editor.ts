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

const SAVE_DEBOUNCE = 1000;

function isContentBlank(content: any) {
  return (
    content.length === 1 &&
    content[0].content.length === 0 &&
    content[0].type === "paragraph" &&
    content[0].id === "initialBlockId"
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
  private yDoc: Y.Doc;
  private hashes: string[] = [];

  private editor: Promise<BlockNoteEditor> | null = null;
  private provider: PhoenixProvider | null = null;

  private logger: Logger;
  private saveTimer: number | null = null;
  private saveArgs: [Runbook | undefined, BlockNoteEditor] | null = null;
  private isShutdown = false;
  private maybeNeedsContentConversion = true;

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
    const extension: any = editor.extensions.collaborationCursor;
    if (extension) {
      extension.options.user.name = user.username || "Anonymous";
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

  getEditor(): Promise<BlockNoteEditor> {
    if (this.editor) return this.editor;

    this.editor = new Promise(async (resolve) => {
      // If viewing a tag, we just want a basic, no-frills editor
      if (this.selectedTag && this.selectedTag !== "latest") {
        const snapshot = await Snapshot.findByRunbookIdAndTag(this.runbook.id, this.selectedTag);
        if (!snapshot) {
          // Fallback to latest if snapshot not found
          this.selectedTag = "latest";
        } else {
          const editor = createBasicEditor(JSON.parse(snapshot.content));
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

        const editor = createLocalOnlyEditor(content);
        if (needsSave) {
          this.runbook.content = JSON.stringify(content);
          this.save(this.runbook, editor as any as BlockNoteEditor);
        }
        resolve(editor as any as BlockNoteEditor);
        return;
      }

      const presenceColor = randomColor();
      const provider = new PhoenixProvider(this.runbook.id, this.yDoc, presenceColor);
      this.provider = provider;
      const editor = createCollaborativeEditor(provider, this.user, presenceColor);

      provider.on("remote_update", () => {
        this.save(this.runbook, editor as any as BlockNoteEditor);
      });

      provider.on("presence:join", this.onPresenceJoin);
      provider.on("presence:leave", this.onPresenceLeave);

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

  save(runbook: Runbook | undefined, editor: BlockNoteEditor) {
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

  async _save(runbookArg: Runbook | undefined, _editorArg: BlockNoteEditor) {
    // Note [MKT]: As of BlockNote 0.39.x, `editorArg` is no longer === to this.editor.
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
