import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import {
  listWorkspaces,
  findRunbook,
  readRunbook,
  writeRunbook,
  deleteRunbook,
  generateId,
} from "../desktop/fs.js";
import type { RunbookFile, Block } from "../types.js";

const blockSchema = z.object({
  type: z.string(),
  props: z.record(z.unknown()).optional().default({}),
  content: z.array(z.any()).optional(),
  children: z.array(z.any()).optional(),
});

export function registerRunbookTools(server: McpServer) {
  server.tool(
    "runbook_list",
    "List all Atuin Desktop runbooks across all workspaces. Returns name, ID, block count, and workspace info.",
    {},
    async () => {
      const workspaces = await listWorkspaces();
      if (workspaces.length === 0) {
        return {
          content: [{ type: "text" as const, text: "No workspaces found in ~/Documents/Atuin Runbooks/" }],
        };
      }

      const lines: string[] = [];
      for (const ws of workspaces) {
        lines.push(`## Workspace: ${ws.name} (${ws.id})`);
        if (ws.runbooks.length === 0) {
          lines.push("  (no runbooks)");
        }
        for (const rb of ws.runbooks) {
          lines.push(`  - ${rb.name} [${rb.id}] (${rb.blockCount} blocks)`);
        }
        lines.push("");
      }

      return {
        content: [{ type: "text" as const, text: lines.join("\n") }],
      };
    }
  );

  server.tool(
    "runbook_get",
    "Read a specific Atuin Desktop runbook by name or ID. Returns the full runbook content including all blocks.",
    {
      name_or_id: z.string().describe("Runbook name or UUID"),
    },
    async ({ name_or_id }) => {
      const found = await findRunbook(name_or_id);
      if (!found) {
        return {
          content: [{ type: "text" as const, text: `Runbook '${name_or_id}' not found` }],
        };
      }

      const { runbook, workspace } = found;
      const summary = [
        `# ${runbook.name}`,
        `ID: ${runbook.id}`,
        `Workspace: ${workspace.name}`,
        `Version: ${runbook.version}`,
        `Blocks: ${runbook.content?.length ?? 0}`,
        "",
        "## Blocks:",
      ];

      for (const block of runbook.content ?? []) {
        const label = (block.props as Record<string, unknown>)?.name || block.type;
        summary.push(`  - [${block.type}] ${label} (${block.id})`);
        if (block.type === "run" || block.type === "script") {
          const code = (block.props as Record<string, unknown>)?.code;
          if (code) summary.push(`    code: ${String(code).slice(0, 200)}`);
        }
      }

      return {
        content: [{ type: "text" as const, text: summary.join("\n") }],
      };
    }
  );

  server.tool(
    "runbook_create",
    "Create a new Atuin Desktop runbook in a workspace. Provide blocks as an array of {type, props} objects.",
    {
      name: z.string().describe("Runbook name"),
      workspace_name_or_id: z.string().describe("Target workspace name or ID"),
      blocks: z.array(blockSchema).optional().describe(
        "Array of blocks. Each block has: type (e.g. 'run', 'script', 'heading', 'paragraph', 'http', 'var', 'env', 'directory'), " +
        "props (type-specific, e.g. {code: 'echo hi', type: 'bash'} for run blocks). " +
        "Omit for an empty runbook."
      ),
    },
    async ({ name, workspace_name_or_id, blocks }) => {
      const workspaces = await listWorkspaces();
      const ws = workspaces.find(
        (w) =>
          w.id === workspace_name_or_id ||
          w.name.toLowerCase() === workspace_name_or_id.toLowerCase()
      );

      if (!ws) {
        return {
          content: [
            { type: "text" as const, text: `Workspace '${workspace_name_or_id}' not found` },
          ],
        };
      }

      const runbook: RunbookFile = {
        id: generateId(),
        name,
        version: 1,
        content: (blocks ?? []).map((b) => ({
          id: generateId(),
          type: b.type,
          props: b.props ?? {},
          content: b.content ?? [],
          children: b.children ?? [],
        })),
      };

      const filePath = await writeRunbook(ws.path, runbook);
      return {
        content: [
          {
            type: "text" as const,
            text: `Created runbook '${name}' (${runbook.id}) at ${filePath}`,
          },
        ],
      };
    }
  );

  server.tool(
    "runbook_update",
    "Update an existing Atuin Desktop runbook — change its name or replace its blocks.",
    {
      name_or_id: z.string().describe("Current runbook name or ID"),
      new_name: z.string().optional().describe("New name for the runbook"),
      blocks: z.array(blockSchema).optional().describe("New block array to replace current content"),
    },
    async ({ name_or_id, new_name, blocks }) => {
      const found = await findRunbook(name_or_id);
      if (!found) {
        return {
          content: [{ type: "text" as const, text: `Runbook '${name_or_id}' not found` }],
        };
      }

      const { runbook, path, workspace } = found;

      if (new_name) runbook.name = new_name;
      if (blocks) {
        runbook.content = blocks.map((b) => ({
          id: generateId(),
          type: b.type,
          props: b.props ?? {},
          content: b.content ?? [],
          children: b.children ?? [],
        }));
      }

      // If name changed, write to new path and delete old
      const newPath = await writeRunbook(workspace.path, runbook);
      if (newPath !== path) {
        await deleteRunbook(path).catch(() => {});
      }

      return {
        content: [
          { type: "text" as const, text: `Updated runbook '${runbook.name}' (${runbook.id})` },
        ],
      };
    }
  );

  server.tool(
    "runbook_delete",
    "Delete an Atuin Desktop runbook by name or ID.",
    {
      name_or_id: z.string().describe("Runbook name or UUID to delete"),
    },
    async ({ name_or_id }) => {
      const found = await findRunbook(name_or_id);
      if (!found) {
        return {
          content: [{ type: "text" as const, text: `Runbook '${name_or_id}' not found` }],
        };
      }

      await deleteRunbook(found.path);
      return {
        content: [
          { type: "text" as const, text: `Deleted runbook '${found.runbook.name}' (${found.runbook.id})` },
        ],
      };
    }
  );
}
