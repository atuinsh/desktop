import { useStore } from "@/state/store";
import { TabUri } from "@/state/store/ui_state";

/**
 * Returns the current tab
 */
export const useCurrentTab = () => {
  const tabs = useStore((state) => state.tabs);
  const currentTabId = useStore((state) => state.currentTabId);

  return tabs.find((tab) => tab.id === currentTabId) || null;
};

/**
 * Returns the current tab's runbook ID, if the current tab is a runbook
 */
export const useCurrentTabRunbookId = () => {
  const currentTab = useCurrentTab();
  if (!currentTab) return null;

  const uri = new TabUri(currentTab.url);
  if (uri.isRunbook()) {
    return uri.getRunbookId()!;
  }

  return null;
};
