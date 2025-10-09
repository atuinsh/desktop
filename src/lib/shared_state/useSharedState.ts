import { useCallback, useEffect, useRef, useState } from "react";
import { SharedStateManager } from "./manager";
import { AtuinSharedStateAdapter } from "./adapter";
import { SharableState } from "./types";
import { Rc } from "@binarymuse/ts-stdlib";

async function defaultUpdateOptimistic<T>(
  _callback: (data: T, cancel: () => undefined) => T | undefined,
): Promise<string | undefined> {
  throw new Error("Shared state is not ready");
}

/**
 * Use a shared state document.
 *
 * To apply optimistic updates, pass a function to `updateOptimistic` that synchronously
 * applies an update to the data and returns undefined, or returns a new copy of the data
 * to diff against. The function will be called with the current state. The call to
 * `updateOptimistic` will return a promise to the change reference for the update.
 *
 * ```
 * const [data, updateOptimistic] = useSharedState("state-id");
 *
 * const changeRef = await updateOptimistic((data) => {
 *   data.foo = "bar";
 * });
 * ```
 *
 * For more information see {@link SharedStateManager}.
 *
 * @param T - The type of the shared state document; must be a JSON-serializable object (not an array).
 * @param stateId - The ID of the shared state document.
 * @returns A tuple containing the current state and a function for optimistically updating the state.
 */
export default function useSharedState<T extends SharableState>(
  stateId: string,
): [
  T,
  (callback: (data: T, cancel: () => undefined) => T | undefined) => Promise<string | undefined>,
] {
  const [data, setData] = useState<T>({} as T);
  const managerRef = useRef<SharedStateManager<T> | null>(null);

  useEffect(() => {
    let disposed = false;
    const manager = SharedStateManager.getInstance(
      stateId,
      new AtuinSharedStateAdapter<T>(stateId),
    );
    const unsub = manager.subscribe((data) => {
      console.log("folder data", data);
      setData(data);
    });

    manager.getDataOnce().then((data) => {
      console.log("folder initial data", data);
      if (!disposed) {
        setData(data);
      }
    });

    managerRef.current = manager;

    return () => {
      disposed = true;
      unsub();
      Rc.dispose(manager);
      managerRef.current = null;
    };
  }, [stateId]);

  const updateOptimistic = useCallback(
    (callback: (data: T, cancel: () => undefined) => T | undefined) => {
      return managerRef.current?.updateOptimistic(callback) || defaultUpdateOptimistic(callback);
    },
    [],
  );

  return [data, updateOptimistic];
}
