import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { atuin, atuinOk } from "../cli.js";

export function registerStoreTools(server: McpServer) {
  server.tool(
    "store_status",
    "Check the status of Atuin's local record store (history, scripts, dotfiles, KV).",
    {},
    async () => {
      const result = await atuin(["store", "status"]);
      return {
        content: [{ type: "text" as const, text: result.stdout || result.stderr }],
      };
    }
  );

  server.tool(
    "store_verify",
    "Verify the integrity of the local Atuin record store.",
    {},
    async () => {
      const result = await atuin(["store", "verify"]);
      return {
        content: [
          {
            type: "text" as const,
            text: result.exitCode === 0
              ? result.stdout || "Store integrity verified"
              : `Verification failed: ${result.stderr || result.stdout}`,
          },
        ],
      };
    }
  );

  server.tool(
    "store_rebuild",
    "Rebuild the local Atuin record store from records. Useful for fixing corruption.",
    {
      tag: z.string().optional().describe("Specific store tag to rebuild (e.g. 'history')"),
    },
    async ({ tag }) => {
      const args = ["store", "rebuild"];
      if (tag) args.push(tag);

      const result = await atuin(args, { timeout: 120_000 });
      return {
        content: [
          {
            type: "text" as const,
            text: result.exitCode === 0
              ? result.stdout || "Store rebuilt successfully"
              : `Rebuild failed: ${result.stderr || result.stdout}`,
          },
        ],
      };
    }
  );
}
