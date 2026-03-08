import { z } from "zod";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { DesktopApiClient } from "../desktop/api.js";

const client = new DesktopApiClient();

export function registerBlockTools(server: McpServer) {
  server.tool(
    "block_execute",
    "Execute a block via the Atuin Desktop app's runtime engine. Requires the desktop app to be running. " +
    "Supports: bash/zsh scripts, HTTP requests, SQL queries (SQLite, Postgres, MySQL, ClickHouse), " +
    "Kubernetes operations, Prometheus queries, and SSH commands.",
    {
      runbook_id: z.string().describe("ID of the runbook containing the block"),
      block_type: z.string().describe(
        "Block type: 'run' (terminal), 'script', 'http', 'sqlite', 'postgres', 'mysql', " +
        "'clickhouse', 'kubernetes', 'prometheus', 'ssh_connect'"
      ),
      props: z.record(z.unknown()).describe(
        "Block-specific properties. Examples:\n" +
        "- run: {type: 'bash', code: 'echo hello'}\n" +
        "- http: {method: 'GET', url: 'https://api.example.com'}\n" +
        "- sqlite: {path: './db.sqlite', query: 'SELECT * FROM users'}\n" +
        "- script: {interpreter: 'python3', code: 'print(42)'}"
      ),
      context: z.record(z.string()).optional().describe("Template variables for the block"),
    },
    async ({ runbook_id, block_type, props, context }) => {
      const available = await client.isAvailable();
      if (!available) {
        return {
          content: [
            {
              type: "text" as const,
              text: "Atuin Desktop app is not running or MCP API is not enabled.\n" +
                "Start the desktop app to use block execution features.\n" +
                "The MCP API runs on http://127.0.0.1:19876 by default.",
            },
          ],
        };
      }

      const block = {
        id: crypto.randomUUID(),
        type: block_type,
        props,
      };

      const result = await client.executeBlock({
        runbookId: runbook_id,
        block,
        context,
      });

      return {
        content: [
          {
            type: "text" as const,
            text: result.success
              ? result.output || "(block executed successfully with no output)"
              : `Block execution failed: ${result.error}`,
          },
        ],
      };
    }
  );

  server.tool(
    "workflow_run",
    "Run a serial workflow (sequence of blocks) from a runbook via the Atuin Desktop app. " +
    "Requires the desktop app to be running.",
    {
      runbook_id: z.string().describe("ID of the runbook to execute"),
      block_ids: z.array(z.string()).optional().describe(
        "Specific block IDs to execute in order. If omitted, runs all executable blocks."
      ),
    },
    async ({ runbook_id, block_ids }) => {
      const available = await client.isAvailable();
      if (!available) {
        return {
          content: [
            {
              type: "text" as const,
              text: "Atuin Desktop app is not running or MCP API is not enabled.",
            },
          ],
        };
      }

      const result = await client.runWorkflow(runbook_id, block_ids ?? []);
      return {
        content: [
          {
            type: "text" as const,
            text: result.success
              ? result.output || "(workflow completed)"
              : `Workflow failed: ${result.error}`,
          },
        ],
      };
    }
  );

  server.tool(
    "workflow_stop",
    "Stop a running workflow in the Atuin Desktop app.",
    {
      runbook_id: z.string().describe("ID of the runbook whose workflow to stop"),
    },
    async ({ runbook_id }) => {
      const available = await client.isAvailable();
      if (!available) {
        return {
          content: [
            { type: "text" as const, text: "Atuin Desktop app is not running." },
          ],
        };
      }

      await client.stopWorkflow(runbook_id);
      return {
        content: [
          { type: "text" as const, text: `Stopped workflow for runbook ${runbook_id}` },
        ],
      };
    }
  );
}
