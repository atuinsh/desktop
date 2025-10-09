import { useEffect } from "react";
import Runbook, { OnlineRunbook } from "@/state/runbooks/runbook";
import { useQuery } from "@tanstack/react-query";
import { RemoteRunbook } from "@/state/models";
import { remoteRunbook } from "./queries/runbooks";

export default function useRemoteRunbook(
  runbook?: Runbook,
): [RemoteRunbook | null | undefined, () => void] {
  const query = useQuery(remoteRunbook(runbook));

  useEffect(() => {
    if (!runbook || !query.isFetched || !runbook.isOnline()) return;
    const rb = runbook as OnlineRunbook;

    if (query.isSuccess) {
      const newRemoteInfo = query.data ? JSON.stringify(query.data) : null;
      const currentRemoteInfo = rb.remoteInfo ? JSON.stringify(rb.remoteInfo) : null;

      if (newRemoteInfo != currentRemoteInfo) {
        rb.remoteInfo = newRemoteInfo;
        rb.save();
      }
    } else if (query.isError && rb.remoteInfo !== null) {
      rb.remoteInfo = null;
      rb.save();
    }
  }, [runbook?.id, query.isSuccess, query.isFetched, query.data]);

  return [query.data, query.refetch];
}
