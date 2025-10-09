import { QueryResult } from "@/lib/blocks/common/database";
import { invoke } from "@tauri-apps/api/core";

export interface MultiQueryResult extends QueryResult {
  queryCount?: number;
  executedQueries?: string[];
}

const executeSingleQuery = async (uri: string, query: string): Promise<QueryResult> => {
  const firstWord = query.trim().split(/\s+/)[0].toLowerCase();

  if (firstWord === "select") {
    let start = performance.now();

    let res = await invoke<any>("postgres_query", {
      uri,
      query,
    });

    return {
      time: new Date(),
      rows: res.rows,
      columns: res.columns,
      rowsAffected: null,
      lastInsertID: null,
      duration: performance.now() - start,
    };
  }

  let start = performance.now();
  let res = await invoke<any>("postgres_execute", { uri, query });

  return {
    time: new Date(),
    rows: null,
    columns: null,
    rowsAffected: res.rowsAffected,
    lastInsertID: null,
    duration: performance.now() - start,
  };
};

export const runQuery = async (
  uri: string,
  query: string,
): Promise<MultiQueryResult> => {
  // Split queries by semicolon and filter out empty statements
  const statements = query.split(";")
    .map(q => q.trim())
    .filter(q => q.length > 0);

  if (statements.length === 0) {
    throw new Error("No query to run");
  }

  // For single query, maintain backward compatibility
  if (statements.length === 1) {
    return await executeSingleQuery(uri, statements[0]);
  }

  // Execute multiple queries sequentially
  const results: QueryResult[] = [];
  const executedQueries: string[] = [];
  let totalDuration = 0;
  let lastResultWithData: QueryResult | null = null;

  for (const statement of statements) {
    try {
      const result = await executeSingleQuery(uri, statement);
      results.push(result);
      executedQueries.push(statement);
      totalDuration += result.duration;

      // Keep track of the last query that returned data (SELECT results)
      if (result.rows && result.columns) {
        lastResultWithData = result;
      }
    } catch (error) {
      // If a query fails, we still want to show what was executed successfully
      throw new Error(`Query ${results.length + 1} failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  // Return the last query with data, or the last executed query if none had data
  const finalResult = lastResultWithData || results[results.length - 1];

  return {
    ...finalResult,
    duration: totalDuration,
    queryCount: statements.length,
    executedQueries,
  };
};
