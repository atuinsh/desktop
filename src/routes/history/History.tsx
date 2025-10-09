import { useEffect, useState, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

import HistoryList from "@/components/HistoryList.tsx";
import HistorySearch from "@/components/HistorySearch.tsx";

import { AtuinState, useStore } from "@/state/store";
import { invoke } from "@tauri-apps/api/core";
import InstallCLI from "@/components/history/InstallCLI";
import clsx from "clsx";
import track_event from "@/tracking";
export default function Search() {
  const history = useStore((state: AtuinState) => state.shellHistory);
  const refreshHistory = useStore((state: AtuinState) => state.refreshShellHistory);
  const historyNextPage = useStore((state: AtuinState) => state.historyNextPage);

  let [query, setQuery] = useState("");
  let [cliInstalled, setCLIInstalled] = useState<boolean | null>(null);

  useEffect(() => {
    (async () => {
      // nothing rn
    })();

    track_event("history.open", {});

    refreshHistory();
  }, []);

  const parentRef = useRef<HTMLElement | null>(null);

  const rowVirtualizer = useVirtualizer({
    count: history.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 90,
    overscan: 5,
  });

  useEffect(() => {
    const [lastItem] = rowVirtualizer.getVirtualItems().slice(-1);

    if (!lastItem) return; // no undefined plz
    if (lastItem.index < history.length - 1) return; // if we're not at the end yet, bail

    // we're at the end! more rows plz!
    historyNextPage(query);
  }, [rowVirtualizer.getVirtualItems()]);

  useEffect(() => {
    (async () => {
      let installed = await invoke<boolean>("is_cli_installed");
      console.log("CLI installation status:", installed);
      setCLIInstalled(installed);
    })();
  }, []);

  return (
    <>
      <div className="h-full w-full flex-1 flex-col overflow-y-hidden">
        <div
          className={clsx({
            "flex shrink-0 items-center gap-x-4 border-b border-t": true,
            "border-gray-200 dark:border-gray-900 border-t-0 dark:border-divider bg-content1": true,
            "px-4 shadow-sm sm:gap-x-6 sm:px-6 lg:px-8 history-search": true,
          })}
        >
          <HistorySearch
            query={query}
            setQuery={(q) => {
              setQuery(q);
              refreshHistory(q);
            }}
            refresh={() => {
              refreshHistory(query);
            }}
          />
        </div>

        <main className="overflow-y-scroll history-list" ref={parentRef}>
          <HistoryList
            history={history}
            items={rowVirtualizer.getVirtualItems()}
            height={rowVirtualizer.getTotalSize()}
          />
        </main>

        {cliInstalled === false && <InstallCLI />}
      </div>
    </>
  );
}
