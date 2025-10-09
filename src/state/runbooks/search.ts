// Handle building and searching an index of runbooks
// TODO: Switch to indexing numeric IDs only

import FlexSearch from "flexsearch";
import Runbook from "./runbook";

interface RunbookDocument {
  id: string;
  title: string;
  content: string;
}

// Define field ranking weights
const FIELD_RANKS = {
  title: 2.0, // Title matches are 2x more important
  content: 1.0, // Base rank for content matches
};

class RunbookIndexService {
  private stored = new Map<string, Runbook>();
  private document: FlexSearch.Document<RunbookDocument>;
  private lastIds: Set<string> = new Set();

  constructor() {
    this.document = new FlexSearch.Document({
      document: {
        id: "id",
        index: [
          {
            field: "title",
            tokenize: "forward",
          },
          {
            field: "content",
            tokenize: "strict",
          },
        ],
      },
      cache: false,
    } as any);
  }

  public bulkUpdateRunbooks(runbooks: Runbook[]): void {
    const ids = new Set(runbooks.map((rb) => rb.id));
    const added = new Set([...ids].filter((id) => !this.lastIds.has(id)));
    const removed = new Set([...this.lastIds].filter((id) => !ids.has(id)));

    // Update lastIds for the next bulk update
    this.lastIds = ids;

    runbooks.forEach((runbook) => {
      if (added.has(runbook.id)) {
        this.addRunbook(runbook);
      } else if (removed.has(runbook.id)) {
        this.removeRunbook(runbook.id);
      } else {
        this.indexRunbook(runbook);
      }
    });
  }

  public indexRunbook(runbook: Runbook) {
    this.stored.set(runbook.id, runbook);
    this.updateRunbook(runbook);
  }

  private createDocumentObject(runbook: Runbook) {
    return {
      id: runbook.id,
      title: runbook.name,
      content: runbook.content,
    };
  }

  public addRunbook(runbook: Runbook) {
    this.stored.set(runbook.id, runbook);
    const doc = this.createDocumentObject(runbook);
    return this.document.addAsync(doc.id, doc);
  }

  public removeRunbook(id: string) {
    this.stored.delete(id);
    return this.document.removeAsync(id);
  }

  public async updateRunbook(runbook: Runbook) {
    await this.removeRunbook(runbook.id);
    await this.addRunbook(runbook);
    return runbook;
  }

  public async searchRunbooks(query: string): Promise<string[]> {
    if (!query || query.trim() === "") {
      return [];
    }

    // Search in all fields
    const results = await this.document.searchAsync(query, {
      enrich: true,
      limit: 50,
    });

    // Track document ranks with a map of id -> rank
    const documentRanks = new Map<string, number>();

    // Process results to calculate ranks based on field matches
    results.forEach((result: any) => {
      const fieldName = result.field;
      const fieldRank = FIELD_RANKS[fieldName as keyof typeof FIELD_RANKS] || 1.0;

      result.result.forEach((id: string) => {
        // If document already has a rank, add to it (matching multiple fields)
        // Otherwise initialize with the field's rank
        const currentRank = documentRanks.get(id) || 0;
        documentRanks.set(id, currentRank + fieldRank);
      });
    });

    // Convert to array of [id, rank] pairs and sort by rank (descending)
    const sortedResults = Array.from(documentRanks.entries())
      .sort((a, b) => b[1] - a[1]) // Sort by rank (descending)
      .map(([id]) => id); // Extract just the IDs

    return sortedResults;
  }
}

export default RunbookIndexService;
