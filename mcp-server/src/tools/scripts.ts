import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { atuin, atuinOk } from "../cli.js";

export function registerScriptsTools(server: McpServer) {
  server.tool(
    "scripts_list",
    "List all Atuin scripts. Scripts are shareable, syncable shell snippets with template variables.",
    {},
    async () => {
      const result = await atuin(["scripts", "list"]);
      return {
        content: [{ type: "text" as const, text: result.stdout || "(no scripts)" }],
      };
    }
  );

  server.tool(
    "scripts_get",
    "Get the content and details of a specific Atuin script.",
    {
      name: z.string().describe("Name of the script to retrieve"),
    },
    async ({ name }) => {
      const result = await atuin(["scripts", "get", name]);
      return {
        content: [{ type: "text" as const, text: result.stdout || "(script not found)" }],
      };
    }
  );

  server.tool(
    "scripts_run",
    "Execute an Atuin script by name, optionally passing template variables (Jinja-style {{var}}).",
    {
      name: z.string().describe("Name of the script to run"),
      variables: z
        .record(z.string())
        .optional()
        .describe("Template variables as key=value pairs, e.g. {\"host\": \"prod-1\", \"port\": \"8080\"}"),
    },
    async ({ name, variables }) => {
      const args = ["scripts", "run", name];
      if (variables) {
        for (const [key, value] of Object.entries(variables)) {
          args.push("-v", `${key}=${value}`);
        }
      }

      const result = await atuin(args, { timeout: 60_000 });
      const output = [result.stdout, result.stderr].filter(Boolean).join("\n");
      return {
        content: [
          {
            type: "text" as const,
            text: result.exitCode === 0
              ? output || "(script completed with no output)"
              : `Script failed (exit ${result.exitCode}):\n${output}`,
          },
        ],
      };
    }
  );

  server.tool(
    "scripts_new",
    "Create a new Atuin script. The script content is provided as the body. Supports Jinja-style {{variable}} templates.",
    {
      name: z.string().describe("Name for the new script"),
      content: z.string().describe("The shell script content"),
      shebang: z.string().optional().describe("Custom shebang line (default: #!/bin/sh)"),
      from_last: z.boolean().optional().describe("Create script from last executed command instead of content"),
    },
    async ({ name, content, shebang, from_last }) => {
      if (from_last) {
        await atuinOk(["scripts", "new", name, "--last"]);
        return {
          content: [
            { type: "text" as const, text: `Created script '${name}' from last command` },
          ],
        };
      }

      const args = ["scripts", "new", name];
      if (shebang) args.push("--shebang", shebang);

      // atuin scripts new reads from stdin for the script body
      await atuinOk(args, { stdin: content });
      return {
        content: [
          { type: "text" as const, text: `Created script '${name}'` },
        ],
      };
    }
  );
}
