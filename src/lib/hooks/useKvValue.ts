import { useCurrentRunbookId } from "@/context/runbook_id_context";
import { KVStore } from "@/state/kv";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

/**
 * A hook that fetches a value from the KV store and updates it when it changes.
 * For blocks, prefer `useBlockKvValue` instead, so the backend runtime system
 * can be notified when the value changes.
 *
 * @param key The key to fetch the value from.
 * @param defaultValue The default value to use if the value is not found.
 * @returns A tuple containing the current value and a function to update the value.
 */
export function useKvValue<T>(key: string, defaultValue: T): [T, (value: T) => Promise<void>] {
  const [value, setValue] = useState<T>(defaultValue);

  const updateValue = async (value: T) => {
    setValue(value);
    const db = await KVStore.open_default();
    await db.set(key, value);
  };

  useEffect(() => {
    (async () => {
      const db = await KVStore.open_default();
      const value = await db.get<T>(key);
      if (value) {
        setValue(value);
      }
    })();
  }, [key]);

  return [value, updateValue];
}

/**
 * A hook that fetches a value from the KV store and updates it when it changes.
 * The value is stored in the KV store under the key `block.${blockId}.${key}`.
 * Updates to the value are propagated to the backend runtime system.
 *
 * @param blockId The ID of the block to store the value for
 * @param key The key to store the value under
 * @param defaultValue The default value to use if the value is not found.
 * @returns A tuple containing the current value and a function to update the value.
 */
export function useBlockKvValue<T>(
  blockId: string,
  key: string,
  defaultValue: T,
): [T, (value: T) => Promise<void>] {
  const runbookId = useCurrentRunbookId();
  if (!runbookId) {
    throw new Error("useBlockKvValue must be used within a runbook context");
  }

  const storeKey = `block.${blockId}.${key}`;
  const [value, updateValue] = useKvValue(storeKey, defaultValue);

  const wrappedUpdateValue = async (value: T) => {
    await updateValue(value);
    await invoke("notify_block_kv_value_changed", {
      documentId: runbookId,
      blockId,
      key,
      value,
    });
  };

  return [value, wrappedUpdateValue];
}
