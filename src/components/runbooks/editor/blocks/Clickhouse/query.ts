import { QueryResult } from "@/lib/blocks/common/database";
import { createClient, ClickHouseClient } from "@clickhouse/client-web";

export interface MultiQueryResult extends QueryResult {
  queryCount?: number;
  executedQueries?: string[];
}

const runSelect = async (
  client: ClickHouseClient,
  query: string,
): Promise<QueryResult> => {
  let res = await client.query({
    query: query,
  });

  let data = await res.json();

  // @ts-ignore
  const columns = data?.meta?.map((col) => col.name);
  // @ts-ignore
  const rows = data?.data.map((obj) => columns.map((key) => obj[key]));

  return {
    time: new Date(),
    rows: rows,
    // @ts-ignore
    rowsRead: data.statistics.rows_read,
    // @ts-ignore
    bytesRead: data.statistics.bytes_read,
    // @ts-ignore
    columns: data.meta,
    rowsAffected: 0,
    lastInsertID: null,
    // @ts-ignore
    duration: data.statistics.elapsed,
  };
};

const executeSingleQuery = async (uri: string, query: string): Promise<QueryResult> => {
  let client = createClient({
    url: uri,
  });

  const firstWord = query.trim().split(/\s+/)[0].toLowerCase();

  if (firstWord === "select") {
    return await runSelect(client, query);
  }

  let start = performance.now();
  await client.exec({ query });

  return {
    time: new Date(),
    rows: null,
    columns: null,
    rowsAffected: 0,
    lastInsertID: null,
    duration: performance.now() - start,
  };
};

// Right now we're using the web clickhouse client
// In the future, I want to use the native Rust client
// Unfortunately, the official client doesn't support the native protocol, and seems
// a bit naive. We'd also need to write a lot of glue code to make the deserialization
// work OK with the frontend. Looks like there's some raw block streaming stuff maybe?
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
