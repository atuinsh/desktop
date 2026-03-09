#!/usr/bin/env bun
import { McpServer, ResourceTemplate } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { registerHistoryTools } from "./tools/history.js";
import { registerScriptsTools } from "./tools/scripts.js";
import { registerDotfilesTools } from "./tools/dotfiles.js";
import { registerKvTools } from "./tools/kv.js";
import { registerSyncTools } from "./tools/sync.js";
import { registerStoreTools } from "./tools/store.js";
import { registerInfoTools } from "./tools/info.js";
import { registerRunbookTools } from "./tools/runbooks.js";
import { registerWorkspaceTools } from "./tools/workspaces.js";
import { registerBlockTools } from "./tools/blocks.js";
import { atuinOk } from "./cli.js";
import { listWorkspaces, findRunbook } from "./desktop/fs.js";

const server = new McpServer({
  name: "atuin",
  version: "0.1.0",
});

// ── Tools ──────────────────────────────────────────────────────────

registerHistoryTools(server);
registerScriptsTools(server);
registerDotfilesTools(server);
registerKvTools(server);
registerSyncTools(server);
registerStoreTools(server);
registerInfoTools(server);
registerRunbookTools(server);
registerWorkspaceTools(server);
registerBlockTools(server);

// ── Resources ──────────────────────────────────────────────────────

server.resource(
  "recent-history",
  "atuin://history/recent",
  { description: "Last 50 shell commands from Atuin history" },
  async () => {
    try {
      const output = await atuinOk(["history", "list", "--limit", "50", "--human"]);
      return {
        contents: [{ uri: "atuin://history/recent", mimeType: "text/plain", text: output }],
      };
    } catch (e) {
      return {
        contents: [
          {
            uri: "atuin://history/recent",
            mimeType: "text/plain",
            text: `Error fetching history: ${e}`,
          },
        ],
      };
    }
  }
);

server.resource(
  "scripts",
  "atuin://scripts",
  { description: "All Atuin scripts" },
  async () => {
    try {
      const output = await atuinOk(["scripts", "list"]);
      return {
        contents: [{ uri: "atuin://scripts", mimeType: "text/plain", text: output }],
      };
    } catch (e) {
      return {
        contents: [
          { uri: "atuin://scripts", mimeType: "text/plain", text: `Error: ${e}` },
        ],
      };
    }
  }
);

server.resource(
  "aliases",
  "atuin://dotfiles/aliases",
  { description: "All Atuin-managed shell aliases" },
  async () => {
    try {
      const output = await atuinOk(["dotfiles", "alias", "list"]);
      return {
        contents: [{ uri: "atuin://dotfiles/aliases", mimeType: "text/plain", text: output }],
      };
    } catch (e) {
      return {
        contents: [
          { uri: "atuin://dotfiles/aliases", mimeType: "text/plain", text: `Error: ${e}` },
        ],
      };
    }
  }
);

server.resource(
  "variables",
  "atuin://dotfiles/vars",
  { description: "All Atuin-managed environment variables" },
  async () => {
    try {
      const output = await atuinOk(["dotfiles", "var", "list"]);
      return {
        contents: [{ uri: "atuin://dotfiles/vars", mimeType: "text/plain", text: output }],
      };
    } catch (e) {
      return {
        contents: [
          { uri: "atuin://dotfiles/vars", mimeType: "text/plain", text: `Error: ${e}` },
        ],
      };
    }
  }
);

server.resource(
  "workspaces",
  "atuin://workspaces",
  { description: "All Atuin Desktop workspaces and their runbooks" },
  async () => {
    const workspaces = await listWorkspaces();
    const lines: string[] = [];
    for (const ws of workspaces) {
      lines.push(`## ${ws.name} (${ws.id})`);
      lines.push(`Path: ${ws.path}`);
      for (const rb of ws.runbooks) {
        lines.push(`  - ${rb.name} [${rb.id}] (${rb.blockCount} blocks)`);
      }
      lines.push("");
    }
    return {
      contents: [
        {
          uri: "atuin://workspaces",
          mimeType: "text/plain",
          text: lines.join("\n") || "(no workspaces)",
        },
      ],
    };
  }
);

server.resource(
  "runbook",
  new ResourceTemplate("atuin://runbook/{id}", { list: undefined }),
  { description: "A specific Atuin Desktop runbook by ID or name" },
  async (uri, params) => {
    const id = params.id as string;
    const found = await findRunbook(id);
    if (!found) {
      return {
        contents: [
          {
            uri: uri.href,
            mimeType: "text/plain",
            text: `Runbook '${id}' not found`,
          },
        ],
      };
    }

    const { runbook } = found;
    const lines = [
      `# ${runbook.name}`,
      `ID: ${runbook.id}`,
      `Version: ${runbook.version}`,
      "",
      "## Blocks:",
    ];

    for (const block of runbook.content ?? []) {
      const label = (block.props as Record<string, unknown>)?.name || block.type;
      lines.push(`- [${block.type}] ${label}`);
      if (block.type === "run" || block.type === "script") {
        const code = (block.props as Record<string, unknown>)?.code;
        if (code) lines.push(`  \`\`\`\n  ${String(code)}\n  \`\`\``);
      }
    }

    return {
      contents: [
        { uri: uri.href, mimeType: "text/plain", text: lines.join("\n") },
      ],
    };
  }
);

// ── Start ──────────────────────────────────────────────────────────

const transport = new StdioServerTransport();
await server.connect(transport);
