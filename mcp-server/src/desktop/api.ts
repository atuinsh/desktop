import type { BlockExecutionRequest, BlockExecutionResult } from "../types.js";

const DEFAULT_PORT = 19876;
const DEFAULT_HOST = "127.0.0.1";

/**
 * Client for the Atuin Desktop local HTTP API.
 * The desktop app must be running with the MCP API enabled.
 */
export class DesktopApiClient {
  private baseUrl: string;

  constructor(port = DEFAULT_PORT, host = DEFAULT_HOST) {
    this.baseUrl = `http://${host}:${port}`;
  }

  /**
   * Check if the desktop app's MCP API is available.
   */
  async isAvailable(): Promise<boolean> {
    try {
      const res = await fetch(`${this.baseUrl}/api/health`, {
        signal: AbortSignal.timeout(2000),
      });
      return res.ok;
    } catch {
      return false;
    }
  }

  /**
   * Execute a block via the desktop app's runtime engine.
   */
  async executeBlock(request: BlockExecutionRequest): Promise<BlockExecutionResult> {
    const res = await fetch(`${this.baseUrl}/api/blocks/execute`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
      signal: AbortSignal.timeout(120_000),
    });

    if (!res.ok) {
      const text = await res.text();
      return { success: false, error: `Desktop API error (${res.status}): ${text}` };
    }

    return (await res.json()) as BlockExecutionResult;
  }

  /**
   * Cancel a running block execution.
   */
  async cancelExecution(executionId: string): Promise<void> {
    await fetch(`${this.baseUrl}/api/blocks/cancel`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ executionId }),
      signal: AbortSignal.timeout(5_000),
    });
  }

  /**
   * Run a serial workflow (sequence of blocks).
   */
  async runWorkflow(
    runbookId: string,
    blockIds: string[]
  ): Promise<BlockExecutionResult> {
    const res = await fetch(`${this.baseUrl}/api/workflow/run`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ runbookId, blockIds }),
      signal: AbortSignal.timeout(300_000),
    });

    if (!res.ok) {
      const text = await res.text();
      return { success: false, error: `Workflow error (${res.status}): ${text}` };
    }

    return (await res.json()) as BlockExecutionResult;
  }

  /**
   * Stop a running workflow.
   */
  async stopWorkflow(runbookId: string): Promise<void> {
    await fetch(`${this.baseUrl}/api/workflow/stop`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ runbookId }),
    });
  }
}
