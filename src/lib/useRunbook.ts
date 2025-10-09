import Runbook from "@/state/runbooks/runbook";
import { useQuery } from "@tanstack/react-query";
import { runbookById } from "./queries/runbooks";

export function useRunbook(id: string | undefined): Runbook | null {
  const query = useQuery(runbookById(id));

  if (query.isFetched) {
    return query.data || null;
  } else {
    return null;
  }
}
