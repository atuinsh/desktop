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
            tokenize: "forward",
            // Limit token size to avoid indexing massive strings (API keys, etc)
            minlength: 2,   // Ignore 1-char tokens
            threshold: 20,  // Max token length (most words < 20 chars, API keys much longer)
          },
        ],
      },
      cache: false,
    } as any);
  }

  public bulkUpdateRunbooks(runbooks: Runbook[]): void {
    console.log('[RunbookIndexService] bulkUpdateRunbooks called with', runbooks.length, 'runbooks');
    
    const ids = new Set(runbooks.map((rb) => rb.id));
    const added = new Set([...ids].filter((id) => !this.lastIds.has(id)));
    const removed = new Set([...this.lastIds].filter((id) => !ids.has(id)));

    console.log('[RunbookIndexService] Added:', added.size, 'Removed:', removed.size, 'Existing:', ids.size - added.size);

    // Update lastIds for the next bulk update
    this.lastIds = ids;

    runbooks.forEach((runbook) => {
      if (added.has(runbook.id)) {
        this.addRunbook(runbook);
      } else {
        // Only re-index if the runbook has changed
        const existing = this.stored.get(runbook.id);
        if (existing && (existing.name !== runbook.name || existing.content !== runbook.content)) {
          this.indexRunbook(runbook);
        }
      }
    });
    
    // Remove deleted runbooks
    removed.forEach((id) => {
      this.removeRunbook(id);
    });
    
    console.log('[RunbookIndexService] After bulk update, total indexed:', this.stored.size);
  }

  public indexRunbook(runbook: Runbook) {
    console.log('[RunbookIndexService] Indexing runbook:', runbook.id, runbook.name);
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

  /**
   * Deep content search optimized for AI agent queries
   * More aggressive tokenization and broader search for semantic/contextual matches
   */
  public async deepSearchRunbooks(query: string, limit: number = 10): Promise<string[]> {
    if (!query || query.trim() === "") {
      return [];
    }

    console.log('[RunbookIndexService] deepSearchRunbooks query:', query);
    console.log('[RunbookIndexService] Total runbooks indexed:', this.stored.size);
    
    // Debug: Show sample of what's indexed
    const sampleIds = Array.from(this.stored.keys()).slice(0, 3);
    console.log('[RunbookIndexService] Sample indexed runbook IDs:', sampleIds);
    sampleIds.forEach(id => {
      const rb = this.stored.get(id);
      if (rb) {
        console.log(`  - ${id}: "${rb.name}" (content length: ${rb.content.length})`);
      }
    });
    
    // Use FlexSearch's bool: "or" with forward tokenization
    const results = await this.document.searchAsync(query, {
      enrich: true,
      limit: 100,
      bool: "or", // Match any term in the query
    });
    
    console.log('[RunbookIndexService] FlexSearch raw results count:', results?.length || 0);
    if (results && results.length > 0) {
      results.forEach((fieldResult: any, idx: number) => {
        console.log(`  Field ${idx}: ${fieldResult.field}, matches: ${fieldResult.result?.length || 0}`);
      });
    } else {
      console.warn('[RunbookIndexService] ⚠️ NO RESULTS from FlexSearch!');
    }

    // Track document ranks with enhanced scoring
    const documentRanks = new Map<string, number>();

    // Process results with enhanced scoring for content depth
    results.forEach((result: any) => {
      const fieldName = result.field;
      let fieldRank = FIELD_RANKS[fieldName as keyof typeof FIELD_RANKS] || 1.0;
      
      // For deep search, we care MORE about content matches (reverse the bias)
      if (fieldName === "content") {
        fieldRank = 2.0; // Content is 2x more important for AI queries
      } else if (fieldName === "title") {
        fieldRank = 1.5; // Title still matters but less dominant
      }

      result.result.forEach((id: string) => {
        const currentRank = documentRanks.get(id) || 0;
        documentRanks.set(id, currentRank + fieldRank);
      });
    });

    // Sort and limit results
    const sortedResults = Array.from(documentRanks.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, limit)
      .map(([id]) => id);

    console.log('[RunbookIndexService] Final ranked results:', sortedResults.length, 'runbooks');
    console.log('[RunbookIndexService] Top result IDs:', sortedResults.slice(0, 3));
    
    return sortedResults;
  }
}

export default RunbookIndexService;
