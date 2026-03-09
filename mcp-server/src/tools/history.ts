import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { atuin, atuinOk } from "../cli.js";

export function registerHistoryTools(server: McpServer) {
  server.tool(
    "history_search",
    "Search shell history with filters. Supports prefix, fulltext, fuzzy, and skim modes.",
    {
      query: z.string().describe("Search query (supports wildcards * or %)"),
      cwd: z.string().optional().describe("Filter by working directory"),
      exit: z.number().optional().describe("Filter by exit code (e.g. 0 for success)"),
      before: z.string().optional().describe("Only commands before this date/time"),
      after: z.string().optional().describe("Only commands after this date/time"),
      limit: z.number().optional().default(25).describe("Max results (default 25)"),
      offset: z.number().optional().describe("Skip first N results"),
      reverse: z.boolean().optional().describe("Reverse chronological order"),
    },
    async ({ query, cwd, exit, before, after, limit, offset, reverse }) => {
      const args = ["search", query, "--cmd-only"];
      if (cwd) args.push("--cwd", cwd);
      if (exit !== undefined) args.push("--exit", String(exit));
      if (before) args.push("--before", before);
      if (after) args.push("--after", after);
      if (limit) args.push("--limit", String(limit));
      if (offset) args.push("--offset", String(offset));
      if (reverse) args.push("--reverse");

      const result = await atuin(args);
      return {
        content: [{ type: "text" as const, text: result.stdout || "(no results)" }],
      };
    }
  );

  server.tool(
    "history_list",
    "List recent shell history entries with full context (time, directory, duration, exit code).",
    {
      limit: z.number().optional().default(25).describe("Max entries (default 25)"),
      offset: z.number().optional().describe("Skip first N entries"),
      cwd: z.string().optional().describe("Filter by working directory"),
      session: z.string().optional().describe("Filter by session ID"),
      reverse: z.boolean().optional().describe("Reverse order (oldest first)"),
    },
    async ({ limit, offset, cwd, session, reverse }) => {
      const args = ["history", "list", "--human"];
      if (limit) args.push("--limit", String(limit));
      if (offset) args.push("--offset", String(offset));
      if (cwd) args.push("--cwd", cwd);
      if (session) args.push("--session", session);
      if (reverse) args.push("--reverse");

      const result = await atuin(args);
      return {
        content: [{ type: "text" as const, text: result.stdout || "(no history)" }],
      };
    }
  );

  server.tool(
    "history_last",
    "Get the most recently executed shell command.",
    {},
    async () => {
      const result = await atuin(["history", "last", "--human"]);
      return {
        content: [{ type: "text" as const, text: result.stdout || "(no history)" }],
      };
    }
  );

  server.tool(
    "history_stats",
    "Get shell usage statistics — most used commands, total count, unique commands. Optionally for a specific date.",
    {
      date: z.string().optional().describe("Date for stats (e.g. 'last friday', '2024-01-01'). Omit for overall stats."),
    },
    async ({ date }) => {
      const args = ["stats"];
      if (date) {
        args.push(date);
      } else {
        args.push("all");
      }

      const result = await atuin(args);
      return {
        content: [{ type: "text" as const, text: result.stdout || "(no stats)" }],
      };
    }
  );

  server.tool(
    "history_delete",
    "Delete a specific command from shell history by its ID.",
    {
      id: z.string().describe("The history entry ID to delete"),
    },
    async ({ id }) => {
      await atuinOk(["history", "delete", "--id", id]);
      return {
        content: [{ type: "text" as const, text: `Deleted history entry ${id}` }],
      };
    }
  );
}
