import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { listWorkspaces, createWorkspace } from "../desktop/fs.js";

export function registerWorkspaceTools(server: McpServer) {
  server.tool(
    "workspace_list",
    "List all Atuin Desktop workspaces with their runbook counts.",
    {},
    async () => {
      const workspaces = await listWorkspaces();
      if (workspaces.length === 0) {
        return {
          content: [
            { type: "text" as const, text: "No workspaces found in ~/Documents/Atuin Runbooks/" },
          ],
        };
      }

      const lines = workspaces.map(
        (ws) => `- ${ws.name} [${ws.id}] — ${ws.runbooks.length} runbooks (${ws.path})`
      );

      return {
        content: [{ type: "text" as const, text: lines.join("\n") }],
      };
    }
  );

  server.tool(
    "workspace_get",
    "Get details of a specific workspace by name or ID, including all its runbooks.",
    {
      name_or_id: z.string().describe("Workspace name or UUID"),
    },
    async ({ name_or_id }) => {
      const workspaces = await listWorkspaces();
      const ws = workspaces.find(
        (w) =>
          w.id === name_or_id ||
          w.name.toLowerCase() === name_or_id.toLowerCase()
      );

      if (!ws) {
        return {
          content: [{ type: "text" as const, text: `Workspace '${name_or_id}' not found` }],
        };
      }

      const lines = [
        `# ${ws.name}`,
        `ID: ${ws.id}`,
        `Path: ${ws.path}`,
        `Runbooks: ${ws.runbooks.length}`,
        "",
      ];

      for (const rb of ws.runbooks) {
        lines.push(`  - ${rb.name} [${rb.id}] (${rb.blockCount} blocks)`);
      }

      return {
        content: [{ type: "text" as const, text: lines.join("\n") }],
      };
    }
  );

  server.tool(
    "workspace_create",
    "Create a new Atuin Desktop workspace. Creates a directory with an atuin.toml config.",
    {
      name: z.string().describe("Workspace name"),
    },
    async ({ name }) => {
      const ws = await createWorkspace(name);
      return {
        content: [
          {
            type: "text" as const,
            text: `Created workspace '${ws.name}' (${ws.id}) at ${ws.path}`,
          },
        ],
      };
    }
  );
}
