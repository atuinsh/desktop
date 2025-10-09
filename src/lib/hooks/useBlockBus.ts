import { useEffect, useRef } from "react";
import BlockBus from "@/lib/workflow/block_bus.ts";

/**
 * Hook to subscribe to BlockBus run events for a specific block
 * 
 * @param blockId The ID of the block to subscribe to
 * @param handler The callback function to execute when the block is run
 */
export function useBlockBusRunSubscription(
  blockId: string, 
  handler: () => void | Promise<void>
) {
  const unsubscribeRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    // Clean up previous subscription if it exists
    if (unsubscribeRef.current) {
      unsubscribeRef.current();
    }

    // Subscribe to the run event for this block
    unsubscribeRef.current = BlockBus.get().subscribeRunBlock(blockId, handler);

    // Cleanup on unmount or when dependencies change
    return () => {
      if (unsubscribeRef.current) {
        unsubscribeRef.current();
      }
    };
  }, [blockId, handler]);
}

/**
 * Hook to subscribe to BlockBus stop events for a specific block
 * 
 * @param blockId The ID of the block to subscribe to
 * @param handler The callback function to execute when the block is stopped
 */
export function useBlockBusStopSubscription(
  blockId: string, 
  handler: () => void | Promise<void>
) {
  const unsubscribeRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    // Clean up previous subscription if it exists
    if (unsubscribeRef.current) {
      unsubscribeRef.current();
    }

    // Subscribe to the stop event for this block
    unsubscribeRef.current = BlockBus.get().subscribeStopBlock(blockId, handler);

    // Cleanup on unmount or when dependencies change
    return () => {
      if (unsubscribeRef.current) {
        unsubscribeRef.current();
      }
    };
  }, [blockId, handler]);
}

/**
 * Additional BlockBus hooks can be added here as needed:
 * - useBlockBusNameChangedSubscription
 * - useBlockBusDependencyChangedSubscription
 * etc.
 */ 