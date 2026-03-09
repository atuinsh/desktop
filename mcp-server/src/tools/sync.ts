import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { atuin, atuinOk } from "../cli.js";

export function registerSyncTools(server: McpServer) {
  server.tool(
    "sync",
    "Trigger an Atuin sync — pushes and pulls encrypted history, scripts, dotfiles, and KV data.",
    {
      force: z.boolean().optional().describe("Force a full re-sync"),
    },
    async ({ force }) => {
      const args = ["sync"];
      if (force) args.push("-f");

      const result = await atuin(args, { timeout: 60_000 });
      const output = [result.stdout, result.stderr].filter(Boolean).join("\n");
      return {
        content: [
          {
            type: "text" as const,
            text: result.exitCode === 0
              ? output || "Sync completed successfully"
              : `Sync failed: ${output}`,
          },
        ],
      };
    }
  );

  server.tool(
    "sync_status",
    "Check Atuin sync and connection status.",
    {},
    async () => {
      const result = await atuin(["status"]);
      return {
        content: [{ type: "text" as const, text: result.stdout || result.stderr }],
      };
    }
  );
}
