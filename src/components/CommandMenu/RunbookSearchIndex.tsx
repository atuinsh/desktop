import { allRunbookIds, runbookById } from "@/lib/queries/runbooks";
import RunbookIndexService from "@/state/runbooks/search";
import { useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

interface RunbookSearchIndexProps {
  index: RunbookIndexService;
}

// This component is written such that it assumes `prop.index` will never change.
export default function RunbookSearchIndex(props: RunbookSearchIndexProps) {
  const queryClient = useQueryClient();
  const { data: ids } = useQuery(allRunbookIds());

  // By dynamically enabling or disabling individual queries, we can avoid
  // constant re-fetching of runbooks that are already in the cache.
  const runbooks = useQueries({
    queries: (ids || []).map((id) => {
      return {
        ...runbookById(id),
        staleTime: 5 * 60 * 1000,
        enabled: !queryClient.getQueryData(["runbook", id]),
      };
    }),
  });

  useEffect(() => {
    if (!runbooks) return;

    const readyRunbooks = runbooks.map((r) => r.data).filter((data) => !!data);
    props.index.bulkUpdateRunbooks(readyRunbooks);
  }, [runbooks]);

  return <div />;
}
