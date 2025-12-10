import * as Y from "yjs";
import * as awarenessProtocol from "y-protocols/awareness";

import SocketManager, { WrappedChannel } from "../socket";
import Logger from "@/lib/logger";
import Emittery from "emittery";
import WorkspaceSyncManager from "./sync/workspace_sync_manager";
import { timeoutPromise } from "./utils";
import { autobind } from "./decorators";
import { schema } from "@/components/runbooks/editor/create_editor";

type AwarenessData = { added: number[]; updated: number[]; removed: number[] };

export type PresenceUserInfo = { id: string; username: string; avatar_url: string; color: string };
type PresenceEntry = { metas: any[]; user: PresenceUserInfo };
type PresenceEntries = Record<string, PresenceEntry>;
type PresenceDiff = { joins: PresenceEntries; leaves: PresenceEntries };

export type SyncType = "online" | "offline" | "timeout" | "error";

/**
 * Handles synchronization of a Y.Doc with the server over a Phoenix channel.
 *
 * @emits `"synced", SyncType ("online" | "offline" | "timeout" | "error")`
 * @emits `"unsupported_block", string[]` when unknown block types are detected
 */
export class PhoenixSynchronizer extends Emittery {
  public static instanceCount = 0;

  protected connected: boolean;
  protected _channel: WrappedChannel | null = null;
  protected subscriptions: any[] = [];
  protected readonly runbookId: string;
  protected readonly requireLock: boolean;
  public readonly doc: Y.Doc;
  protected readonly awareness: awarenessProtocol.Awareness;
  protected logger: Logger;
  protected isSyncing: boolean = false;
  protected isShutdown: boolean = false;
  protected unlock: Function | null = null;
  protected presenceColor: string | null = null;

  // Verification infrastructure for validating incoming updates
  protected verificationDoc: Y.Doc;
  protected knownBlockTypes: Set<string>;

  constructor(runbookId: string, doc: Y.Doc, requireLock: boolean = true, isProvider = false) {
    super();

    this.runbookId = runbookId;
    this.doc = doc;
    this.awareness = new awarenessProtocol.Awareness(this.doc);
    this.requireLock = requireLock;

    // Initialize verification infrastructure (plain Y.Doc, no editor binding)
    this.verificationDoc = new Y.Doc();
    this.knownBlockTypes = new Set(Object.keys(schema.blockSpecs));

    PhoenixSynchronizer.instanceCount++;
    if (isProvider) {
      this.logger = new Logger(
        `PhoenixProvider (${runbookId}) - #${PhoenixSynchronizer.instanceCount}`,
        "blue",
        "cyan",
      );
      this.logger.debug("Creating new provider instance");
    } else {
      this.logger = new Logger(
        `PhoenixSynchronizer (${runbookId}) - #${PhoenixSynchronizer.instanceCount}`,
        "blue",
        "cyan",
      );
      this.logger.debug("Creating new synchronizer instance");
    }

    const manager = SocketManager.get();
    this.subscriptions.push(manager.onConnect(this.onSocketConnected));
    this.subscriptions.push(manager.onDisconnect(this.onSocketDisconnected));
    this.connected = manager.isConnected();

    setTimeout(() => this.init());
  }

  /**
   * Syncs the verification doc to match the main doc's current state.
   * Called before validating incoming updates.
   */
  protected syncVerificationDoc() {
    const currentState = Y.encodeStateAsUpdate(this.doc);
    Y.applyUpdate(this.verificationDoc, currentState);
  }

  /**
   * Applies an update to both the main doc and verification doc.
   * Use this instead of Y.applyUpdate(this.doc, ...) directly to keep docs in sync.
   */
  protected applyUpdate(update: Uint8Array, origin?: any) {
    Y.applyUpdate(this.doc, update, origin);
    Y.applyUpdate(this.verificationDoc, update, origin);
  }

  /**
   * Validates an update and applies it to both docs if valid.
   * Returns true if the update was valid and applied, false otherwise.
   * On invalid update, emits "unsupported_block" event and shuts down.
   */
  protected async validateAndApplyUpdate(update: Uint8Array, origin?: any): Promise<boolean> {
    // Sync verification doc to current main doc state
    this.syncVerificationDoc();

    // Apply update to verification doc and check for unknown block types
    Y.applyUpdate(this.verificationDoc, update);
    const fragment = this.verificationDoc.getXmlFragment("document-store");
    const unknownTypes = this.findUnknownBlockTypesInXml(fragment);

    if (unknownTypes.length > 0) {
      this.logger.warn(`Unsupported block types detected: ${unknownTypes.join(", ")}`);
      await this.emit("unsupported_block", unknownTypes);
      this.shutdown();
      return false;
    }

    // Validation passed - apply to main doc (verification doc already has it)
    Y.applyUpdate(this.doc, update, origin);
    return true;
  }

  /**
   * Debug helper to log the XML structure
   */
  // @ts-ignore - Unused but kept for future debugging
  protected debugLogXmlStructure(element: Y.XmlFragment | Y.XmlElement, depth: number) {
    const indent = "  ".repeat(depth);
    // Check XmlElement FIRST since it extends XmlFragment
    if (element instanceof Y.XmlElement) {
      this.logger.info(`${indent}<${element.nodeName}>`);
      for (const child of element.toArray()) {
        if (child instanceof Y.XmlElement) {
          this.debugLogXmlStructure(child, depth + 1);
        } else {
          this.logger.info(`${indent}  [XmlText]`);
        }
      }
    } else if (element instanceof Y.XmlFragment) {
      this.logger.info(`${indent}[XmlFragment]`);
      for (const child of element.toArray()) {
        if (child instanceof Y.XmlElement) {
          this.debugLogXmlStructure(child, depth + 1);
        } else {
          this.logger.info(`${indent}  [XmlText]`);
        }
      }
    }
  }

  /**
   * Recursively inspects the raw Y.XmlFragment for unknown block types.
   *
   * BlockNote document structure:
   * - XmlFragment "document-store"
   *   - blockGroup
   *     - blockContainer (for each block)
   *       - [actual block type, e.g., "paragraph", "run", etc.]
   *       - blockGroup (optional, for nested children)
   */
  protected findUnknownBlockTypesInXml(fragment: Y.XmlFragment): string[] {
    const unknown: string[] = [];

    const walkBlockGroup = (blockGroup: Y.XmlElement) => {
      for (const child of blockGroup.toArray()) {
        if (child instanceof Y.XmlElement && child.nodeName === "blockContainer") {
          walkBlockContainer(child);
        }
      }
    };

    const walkBlockContainer = (container: Y.XmlElement) => {
      for (const child of container.toArray()) {
        if (child instanceof Y.XmlElement) {
          if (child.nodeName === "blockGroup") {
            // Nested children - recurse
            walkBlockGroup(child);
          } else {
            // This is the actual block type
            const blockType = child.nodeName;
            if (!this.knownBlockTypes.has(blockType)) {
              if (!unknown.includes(blockType)) {
                unknown.push(blockType);
              }
            }
            // Check for nested blockGroup inside the block itself (e.g., for table cells)
            for (const innerChild of child.toArray()) {
              if (innerChild instanceof Y.XmlElement && innerChild.nodeName === "blockGroup") {
                walkBlockGroup(innerChild);
              }
            }
          }
        }
      }
    };

    // Start at the root XmlFragment
    for (const child of fragment.toArray()) {
      if (child instanceof Y.XmlElement && child.nodeName === "blockGroup") {
        walkBlockGroup(child);
      }
    }

    return unknown;
  }

  get channel() {
    if (this._channel) return this._channel;

    const manager = SocketManager.get();
    const channelParams = {
      use_presence: this.presenceColor !== null,
      presence_color: this.presenceColor,
    };
    this._channel = manager.channel(`doc:${this.runbookId}`, channelParams);
    return this._channel;
  }

  init() {
    if (this.isShutdown) return;

    if (!this.connected) {
      // onSocketConnected will be called immediately if the socket is already connected
      this.logger.debug("Socket disconnected; starting in offline mode");
      this.startOffline();
    }
  }

  startOffline() {
    this.emit("synced", "offline");
  }

  @autobind
  async onSocketConnected() {
    if (this.isShutdown) return;

    this.connected = true;

    this.logger.debug("Socket connected");
    if (this.channel.state == "closed") {
      try {
        await this.channel.ensureJoined();

        // Either this is the first connection, or we're reconnecting. Either way,
        // we need to resync with the remote document.
        this.resync();
      } catch (err: any) {
        this.logger.error("Failed to join doc channel", JSON.stringify(err));
        this.logger.debug("Starting in offline mode");
        this.startOffline();

        this.channel.nextJoin().then(() => {
          if (!this.isShutdown && !this.isSyncing) {
            this.logger.debug("Joined doc channel; resyncing");
            this.resync();
          }
        });
      }
    } else {
      this.resync();
    }
  }

  @autobind
  onSocketDisconnected() {
    if (this.connected) {
      this.logger.warn("Socket disconnected");
      this.connected = false;
    }
  }

  async resync() {
    if (this.isShutdown) return;
    if (this.isSyncing) {
      this.logger.warn("Already syncing; skipping resync");
      return;
    }

    this.isSyncing = true;
    if (this.requireLock) {
      this.logger.debug("Acquiring sync lock...");
      this.unlock = await WorkspaceSyncManager.syncMutex(this.runbookId).lock();
      if (this.isShutdown) {
        this.unlock();
        this.unlock = null;
        return;
      }
    }
    this.logger.info("Starting resync");

    try {
      const timerPromise = timeoutPromise(5000, false);
      const resyncPromise = this.doResync();
      const didResync = await Promise.race([timerPromise, resyncPromise]);
      if (!didResync) {
        this.logger.error("Resync timed out");
        this.emit("synced", "timeout");
      } else {
        this.emit("synced", "online");
      }
    } catch (err: any) {
      this.logger.error("Failed to resync", JSON.stringify(err));
      this.emit("synced", "error");
    } finally {
      this.isSyncing = false;
      if (this.unlock) {
        this.unlock();
        this.unlock = null;
      }
    }
  }

  protected async doResync(): Promise<boolean> {
    if (this.isShutdown) return false;

    // === VALIDATION PHASE ===
    // Use a fresh doc with empty state vector to get FULL server state for validation.
    // This ensures we validate what the SERVER has, independent of our local state.
    const freshDoc = new Y.Doc();
    const emptyStateVector = Y.encodeStateVector(freshDoc);
    this.logger.debug(`⬆️ [Validation] Requesting full server state`);
    const validationServerVector = await this.channel
      .push("sync_step_1", emptyStateVector)
      .receiveBin();
    const emptyDiff = Y.encodeStateAsUpdate(freshDoc, validationServerVector);
    const fullServerState = await this.channel.push("sync_step_2", emptyDiff).receiveBin();
    this.logger.debug(
      `⬇️ [Validation] Received full server state (${fullServerState.byteLength} bytes)`,
    );

    // Apply full server state to fresh doc and validate
    Y.applyUpdate(freshDoc, fullServerState);
    const fragment = freshDoc.getXmlFragment("document-store");
    const unknownTypes = this.findUnknownBlockTypesInXml(fragment);
    freshDoc.destroy();

    if (unknownTypes.length > 0) {
      this.logger.warn(
        `Unsupported block types detected during resync: ${unknownTypes.join(", ")}`,
      );
      await this.emit("unsupported_block", unknownTypes);
      this.shutdown();
      return false;
    }

    // === NORMAL SYNC PHASE ===
    const stateVector = Y.encodeStateVector(this.doc);
    this.logger.debug(`⬆️ Sending state vector (${stateVector.byteLength} bytes)`);
    const serverVector = await this.channel.push("sync_step_1", stateVector).receiveBin();
    this.logger.debug(`⬇️ Received server state vector (${serverVector.byteLength} bytes)`);

    const diff = Y.encodeStateAsUpdate(this.doc, serverVector);
    this.logger.debug(`⬆️ Sending state diff (${diff.byteLength} bytes)`);
    const serverDiff = await this.channel.push("sync_step_2", diff).receiveBin();
    this.logger.debug(`⬇️ Received server diff (${serverDiff.byteLength} bytes)`);

    if (!this.isShutdown) {
      // Use applyUpdate to keep both main doc and verification doc in sync
      this.applyUpdate(serverDiff, this);
      this.logger.info("Resync complete");
      return true;
    } else {
      this.logger.info("Skipping applying diff from server because provider is shut down");
      return false;
    }
  }

  shutdown() {
    if (this.isShutdown) return;

    this.logger.debug("Shutting down");
    this.isShutdown = true;
    PhoenixSynchronizer.instanceCount--;
    if (this.unlock) {
      this.unlock();
    }
    // disconnect from the server
    this.channel.leave();
    this.subscriptions.forEach((unsub) => unsub());
    // disconnect from the ydoc
    // shut down the event emitter
    this.clearListeners();
    this.verificationDoc.destroy();
  }
}

/**
 * As a sublcass of PhoenixSynchronizer, this class handles synchronization of a Y.Doc
 * with the server over a Phoenix channel. It also serves as a two-way synchronization provider
 * for BlockNote.
 *
 * @emits `"synced", SyncType ("online" | "offline" | "timeout" | "error")`
 * @emits `"remote_update"` when a remote update is received
 * @emits `"unsupported_block", string[]` when unknown block types are detected
 */
export default class PhoenixProvider extends PhoenixSynchronizer {
  protected pendingUpdate: Uint8Array | null = null;
  protected pendingLocalUpdate: Uint8Array | null = null;
  protected scheduledEmitAfterSync: boolean = false;
  private validationComplete: boolean = false;

  constructor(runbookId: string, doc: Y.Doc, presenceColor: string, requireLock: boolean = true) {
    super(runbookId, doc, requireLock, true);
    this.presenceColor = presenceColor;

    this.doc.on("update", this.handleDocUpdate);
    this.awareness.on("update", this.handleAwarenessUpdate);

    this.subscriptions.push(this.channel.on("apply_update", this.handleIncomingUpdate));
    this.subscriptions.push(this.channel.on("awareness", this.handleIncomingAwareness));
    this.subscriptions.push(this.channel.on("presence_state", this.handlePresenceState));
    this.subscriptions.push(this.channel.on("presence_diff", this.handlePresenceDiff));
  }

  /**
   * Override doResync to set validationComplete flag after validation passes.
   * This ungates handleDocUpdate so updates can be sent to the server.
   */
  protected async doResync(): Promise<boolean> {
    const result = await super.doResync();
    if (result) {
      // Validation passed - now allow updates to be sent to server
      this.validationComplete = true;

      // Send any local updates that were queued while waiting for validation
      if (this.pendingLocalUpdate && this.connected) {
        this.channel.push("client_update", this.pendingLocalUpdate.buffer);
        this.pendingLocalUpdate = null;
      }
    }
    return result;
  }

  emitAfterSync() {
    if (this.scheduledEmitAfterSync) return;

    this.scheduledEmitAfterSync = true;
    this.once("synced").then(async () => {
      if (this.pendingUpdate) {
        try {
          const valid = await this.validateAndApplyUpdate(this.pendingUpdate, this);
          if (!valid) return; // validateAndApplyUpdate handles emit + shutdown
          this.emit("remote_update");
        } catch (err) {
          this.logger.error("Failed to apply update", err);
          this.resync();
          this.emitAfterSync();
          return;
        }
      }
      this.pendingUpdate = null;
      this.scheduledEmitAfterSync = false;
    });
  }

  @autobind
  handleDocUpdate(update: Uint8Array, origin: any) {
    if (origin === this || !this.channel) return;

    // Don't send updates to server until we've validated the server state.
    // This prevents y-prosemirror deletions of unknown blocks from propagating.
    // Queue the updates to send after validation completes.
    if (!this.validationComplete) {
      if (this.pendingLocalUpdate) {
        this.pendingLocalUpdate = Y.mergeUpdates([this.pendingLocalUpdate, update]);
      } else {
        this.pendingLocalUpdate = update;
      }
      return;
    }

    if (this.connected) {
      this.channel.push("client_update", update.buffer);
    }
  }

  @autobind
  handleAwarenessUpdate({ added, updated, removed }: AwarenessData, origin: any) {
    if (origin === this || !this.channel || this.channel.state != "joined") return;

    const changedClients = added.concat(updated).concat(removed);
    if (this.connected) {
      this.channel.push(
        "client_awareness",
        awarenessProtocol.encodeAwarenessUpdate(this.awareness, changedClients).buffer,
      );
    }
  }

  @autobind
  async handleIncomingUpdate(payload: Uint8Array) {
    if (this.isSyncing) {
      if (this.pendingUpdate) {
        this.pendingUpdate = Y.mergeUpdates([this.pendingUpdate, payload]);
      } else {
        this.pendingUpdate = payload;
      }

      this.emitAfterSync();
      return;
    }

    try {
      const valid = await this.validateAndApplyUpdate(payload, this);
      if (!valid) return; // validateAndApplyUpdate handles emit + shutdown
      this.emit("remote_update");
    } catch (err) {
      this.logger.error("Failed to apply update", err);
      this.resync();
    }
  }

  @autobind
  handleIncomingAwareness(payload: Uint8Array) {
    try {
      awarenessProtocol.applyAwarenessUpdate(this.awareness, payload, this);
    } catch (err: any) {
      this.logger.error("Failed to apply awareness update", err);
      this.resync();
    }
  }

  @autobind
  handlePresenceState(presences: PresenceEntries) {
    for (const id in presences) {
      this.emit("presence:join", presences[id].user);
    }
  }

  @autobind
  handlePresenceDiff(diff: PresenceDiff) {
    for (const id in diff.joins) {
      this.emit("presence:join", diff.joins[id].user);
    }

    for (const id in diff.leaves) {
      this.emit("presence:leave", diff.leaves[id].user);
    }
  }

  shutdown() {
    if (this.isShutdown) return;

    super.shutdown();
    this.doc.off("update", this.handleDocUpdate);
    this.awareness.off("update", this.handleAwarenessUpdate);
  }
}
