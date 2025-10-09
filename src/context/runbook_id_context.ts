import { createContext, useContext } from "react";

const RunbookIdContext = createContext<string | null>(null);
const useCurrentRunbookId = () => {
  return useContext(RunbookIdContext);
};

export default RunbookIdContext;
export { useCurrentRunbookId };
