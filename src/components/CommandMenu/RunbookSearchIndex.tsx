import RunbookIndexService from "@/state/runbooks/search";

interface RunbookSearchIndexProps {
  index: RunbookIndexService;
}

// TODO: Search indexing is temporarily disabled while we migrate away from
// storing runbook.content in the database. The search index needs to be
// moved to a Web Worker that can extract text from ydoc instead.
// See: https://github.com/atuinsh/desktop/issues/XXX
export default function RunbookSearchIndex(_props: RunbookSearchIndexProps) {
  // Disabled - search indexing will be re-enabled once moved to a worker
  return <div />;
}
