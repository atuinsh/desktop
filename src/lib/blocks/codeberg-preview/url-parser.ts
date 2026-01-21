export type CodebergUrlType = "repo" | "pr" | "issue" | "code";

export interface ParsedCodebergUrl {
  type: CodebergUrlType;
  owner: string;
  repo: string;
  prNumber?: number;
  issueNumber?: number;
  branch?: string;
  filePath?: string;
  lineStart?: number;
  lineEnd?: number;
}

const CODEBERG_URL_REGEX = /^https?:\/\/(www\.)?codeberg\.org\/([^/]+)\/([^/]+)(\/.*)?$/;

export function isCodebergUrl(text: string): boolean {
  return CODEBERG_URL_REGEX.test(text.trim());
}

export function parseCodebergUrl(url: string): ParsedCodebergUrl | null {
  const trimmedUrl = url.trim();
  const match = trimmedUrl.match(CODEBERG_URL_REGEX);

  if (!match) {
    return null;
  }

  const owner = match[2];
  const repo = match[3];
  const rest = match[4] || "";

  // Check for PR URL: /pulls/{number}
  const prMatch = rest.match(/^\/pulls\/(\d+)/);
  if (prMatch) {
    return {
      type: "pr",
      owner,
      repo,
      prNumber: parseInt(prMatch[1], 10),
    };
  }

  // Check for issue URL: /issues/{number}
  const issueMatch = rest.match(/^\/issues\/(\d+)/);
  if (issueMatch) {
    return {
      type: "issue",
      owner,
      repo,
      issueNumber: parseInt(issueMatch[1], 10),
    };
  }

  // Check for code URL: /src/branch/{branch}/{path} or /src/commit/{hash}/{path}
  // Line numbers: #L{start}-L{end} or #L{start}
  const codeMatch = rest.match(/^\/src\/(?:branch|commit|tag)\/([^/]+)\/(.+?)(?:#L(\d+)(?:-L(\d+))?)?$/);
  if (codeMatch) {
    const result: ParsedCodebergUrl = {
      type: "code",
      owner,
      repo,
      branch: codeMatch[1],
      filePath: codeMatch[2],
    };

    if (codeMatch[3]) {
      result.lineStart = parseInt(codeMatch[3], 10);
    }
    if (codeMatch[4]) {
      result.lineEnd = parseInt(codeMatch[4], 10);
    }

    return result;
  }

  // Default to repo URL
  const isRepoPath =
    rest === "" ||
    rest === "/" ||
    rest.match(/^\/(src|commits|branches|tags|releases|issues|pulls|activity|settings)?(\/?.*)$/);

  if (isRepoPath) {
    return {
      type: "repo",
      owner,
      repo,
    };
  }

  return {
    type: "repo",
    owner,
    repo,
  };
}
