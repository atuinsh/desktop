import { useEffect, useState, useCallback, useRef } from "react";
import { useCurrentRunbookId } from "@/context/runbook_id_context";
import { getBlockLocalState, setBlockLocalState } from "@/state/block_state";

/**
 * A hook that provides local state for a block that persists across reloads
 * but doesn't sync to other users.
 *
 * This is similar to useState, but the state is stored in SQLite and associated
 * with the block ID, so it persists across page reloads while remaining local to the user.
 *
 * @param blockId The ID of the block
 * @param propertyName The name of the property to store
 * @param defaultValue The default value to use if the property doesn't exist
 * @returns A tuple containing [value, setValue] similar to useState
 *
 * @example
 * const [collapsed, setCollapsed] = useBlockLocalState(block.id, "collapsed", false);
 */
export function useBlockLocalState<T extends string | number | boolean>(
  blockId: string,
  propertyName: string,
  defaultValue: T,
): [T, (value: T) => void] {
  const [value, setValue] = useState<T>(defaultValue);
  const currentRunbookId = useCurrentRunbookId();
  const isInitialized = useRef(false);

  // Load the value from the database when the component mounts or when deps change
  useEffect(() => {
    if (!currentRunbookId || !blockId || !propertyName) {
      return;
    }

    const loadValue = async () => {
      try {
        const storedValue = await getBlockLocalState(
          currentRunbookId,
          blockId,
          propertyName,
        );

        if (storedValue !== null) {
          // Convert the stored string back to the original type
          const parsedValue = parseValue(storedValue, defaultValue);
          setValue(parsedValue);
        }
      } catch (error) {
        console.error("Error loading block local state:", error);
      } finally {
        isInitialized.current = true;
      }
    };

    loadValue();
  }, [currentRunbookId, blockId, propertyName, defaultValue]);

  // Save the value to the database whenever it changes
  const setValueAndPersist = useCallback(
    (newValue: T) => {
      setValue(newValue);

      if (!currentRunbookId || !blockId || !propertyName || !isInitialized.current) {
        return;
      }

      // Convert the value to a string for storage
      const stringValue = String(newValue);

      setBlockLocalState(currentRunbookId, blockId, propertyName, stringValue).catch(
        (error) => {
          console.error("Error saving block local state:", error);
        },
      );
    },
    [currentRunbookId, blockId, propertyName],
  );

  return [value, setValueAndPersist];
}

/**
 * Helper function to parse a stored string value back to its original type
 */
function parseValue<T extends string | number | boolean>(
  storedValue: string,
  defaultValue: T,
): T {
  // Infer the type from the default value
  if (typeof defaultValue === "boolean") {
    return (storedValue === "true") as T;
  } else if (typeof defaultValue === "number") {
    const parsed = Number(storedValue);
    return (isNaN(parsed) ? defaultValue : parsed) as T;
  } else {
    return storedValue as T;
  }
}

