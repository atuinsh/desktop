import { queryOptions } from "@tanstack/react-query";
import { localQuery } from "./local_query";
import SavedBlock from "@/state/runbooks/saved_block";

export function savedBlocks() {
  return queryOptions({
    ...localQuery,
    queryKey: ["saved_blocks"],
    queryFn: () => {
      return SavedBlock.all();
    },
  });
}
