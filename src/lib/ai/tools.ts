import { tool } from "ai";
import { z } from "zod";
import RunbookIndexService from "@/state/runbooks/search";
import Runbook from "@/state/runbooks/runbook";

// Create a singleton search index for AI tools
let globalSearchIndex: RunbookIndexService | null = null;

export function setAISearchIndex(index: RunbookIndexService) {
  globalSearchIndex = index;
}

function getSearchIndex(): RunbookIndexService {
  if (!globalSearchIndex) {
    throw new Error("Search index not initialized. Call setAISearchIndex first.");
  }
  return globalSearchIndex;
}

/**
 * Deep content search for runbooks - optimized for AI semantic queries
 * Uses broader matching and prioritizes content over titles
 */
export const searchWorkspaceTool = tool({
  description: 'Deep search for runbooks by content, concepts, and context. Searches both titles and full content with semantic matching. Content matches are prioritized over title matches. Use this to find runbooks related to technologies, procedures, or concepts.',
  parameters: z.object({
    query: z.string().describe('The search query - can be keywords, concepts, or natural language (e.g., "postgres connection", "deployment steps", "docker compose")'),
    limit: z.number().optional().describe('Maximum number of results to return (default: 10)'),
  }),
  execute: async ({ query, limit = 10 }) => {
    console.log('[AI Tool] search_workspace called (deep search):', { query, limit });
    try {
      const index = getSearchIndex();
      
      // Also log what runbooks are actually indexed
      console.log('[AI Tool] Total runbooks in index:', index['stored']?.size || 0);
      
      // Use deep search instead of regular search
      const resultIds = await index.deepSearchRunbooks(query, limit);
      console.log('[AI Tool] search_workspace found:', resultIds.length, 'results');
      console.log('[AI Tool] Result IDs:', resultIds);
      
      // Fetch runbook details for each ID
      const runbooks = await Promise.all(
        resultIds.map(async (id) => {
          try {
            const runbook = await Runbook.load(id);
            if (!runbook) return null;
            
            return {
              id: runbook.id,
              name: runbook.name,
              workspaceId: runbook.workspaceId,
              contentPreview: runbook.content.substring(0, 200) + (runbook.content.length > 200 ? '...' : ''),
              created: runbook.created,
              updated: runbook.updated,
            };
          } catch (error) {
            console.error(`Failed to fetch runbook ${id}:`, error);
            return null;
          }
        })
      );
      
      // Filter out nulls
      const validRunbooks = runbooks.filter(r => r !== null);
      
      console.log('[AI Tool] search_workspace returning:', validRunbooks.length, 'runbooks');
      return {
        success: true,
        results: validRunbooks,
        totalFound: resultIds.length,
      };
    } catch (error) {
      console.error('[AI Tool] search_workspace error:', error);
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Search failed',
      };
    }
  },
});

/**
 * Get the full content of a specific runbook by ID
 */
export const getRunbookContentTool = tool({
  description: 'Get the full content of a specific runbook by its ID. Use this after searching to read the actual runbook content and blocks.',
  parameters: z.object({
    runbookId: z.string().describe('The ID of the runbook to retrieve'),
  }),
  execute: async ({ runbookId }) => {
    console.log('[AI Tool] get_runbook_content called:', { runbookId });
    try {
      const runbook = await Runbook.load(runbookId);
      
      if (!runbook) {
        console.warn('[AI Tool] get_runbook_content: runbook not found');
        return {
          success: false,
          error: `Runbook with ID ${runbookId} not found`,
        };
      }
      
      console.log('[AI Tool] get_runbook_content found:', runbook.name);
      return {
        success: true,
        id: runbook.id,
        name: runbook.name,
        content: runbook.content,
        workspaceId: runbook.workspaceId,
        created: runbook.created,
        updated: runbook.updated,
      };
    } catch (error) {
      console.error('[AI Tool] get_runbook_content error:', error);
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Failed to get runbook content',
      };
    }
  },
});


