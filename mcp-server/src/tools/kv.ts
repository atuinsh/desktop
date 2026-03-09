import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { atuin, atuinOk } from "../cli.js";

export function registerKvTools(server: McpServer) {
  server.tool(
    "kv_get",
    "Get a value from Atuin's encrypted, synced key-value store.",
    {
      key: z.string().describe("The key to retrieve"),
      namespace: z.string().optional().describe("Optional namespace (default: 'default')"),
    },
    async ({ key, namespace }) => {
      const args = ["kv", "get"];
      if (namespace) args.push("-n", namespace);
      args.push(key);

      const result = await atuin(args);
      if (result.exitCode !== 0) {
        return {
          content: [{ type: "text" as const, text: `Key '${key}' not found` }],
        };
      }
      return {
        content: [{ type: "text" as const, text: result.stdout }],
      };
    }
  );

  server.tool(
    "kv_set",
    "Set a key-value pair in Atuin's encrypted, synced KV store. Max 100KiB per entry.",
    {
      key: z.string().describe("The key to set"),
      value: z.string().describe("The value to store (max 100KiB)"),
      namespace: z.string().optional().describe("Optional namespace (default: 'default')"),
    },
    async ({ key, value, namespace }) => {
      const args = ["kv", "set"];
      if (namespace) args.push("-n", namespace);
      args.push("-k", key, value);

      await atuinOk(args);
      return {
        content: [{ type: "text" as const, text: `Set '${key}' = '${value}'` }],
      };
    }
  );
}
