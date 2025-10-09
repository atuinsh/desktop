import { DefaultReactSuggestionItem } from "@blocknote/react";
import RunbookIndexService from "@/state/runbooks/search";
import Runbook from "@/state/runbooks/runbook";
import { useStore } from "@/state/store";

// Create a global search index instance
const searchIndex = new RunbookIndexService();

export function getRunbookLinkMenuItems(
  editor: any,
  query: string = "",
): Promise<DefaultReactSuggestionItem[]> {
  return new Promise(async (resolve) => {
    try {
      // Get current organization ID to scope runbooks to current org
      const { selectedOrg } = useStore.getState();
      console.log("Current organization ID:", selectedOrg);

      // Get runbooks from current organization only
      // TODO: support offline runbooks
      const runbooks = selectedOrg
        ? await Runbook.allFromOrg(selectedOrg)
        : await Runbook.allFromOrg(null); // fallback to all if no org selected

      // Update search index with current runbooks
      searchIndex.bulkUpdateRunbooks(runbooks);

      if (!query.trim()) {
        // If no query, show recent runbooks (sorted by updated date)
        const recentRunbooks = runbooks
          .slice()
          .sort((a: Runbook, b: Runbook) => b.updated.getTime() - a.updated.getTime())
          .slice(0, 10);

        const items = recentRunbooks.map((runbook: Runbook) => ({
          title: runbook.name || "Untitled Runbook",
          onItemClick: () => {
            editor.insertInlineContent([
              {
                type: "runbook-link",
                props: {
                  runbookId: runbook.id,
                  runbookName: runbook.name || "Untitled Runbook",
                },
              },
              " ", // add a space after the link
            ]);
          },
        }));

        resolve(items);
        return;
      }

      // Search runbooks
      searchIndex
        .searchRunbooks(query)
        .then((resultIds) => {
          const items: DefaultReactSuggestionItem[] = resultIds
            .map((id) => runbooks.find((rb: Runbook) => rb.id === id))
            .filter((rb): rb is Runbook => rb !== undefined)
            .slice(0, 10) // Limit to 10 results
            .map((runbook) => ({
              title: runbook.name || "Untitled Runbook",
              onItemClick: () => {
                editor.insertInlineContent([
                  {
                    type: "runbook-link",
                    props: {
                      runbookId: runbook.id,
                      runbookName: runbook.name || "Untitled Runbook",
                    },
                  },
                  " ", // add a space after the link
                ]);
              },
            }));

          resolve(items);
        })
        .catch((error) => {
          console.error("Error searching runbooks:", error);
          resolve([]);
        });
    } catch (error) {
      console.error("Error loading runbooks:", error);
      resolve([]);
    }
  });
}
