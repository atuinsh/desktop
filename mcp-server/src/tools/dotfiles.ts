import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { atuinOk } from "../cli.js";

export function registerDotfilesTools(server: McpServer) {
  // --- Aliases ---

  server.tool(
    "alias_list",
    "List all Atuin-managed shell aliases (synced across machines).",
    {},
    async () => {
      const output = await atuinOk(["dotfiles", "alias", "list"]);
      return {
        content: [{ type: "text" as const, text: output || "(no aliases)" }],
      };
    }
  );

  server.tool(
    "alias_set",
    "Create or update an Atuin-managed shell alias. Synced and encrypted across machines.",
    {
      name: z.string().describe("Alias name (e.g. 'll')"),
      command: z.string().describe("Command the alias expands to (e.g. 'ls -la')"),
    },
    async ({ name, command }) => {
      await atuinOk(["dotfiles", "alias", "set", name, command]);
      return {
        content: [{ type: "text" as const, text: `Alias '${name}' set to '${command}'` }],
      };
    }
  );

  server.tool(
    "alias_delete",
    "Delete an Atuin-managed shell alias.",
    {
      name: z.string().describe("Alias name to delete"),
    },
    async ({ name }) => {
      await atuinOk(["dotfiles", "alias", "delete", name]);
      return {
        content: [{ type: "text" as const, text: `Alias '${name}' deleted` }],
      };
    }
  );

  // --- Environment Variables ---

  server.tool(
    "var_list",
    "List all Atuin-managed environment variables (synced across machines).",
    {},
    async () => {
      const output = await atuinOk(["dotfiles", "var", "list"]);
      return {
        content: [{ type: "text" as const, text: output || "(no variables)" }],
      };
    }
  );

  server.tool(
    "var_set",
    "Set an Atuin-managed environment variable. Synced and encrypted across machines.",
    {
      name: z.string().describe("Variable name (e.g. 'EDITOR')"),
      value: z.string().describe("Variable value"),
      no_export: z.boolean().optional().describe("If true, variable is not exported to child processes"),
    },
    async ({ name, value, no_export }) => {
      const args = ["dotfiles", "var", "set"];
      if (no_export) args.push("-n");
      args.push(name, value);

      await atuinOk(args);
      return {
        content: [{ type: "text" as const, text: `Variable '${name}' set` }],
      };
    }
  );

  server.tool(
    "var_delete",
    "Delete an Atuin-managed environment variable.",
    {
      name: z.string().describe("Variable name to delete"),
    },
    async ({ name }) => {
      await atuinOk(["dotfiles", "var", "delete", name]);
      return {
        content: [{ type: "text" as const, text: `Variable '${name}' deleted` }],
      };
    }
  );
}
