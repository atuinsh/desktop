import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { atuin } from "../cli.js";

export function registerInfoTools(server: McpServer) {
  server.tool(
    "doctor",
    "Run Atuin diagnostics to check for common issues with shell integration, sync, and configuration.",
    {},
    async () => {
      const result = await atuin(["doctor"]);
      return {
        content: [{ type: "text" as const, text: result.stdout || result.stderr }],
      };
    }
  );

  server.tool(
    "info",
    "Get Atuin system and configuration info — version, data directory, sync address, shell, etc.",
    {},
    async () => {
      const result = await atuin(["info"]);
      return {
        content: [{ type: "text" as const, text: result.stdout || result.stderr }],
      };
    }
  );
}
