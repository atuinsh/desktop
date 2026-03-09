import type { AtuinCliResult } from "./types.js";

const DEFAULT_TIMEOUT = 30_000;

/**
 * Execute an atuin CLI command and return stdout/stderr/exitCode.
 */
export async function atuin(
  args: string[],
  options: { timeout?: number; stdin?: string } = {}
): Promise<AtuinCliResult> {
  const { timeout = DEFAULT_TIMEOUT, stdin } = options;

  const proc = Bun.spawn(["atuin", ...args], {
    stdout: "pipe",
    stderr: "pipe",
    stdin: stdin ? "pipe" : undefined,
  });

  if (stdin && proc.stdin) {
    proc.stdin.write(new TextEncoder().encode(stdin));
    proc.stdin.end();
  }

  const timeoutPromise = new Promise<never>((_, reject) =>
    setTimeout(() => {
      proc.kill();
      reject(new Error(`atuin ${args[0]} timed out after ${timeout}ms`));
    }, timeout)
  );

  const [stdout, stderr, exitCode] = await Promise.race([
    Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ]),
    timeoutPromise,
  ]);

  return { stdout: stdout.trim(), stderr: stderr.trim(), exitCode };
}

/**
 * Execute atuin and return stdout, throwing on non-zero exit.
 */
export async function atuinOk(
  args: string[],
  options: { timeout?: number; stdin?: string } = {}
): Promise<string> {
  const result = await atuin(args, options);
  if (result.exitCode !== 0) {
    throw new Error(
      `atuin ${args[0]} failed (exit ${result.exitCode}): ${result.stderr || result.stdout}`
    );
  }
  return result.stdout;
}
