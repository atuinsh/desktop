import * as jsondiffpatch from "jsondiffpatch";
import Emittery, { UnsubscribeFunction } from "emittery";
import { SharedStateAdapter } from "./adapter";
import { ChangeRef, ServerUpdate, Version, Event, SharableState, OptimisticUpdate } from "./types";
import { uuidv7 } from "uuidv7";
import { autobind } from "../decorators";
import Logger from "../logger";
import {
  getSharedStateDocument,
  pushOptimisticUpdate,
  removeOptimisticUpdates,
  updateSharedStateDocument,
} from "./commands";
import { useStore } from "@/state/store";
import { ConnectionState } from "@/state/store/user_state";
import { Rc } from "@binarymuse/ts-stdlib";
import { timeoutPromise } from "../utils";

function applyOptimisticUpdates<T>(data: T, updates: Array<OptimisticUpdate>) {
  for (const update of updates) {
    patchMutate(data, update.delta);
  }
  return data;
}

/**
 * A manager for a shared state. The state must be an object (not an array) that can be
 * serialized to JSON.
 *
 * @param T - The type of the shared state.
 * @param stateId - The ID of the shared state document.
 * @param adapter - The adapter for managing the shared state. See {@link SharedStateAdapter}.
 */
export class SharedStateManager<T extends SharableState> {
  private static instanceMap = new Map<string, Rc<SharedStateManager<any>>>();
  private static logger = new Logger("SharedStateManager (static)");

  public readonly stateId: string;

  private emitter = new Emittery();
  private unsub: UnsubscribeFunction | null = null;
  private adapter: SharedStateAdapter<T>;

  private _data: T = {} as T;
  private _optimisticData: T | null = null;
  private initialized: boolean = false;

  private optimisticUpdates: Array<OptimisticUpdate> = [];
  private version: Version = -1;
  private cached_updates: Map<Version, ServerUpdate> = new Map();
  private logger: Logger;

  private shutdown = false;
  private resyncing = false;

  public static startInstance<T extends SharableState>(
    stateId: string,
    adapter: SharedStateAdapter<T>,
  ): void {
    const manager = this.getInstance(stateId, adapter);
    // Dispose immediately, since we're storing a strong reference in the map
    Rc.dispose(manager);
  }

  public static stopInstance(stateId: string) {
    if (this.instanceMap.has(stateId)) {
      const rc = this.instanceMap.get(stateId)!;
      const info = Rc.inspect(rc);
      if (info.refCount > 1) {
        this.logger.warn(
          `Stopping shared state manager for ${stateId} but strong ref count is higher than the expected of 1: ${info.refCount}`,
        );
      }
      Rc.dispose(rc);

      if (this.instanceMap.has(stateId)) {
        this.logger.error(
          `Shared state manager for ${stateId} still exists in the map after stopInstance()`,
        );
      }
    }
  }

  public static getInstance<T extends SharableState>(
    stateId: string,
    adapter: SharedStateAdapter<T>,
  ): Rc<SharedStateManager<T>> {
    if (this.instanceMap.has(stateId)) {
      return Rc.clone(this.instanceMap.get(stateId)!);
    }

    const manager = new SharedStateManager<T>(stateId, adapter);
    const rc = Rc(manager, () => {
      this.instanceMap.delete(stateId);
      manager.destroy();
    });

    this.instanceMap.set(stateId, rc);
    return Rc.clone(rc);
  }

  private constructor(stateId: string, adapter: SharedStateAdapter<T>) {
    this.stateId = stateId;
    this.adapter = adapter;
    this.logger = new Logger(`SharedStateManager: ${stateId}`);
    this.setup();
  }

  /**
   * Subscribe to updates to the shared state.
   *
   * @param callback - A function that will be called with the latest state.
   * @returns A function that can be called to cancel the subscription.
   */
  public subscribe(callback: (data: T) => void): UnsubscribeFunction {
    return this.emitter.on(Event.UPDATE, callback);
  }

  public getDataOnce(): Promise<T> {
    if (this.initialized) {
      return Promise.resolve(this.data);
    } else {
      return this.emitter.once(Event.UPDATE);
    }
  }

  /**
   * `updateOptimistic` can be used to update the stored data; the change to the data
   * is stored locally as an optimistic update. When the adapter acknowledges the update
   * (by sending a delta with the same change reference), the optimistic update is applied
   * permanently to the shared state and the temporary update is discarded, even if the
   * delta from the server is different from the one stored in the optimistic update.
   *
   * @param callback - A function that **synchronously** applies an optimistic update to the data
   * and returns undefined, or else **synchronously** returns a new copy of the data that should
   * be used instead. A function that cancels the update can be passed as the second argument;
   * if the update is cancelled, `updateOptimistic` will return `undefined` instead of a change
   * reference.
   * @returns A promise to the change reference for the update, or `undefined` if the update was
   * cancelled.
   */
  @autobind
  public async updateOptimistic(
    callback: (data: T, cancel: () => undefined) => T | undefined,
  ): Promise<ChangeRef | undefined> {
    const orig = jsondiffpatch.clone(this.data) as T;
    const clone = jsondiffpatch.clone(this.data) as T;

    let cancelled = false;
    const cancel = () => {
      cancelled = true;
      return undefined;
    };
    let changed = callback(clone, cancel);
    if (cancelled) {
      this.logger.debug("Optimistic update cancelled");
      return undefined;
    }

    if (changed === undefined) {
      changed = clone;
    }

    const update = {
      sourceVersion: this.version,
      delta: jsondiffpatch.diff(orig, changed) || {},
      changeRef: uuidv7(),
    };

    this.logger.debug("Pushing optimistic update", update);

    pushOptimisticUpdate(this.stateId, update).catch((err) => {
      this.logger.error("Failed to push optimistic update to the backend!", err);
    });

    this.optimisticUpdates.push(update);
    this.setOptmisticData();
    this.emitter.emit(Event.UPDATE, this.data);
    return update.changeRef;
  }

  public destroy() {
    this.logger.debug("Shutting down");
    this.shutdown = true;
    this.adapter.destroy();
    this.unsub?.();
    this.emitter.clearListeners();
  }

  /**
   * Get the current state, including optimistic updates.
   */
  get data(): T {
    return this._optimisticData || this._data;
  }

  private async setData(data: T, version: Version, updateDocument = true) {
    this._data = data;
    this.version = version;

    if (updateDocument) {
      await updateSharedStateDocument(this.stateId, data, version);
    }

    this.setOptmisticData();
    this.emitter.emit(Event.UPDATE, this.data);
    this.initialized = true;
  }

  private setOptmisticData() {
    if (this.optimisticUpdates.length > 0) {
      const clone = jsondiffpatch.clone(this._data) as T;
      this._optimisticData = applyOptimisticUpdates(clone, this.optimisticUpdates);
    } else {
      this._optimisticData = null;
    }
  }

  private async setup() {
    const document = await getSharedStateDocument<T>(this.stateId);
    if (document) {
      this.optimisticUpdates = document.optimisticUpdates;
      this.setData(document.value, document.version, false);
      this.logger.debug(`Setup; loaded version ${document.version}`, this.data);
    }

    await this.adapter.init();
    this.unsub = this.adapter.subscribe(this.handleUpdate);

    useStore.subscribe(
      (state) => state.connectionState,
      (connectionState) => {
        if (connectionState === ConnectionState.Online) {
          // onConnect is async on the next tick, so wait a small amount of time to init
          timeoutPromise(100, undefined).then(() => {
            return this.adapter.ensureConnected()
          }).then(() => {
            return this.resync();
          }).catch((err) => {
            this.logger.error("Failed to ensure channel connection and resync:", err);
          });
        }
      },
      {
        fireImmediately: true,
      },
    );
  }

  private async resync() {
    if (this.shutdown || this.resyncing) return;
    if (useStore.getState().connectionState !== ConnectionState.Online) return;

    this.resyncing = true;
    this.logger.info("Resyncing...");

    let newVersion: Version;
    let newData: T;

    while (true) {
      const resync = await this.adapter.resync(this.version);

      if (this.shutdown) return;

      this.logger.debug(`Resynced; received version ${resync.version}`);
      newVersion = resync.version;
      newData = resync.data;
      const changeRefs = resync.change_refs;

      this.removeOptimisticUpdates(changeRefs);

      const maxVersion = Math.max(...this.cached_updates.keys());
      if (maxVersion > newVersion) {
        this.logger.debug(
          `Applying cached updates for versions ${newVersion + 1} through ${maxVersion}...`,
        );
        for (let i = newVersion + 1; i <= maxVersion; i++) {
          const update = this.cached_updates.get(i);
          if (update && Object.keys(update.delta as object).length > 0) {
            patchMutate(newData, update.delta);
            newVersion = i;
          } else if (update) {
            this.logger.debug(`Skipping cached update for version ${i}; delta is empty`);
            newVersion = i;
          } else {
            this.logger.warn(`Missing cached update for version ${i}; attempting resync again`);
            continue;
          }
        }
      }

      break;
    }

    for (const v of this.cached_updates.keys()) {
      if (v < newVersion) {
        this.cached_updates.delete(v);
      }
    }

    this.resyncing = false;

    this.setData(newData, newVersion);
  }

  @autobind
  private async handleUpdate(payload: ServerUpdate) {
    if (this.shutdown) return;

    const expectedVersion = this.version + 1;
    if (payload.version < expectedVersion) {
      this.logger.debug(
        `Ignoring update; version ${payload.version} is less than expected version ${expectedVersion}`,
      );
      return;
    } else if (payload.version > expectedVersion) {
      this.logger.debug(
        `Unexpected update; version ${payload.version} is greater than expected version ${expectedVersion}`,
      );
      if (this.resyncing) {
        this.cached_updates.set(payload.version, payload);
      } else {
        await this.resync();
      }
    } else {
      this.logger.debug(`Applying update; final version: ${payload.version}`);

      // If this update was sent as a response to an optimistic update,
      // remove the optimistic update from the list on both the frontend and backend.
      if (payload.change_ref) {
        await this.removeOptimisticUpdates([payload.change_ref]);
      }

      if (Object.keys(payload.delta as object).length > 0) {
        const clone = patchClone(this._data, payload.delta);

        if (!this.shutdown) {
          this.setData(clone, payload.version);
        }
      } else {
        this.version = payload.version;
        await updateSharedStateDocument(this.stateId, this._data, this.version);
      }
    }
  }

  public async expireOptimisticUpdates(changeRefs: ChangeRef[]) {
    await this.removeOptimisticUpdates(changeRefs);
    this.setOptmisticData();
    this.emitter.emit(Event.UPDATE, this.data);
  }

  private async removeOptimisticUpdates(changeRefs: ChangeRef[]) {
    if (changeRefs.length === 0) return;

    const set = new Set(changeRefs);
    this.optimisticUpdates = this.optimisticUpdates.filter((update) => !set.has(update.changeRef));
    await removeOptimisticUpdates(this.stateId, changeRefs);
  }
}

function patchMutate<T>(data: T, delta: jsondiffpatch.Delta) {
  // JsonDiffEx can send an empty delta when the data hasn't changed;
  // `jsondiffpatch.patch()` will throw an error in this case, so we need to check for an empty delta.
  if (Object.keys(delta as object).length > 0) {
    // `patch` mutates the delta object, so we need to clone it first
    const deltaClone = jsondiffpatch.clone(delta) as jsondiffpatch.Delta;

    jsondiffpatch.patch(data, deltaClone);
  }
}

function patchClone<T>(data: T, delta: jsondiffpatch.Delta): T {
  const clone = jsondiffpatch.clone(data) as T;
  patchMutate(clone, delta);
  return clone;
}
