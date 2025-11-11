import { BlockNoteEditor } from "@blocknote/core";
import { useStore } from "@/state/store";
import { useCurrentTab, useCurrentTabRunbookId } from "@/lib/hooks/useCurrentTab";

/**
 * @file editor_utils.ts
 * 
 * Utilities for accessing BlockNote editors from various contexts.
 * 
 * ## Getting the Current Tab/Runbook ID
 * 
 * There are multiple ways to get the current tab or runbook ID depending on your context:
 * 
 * ### In React Components:
 * - `useCurrentTab()` - Hook that returns the current tab object
 * - `useCurrentTabRunbookId()` - Hook that returns the current runbook ID (if viewing a runbook)
 * - `useParams()` - React Router hook to get URL params like `{ runbookId }`
 * - `useContext(TabsContext)` - Access tab info via context
 * - `useContext(RunbookIdContext)` - Access runbook ID via context
 * 
 * ### Outside React (e.g., in AI agent tools):
 * - `useStore.getState().currentTabId` - Direct access to current tab ID
 * - `getCurrentEditor()` - Get the current editor directly
 * - `getEditorByRunbookId(id)` - Get editor by runbook ID
 * 
 * ### Example Usage:
 * ```typescript
 * // In a React component:
 * const editor = useCurrentEditor();
 * const runbookId = useCurrentTabRunbookId();
 * 
 * // In an AI agent tool (non-React):
 * const editor = getCurrentEditor();
 * const currentTabId = useStore.getState().currentTabId;
 * ```
 */

/**
 * Get the BlockNote editor for the currently active tab
 * 
 * NOTE: This is a non-hook utility for use in non-React contexts (like the AI agent).
 * For React components, consider using `useCurrentEditor()` instead.
 * 
 * @returns The BlockNote editor instance, or null if no editor is available
 */
export function getCurrentEditor(): BlockNoteEditor | null {
  const state = useStore.getState();
  const { currentTabId, tabs } = state;
  
  if (!currentTabId) {
    return null;
  }
  
  const currentTab = tabs.find((tab) => tab.id === currentTabId);
  return currentTab?.editor || null;
}

/**
 * React hook to get the BlockNote editor for the currently active tab
 * 
 * This is the preferred method for React components as it will re-render
 * when the current tab changes.
 * 
 * @returns The BlockNote editor instance, or null if no editor is available
 */
export function useCurrentEditor(): BlockNoteEditor | null {
  const currentTab = useCurrentTab();
  return currentTab?.editor || null;
}

/**
 * Get the BlockNote editor for a specific tab by tab ID
 * @param tabId The ID of the tab
 * @returns The BlockNote editor instance, or null if no editor is available
 */
export function getEditorByTabId(tabId: string): BlockNoteEditor | null {
  const state = useStore.getState();
  const tab = state.tabs.find((tab) => tab.id === tabId);
  return tab?.editor || null;
}

/**
 * Get the BlockNote editor for a specific runbook by runbook ID
 * @param runbookId The ID of the runbook
 * @returns The BlockNote editor instance, or null if no editor is available
 */
export function getEditorByRunbookId(runbookId: string): BlockNoteEditor | null {
  const state = useStore.getState();
  const runbookUrl = `/runbook/${runbookId}`;
  const tab = state.tabs.find((tab) => tab.url === runbookUrl);
  return tab?.editor || null;
}

