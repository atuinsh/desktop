import * as jsondiffpatch from "jsondiffpatch";
import SocketManager, { WrappedChannel } from "@/socket";
import Emittery, { UnsubscribeFunction } from "emittery";
import { autobind } from "../decorators";
import Logger from "../logger";
import { Version, Event, ServerUpdate, ChangeRef, SharableState } from "./types";

type ResyncRequest = {
  last_known_version: Version;
};

type ResyncPayload<T> = {
  version: Version;
  data: T;
  change_refs: ChangeRef[];
};

export type ServerUpdatePayload = {
  version: Version;
  delta: jsondiffpatch.Delta;
  change_ref: ChangeRef;
};

/**
 * An adapter for managing a shared state. Responsible for receiving updates from
 * some endpoint.
 *
 * They should be cheap to construct, and do any heavy initialization in the `init()` method,
 * since they must be passed to `SharedStateManager.getInstance()`, even if the adapter doesn't
 * end up being used (due to an instance already existing for the given state ID).
 *
 * @param T - The type of the shared state.
 */
export interface SharedStateAdapter<T extends SharableState> {
  init(): Promise<void>;
  ensureConnected(): Promise<void>;
  subscribe(callback: (payload: ServerUpdate) => void): UnsubscribeFunction;
  resync(last_known_version: Version): Promise<ResyncPayload<T>>;
  destroy(): void;
}

/**
 * An adapter for managing a shared state document that uses Phoenix channels to communicate
 * with Atuin Hub.
 *
 * @param T - The type of the shared state.
 */
export class AtuinSharedStateAdapter<T extends SharableState> implements SharedStateAdapter<T> {
  private stateId: string;
  private channel: WrappedChannel | null;
  private emitter: Emittery = new Emittery();
  private logger: Logger | null;
  private unsub: UnsubscribeFunction | null = null;

  constructor(stateId: string) {
    this.stateId = stateId;
    this.channel = null;
    this.logger = null;
  }

  public async init() {
    this.logger = new Logger(`PhoenixSharedStateAdapter: ${this.stateId}`);
    const socketManager = SocketManager.get();

    socketManager.onConnect(() => {
      this.channel = socketManager.channel(`shared-state:${this.stateId}`);
      this.unsub = this.channel.on(Event.UPDATE, this.handleUpdate);
    });
  }

  public async ensureConnected() {
    if (!this.channel) {
      throw new Error("Channel not yet initialized");
    }

    if (this.channel!.state === "joined" || this.channel!.state === "joining") return;

    try {
      await this.channel!.ensureJoined();
    } catch (err) {
      this.logger!.error("Failed to join channel:", err);
      throw err;
    }
  }

  public subscribe(callback: (payload: ServerUpdate) => void): UnsubscribeFunction {
    return this.emitter.on(Event.UPDATE, callback);
  }

  public resync(last_known_version: Version): Promise<ResyncPayload<T>> {
    if (!this.channel) {
      throw new Error("Channel not yet initialized");
    }

    return this.channel!.push<ResyncRequest>(Event.RESYNC_REQ, { last_known_version }).receive<
      ResyncPayload<T>
    >();
  }

  public destroy() {
    this.logger!.debug("Shutting down");
    this.unsub?.();
    this.channel?.leave();
    this.emitter.clearListeners();
  }

  @autobind
  private handleUpdate(payload: ServerUpdatePayload) {
    this.emitter.emit(Event.UPDATE, payload);
  }
}

export class OfflineSharedStateAdapter<T extends SharableState> implements SharedStateAdapter<T> {
  async init(): Promise<void> {
    return;
  }
  async ensureConnected(): Promise<void> {
    return;
  }
  subscribe(_callback: (payload: ServerUpdate) => void): UnsubscribeFunction {
    return () => {};
  }
  async resync(_last_known_version: Version): Promise<ResyncPayload<T>> {
    throw new Error("Offline adapter does not support resync");
  }
  destroy(): void {
    return;
  }
}
